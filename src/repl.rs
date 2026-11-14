//! Interactive frontend: history is validated but never replayed.
use std::{collections::{HashMap,HashSet},io::{self,BufRead,Write,IsTerminal},path::PathBuf,sync::Arc};
use crate::{ast::*,parser::{Parser,Lexer,TokenKind},runtime::{actor::{SignalRouter,RuntimeValue},work_stealing::WorkStealingPool},semantic::{packages::PackageManager,type_checker::TypeChecker,dag::CallGraphValidator,termination::TerminationChecker}};
const ENTRY:&str="\u{0}repl_entry";
pub struct Session{
    packages:PackageManager,
    imports:Vec<ImportDecl>,
    declarations:Vec<TopLevelDecl>,
    statements:Vec<Statement>,
    values:HashMap<String,RuntimeValue>,
    types:HashMap<String,TypeSpec>,
}
impl Session{
    pub fn new(base:impl Into<PathBuf>)->Self{Self{packages:PackageManager::new(base.into()),imports:vec![],declarations:vec![],statements:vec![],values:HashMap::new(),types:HashMap::new()}}
    fn candidate(&self,source:&str)->Result<(Program,usize),String>{
        let mut imports=self.imports.clone();let mut declarations=self.declarations.clone();let mut statements=self.statements.clone();
        match Parser::new(source).and_then(|mut p|p.parse_program()){
            Ok(p)=>{imports.extend(p.imports);declarations.extend(p.declarations.into_iter().filter(|d|!matches!(d,TopLevelDecl::Import(_))));},
            Err(_)=>{
                let wrapper=format!("routine __repl_input() {{\n{}\n}}",source);
                let p=Parser::new(&wrapper).and_then(|mut p|p.parse_program()).map_err(|e|e.to_string())?;
                let Some(TopLevelDecl::Routine(r))=p.declarations.into_iter().next()else{unreachable!()};
                statements.extend(r.body);
            }
        }
        let mut names=HashSet::new();
        for d in &declarations{
            let n=match d{TopLevelDecl::Const(c)=>&c.name,TopLevelDecl::Type(t)=>&t.name,TopLevelDecl::Func(f)=>&f.name,TopLevelDecl::Routine(r)=>&r.name,TopLevelDecl::Loop(l)=>&l.name,_=>continue};
            if !names.insert(n.clone()){return Err(format!("declaration {} already exists; use :reset to start a new session",n));}
        }
        declarations.push(TopLevelDecl::Routine(RoutineDecl{documentation:String::new(),name:ENTRY.into(),type_params:vec![],params:vec![],body:statements}));
        Ok((Program{imports,declarations},self.statements.len()))
    }
    fn prepare(&self,source:&str,persist:bool)->Result<(Program,Program,HashMap<String,TypeSpec>,usize,PackageManager),String>{
        let (raw,start)=self.candidate(source)?;
        let mut packages=self.packages.clone();
        let program=packages.resolve_program(raw.clone()).map_err(|e|e.to_string())?;
        CallGraphValidator::new().build_and_validate(&program).map_err(|e|e.to_string())?;
        let mut checker=TypeChecker::new();checker.check_program(&program).map_err(|e|e.to_string())?;
        let types=checker.repl_binding_types(persist).map_err(|e|e.to_string())?;
        TerminationChecker::verify_program(&program).map_err(|e|e.to_string())?;
        Ok((raw,program,types,start,packages))
    }
    pub fn submit(&mut self,source:&str)->Result<Vec<RuntimeValue>,String>{
        let (mut raw,program,types,start,packages)=self.prepare(source,true)?;
        let router=Arc::new(SignalRouter::default());router.register_enums(&program);
        let mut body=None;
        for d in &program.declarations{match d{
            TopLevelDecl::Func(f)=>router.register_function(f.clone()),
            TopLevelDecl::Routine(r) if r.name==ENTRY=>body=Some(r.body.clone()),
            TopLevelDecl::Routine(r)=>{if !router.register_routine_with_params(&r.name,r.params.clone(),r.body.clone()){return Err(format!("duplicate routine {}",r.name));}},
            TopLevelDecl::Loop(l)=>{if !router.register_loop(&l.name,l.body.clone()){return Err(format!("duplicate loop {}",l.name));}},
            _=>{}
        }}
        let mut values=self.values.clone();
        let mut runtime_types=self.types.iter().map(|(n,t)|(n.clone(),crate::documentation::type_signature(t))).collect();
        let registry=crate::semantic::adt::Constructors::new(&program);
        for value in values.values_mut(){normalize_enums(value,&registry);}
        let body=body.expect("entry routine");
        let results=if body.len()==start{vec![]}else{
            WorkStealingPool::for_repl().execute_repl(&body[start..],&router,&mut runtime_types,&mut values)?
        };
        let TopLevelDecl::Routine(entry)=raw.declarations.pop().expect("entry")else{unreachable!()};
        self.imports=raw.imports;self.declarations=raw.declarations;self.statements=entry.body;self.values=values;self.types=types;self.packages=packages;
        Ok(results)
    }
    pub fn bindings(&self)->Vec<(String,String,RuntimeValue)>{
        let mut values=self.values.iter().map(|(n,v)|(n.clone(),crate::documentation::type_signature(&self.types[n]),v.clone())).collect::<Vec<_>>();values.sort_by(|a,b|a.0.cmp(&b.0));values
    }
    pub fn expression_type(&self,source:&str)->Result<String,String>{
        let mut index=0;let name=loop{let n=format!("__repl_type_{}",index);if !self.types.contains_key(&n){break n;}index+=1;};
        let (_,_,types,_,_)=self.prepare(&format!("let {} = ({})",name,source),false)?;
        types.get(&name).map(crate::documentation::type_signature).ok_or_else(||"expected an expression".into())
    }
}
fn incomplete(source:&str)->bool{
    let mut lexer=Lexer::new(source);let mut depth=0;let mut last=None;let mut docs_only=true;let mut has_docs=false;
    loop{match lexer.next_token(){
        Ok(t)=>match t.kind{
            TokenKind::Eof=>break,
            TokenKind::LBrace|TokenKind::LParen|TokenKind::LBracket=>{depth+=1;docs_only=false;last=None;},
            TokenKind::RBrace|TokenKind::RParen|TokenKind::RBracket=>{depth-=1;docs_only=false;last=None;},
            TokenKind::DocComment(_)=>{has_docs=true;},
            kind=>{docs_only=false;last=Some(kind);}
        },
        Err(e)=>return e.to_string().contains("Unterminated")||e.to_string().contains("Unclosed"),
    }}
    depth>0 || docs_only&&has_docs || matches!(last,Some(TokenKind::Assign|TokenKind::FatArrow|TokenKind::Arrow|TokenKind::Plus|TokenKind::Minus|TokenKind::Star|TokenKind::Slash|TokenKind::PipeForward|TokenKind::PipeBackward|TokenKind::ComposeForward|TokenKind::ComposeArrow))
}
pub fn run_cli(base:PathBuf)->Result<(),Box<dyn std::error::Error>>{
    let interactive=io::stdin().is_terminal();
    if interactive{println!("Lia REPL — :help for commands, :quit to exit.");}
    let stdin=io::stdin();let mut input=stdin.lock();let mut session=Session::new(base.clone());let mut pending=String::new();
    loop{
        if interactive{print!("{}",if pending.is_empty(){"lia> "}else{"...> "});io::stdout().flush()?;}
        let mut line=String::new();
        if input.read_line(&mut line)?==0{if !pending.trim().is_empty(){eprintln!("Error: incomplete input at end of stream");}break;}
        let command=line.trim();
        if command==":quit" || command==":q"{break;}
        if command==":abort"{pending.clear();continue;}
        if pending.is_empty() && command.starts_with(':'){
            match command{
                ":quit"|":q"=>break,
                ":help"=>println!(":help  :bindings  :type EXPR  :reset  :abort  :quit\nEnter Lia declarations, statements or expressions. Ctrl-C exits the session."),
                ":reset"=>{session=Session::new(base.clone());println!("Session reset.");},
                ":bindings"=>for (n,t,v) in session.bindings(){println!("{}: {} = {}",n,t,v);},
                _ if command.starts_with(":type ")=>match session.expression_type(command[6..].trim()){Ok(t)=>println!("{}",t),Err(e)=>eprintln!("Error: {}",e)},
                _=>eprintln!("Error: unknown command; use :help")
            }
            continue;
        }
        if pending.is_empty() && command.is_empty(){continue;}
        pending.push_str(&line);
        if incomplete(&pending){continue;}
        match session.submit(&pending){Ok(values)=>for value in values{println!("{}",value);},Err(e)=>eprintln!("Error: {}",e)}
        pending.clear();
    }
    Ok(())
}

fn normalize_enums(value:&mut RuntimeValue,registry:&crate::semantic::adt::Constructors){
    match value{
        RuntimeValue::Enum{enum_name,fields,..}=>{if let Some(name)=registry.aliases.get(enum_name){*enum_name=name.clone();}for v in fields{normalize_enums(v,registry);}},
        RuntimeValue::List(v)=>for v in v{normalize_enums(v,registry);},
        RuntimeValue::Dict(v)=>for v in v.values_mut(){normalize_enums(v,registry);},
        RuntimeValue::Closure{captures,..}=>for v in captures.values_mut(){normalize_enums(v,registry);},
        _=>{}
    }
}


// Exit immediately on Ctrl-C: stdin can be blocked and actor loops can remain
// active. _exit is async-signal-safe and deliberately skips locks/destructors.
#[cfg(unix)]
pub fn install_interrupt_handler()->io::Result<()>{
    extern "C" fn interrupt(_:libc::c_int){unsafe{libc::_exit(130)}}
    let previous=unsafe{libc::signal(libc::SIGINT,interrupt as *const () as libc::sighandler_t)};
    if previous==libc::SIG_ERR{Err(io::Error::last_os_error())}else{Ok(())}
}
#[cfg(not(unix))]
pub fn install_interrupt_handler()->io::Result<()>{Ok(())}
