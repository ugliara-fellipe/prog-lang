use std::path::{Path,PathBuf};
use std::process::{Command,Output};
use std::sync::atomic::{AtomicUsize,Ordering};
use lia::{ast::*,documentation,parser::{Lexer,Parser,TokenKind},semantic::{packages::PackageManager,type_checker::TypeChecker}};

fn parse(source:&str)->Program{Parser::new(source).unwrap().parse_program().unwrap()}
struct Directory(PathBuf);
impl Directory{
    fn new()->Self{
        static NEXT:AtomicUsize=AtomicUsize::new(0);
        let dir=std::env::temp_dir().join(format!("lia-docs-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();Self(dir)
    }
    fn write(&self,name:&str,source:&str)->PathBuf{let p=self.0.join(name);std::fs::write(&p,source).unwrap();p}
}
impl Drop for Directory{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn cli(args:&[&Path])->Output{Command::new(env!("CARGO_BIN_EXE_lia")).args(args).output().unwrap()}
fn success(out:&Output){assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));}

#[test]
fn lexer_distinguishes_docstrings_from_ordinary_comments_and_preserves_indentation(){
    let source="// ordinary\n//// also ordinary\n/* /// ignored */\n/// Ação e Unicode ç\r\n///     indented\n///\nfunc f(){}";
    let mut lex=Lexer::new(source);
    let first=lex.next_token().unwrap();assert_eq!((first.line,first.col),(4,1));assert_eq!(first.kind,TokenKind::DocComment("Ação e Unicode ç".into()));
    assert_eq!(lex.next_token().unwrap().kind,TokenKind::DocComment("    indented".into()));
    assert_eq!(lex.next_token().unwrap().kind,TokenKind::DocComment(String::new()));
    assert_eq!(lex.next_token().unwrap().kind,TokenKind::Func);
}
#[test]
fn documentation_attaches_to_each_supported_declaration(){
    let source="/// constant\nconst N=1\n/// record\ntype R {\n/// field\nx:Int}\n/// enum\nenum E {\n/// variant\nItem(Int),\n/// absent\nEmpty}\n/// sum\ntype S=\n/// yes\nYes(Int)|\n/// no\nNo\n/// function\nfunc f()->Int{N}\n/// routine\nroutine main(){\n/// local\nconst X=N}\n/// loop\nloop Worker(){terminate}";
    let p=parse(source);
    let TopLevelDecl::Const(c)=&p.declarations[0]else{panic!()};assert_eq!(c.documentation,"constant");
    let TopLevelDecl::Type(t)=&p.declarations[1]else{panic!()};assert_eq!(t.documentation,"record");assert_eq!(t.fields[0].documentation,"field");
    let TopLevelDecl::Type(t)=&p.declarations[2]else{panic!()};assert_eq!(t.documentation,"enum");assert_eq!(t.variants[0].documentation,"variant");assert_eq!(t.variants[1].documentation,"absent");
    let TopLevelDecl::Type(t)=&p.declarations[3]else{panic!()};assert_eq!(t.documentation,"sum");assert_eq!(t.variants[0].documentation,"yes");assert_eq!(t.variants[1].documentation,"no");
    let TopLevelDecl::Func(f)=&p.declarations[4]else{panic!()};assert_eq!(f.documentation,"function");
    let TopLevelDecl::Routine(r)=&p.declarations[5]else{panic!()};assert_eq!(r.documentation,"routine");let Statement::Const(c)=&r.body[0]else{panic!()};assert_eq!(c.documentation,"local");
    let TopLevelDecl::Loop(l)=&p.declarations[6]else{panic!()};assert_eq!(l.documentation,"loop");
    TypeChecker::new().check_program(&p).unwrap();
}
#[test]
fn orphan_and_misplaced_documentation_is_rejected(){
    for source in [
        "/// orphan", "/// import\nimport std",
        "routine main(){/// orphan\n}",
        "routine main(){/// expression\n1}",
        "routine main(){/// variable\nlet x=1}",
        "type R {x:Int,/// orphan\n}",
        "enum E {A,/// orphan\n}",
        "func f(/// parameter\nx:Int)->Int{x}",
        "func f()->Int{1} /// orphan",
    ]{assert!(Parser::new(source).unwrap().parse_program().is_err(),"{source}");}
}
#[test]
fn markdown_preserves_examples_and_renders_typed_signatures(){
    let source=std::fs::read_to_string("programs/23_docstrings.lia").unwrap();let p=parse(&source);
    let docs=documentation::render_markdown(&p,"23_docstrings.lia");
    for expected in [
        "# API: `23_docstrings.lia`","## Imports","import std",
        "## const `REPEATS`","const REPEATS: Int = 2",
        "func identity<T>(value: T) -> T","### Parameters","### Returns",
        "### Field `x`","Posição horizontal.",
        "### Variant `Found`","Found(T)","Nenhum valor disponível.",
        "~~~lia\nidentity<Int>(3)\n~~~",
        "loop Listener()","routine Worker(value: Int)",
        "- a: primeira parcela.","Retorna uma cópia do argumento.",
    ]{assert!(docs.contains(expected),"missing {expected}: {docs}");}
    assert!(!docs.contains("Repetições locais"));
    assert!(docs.find("## const").unwrap()<docs.find("## type").unwrap());
    assert_eq!(docs,documentation::render_markdown(&p,"23_docstrings.lia"));
}
#[test]
fn doc_cli_stdout_and_file_outputs_match_without_running_code(){
    let dir=Directory::new();
    let input=dir.write("main.lia","import std\n/// Fails only if executed.\nfunc failure()->Int{throw \"runtime\"}\n/// Entrypoint.\nroutine main(){std.log(\"MUST NOT RUN\") std.log(failure())}");
    let out=cli(&[Path::new("doc"),&input]);success(&out);
    let docs=String::from_utf8(out.stdout).unwrap();assert!(docs.starts_with("# API:"));assert!(!docs.contains("MUST NOT RUN"));assert!(out.stderr.is_empty());
    let verbose=cli(&[Path::new("doc"),&input,Path::new("--verbose")]);success(&verbose);assert_eq!(verbose.stdout,docs.as_bytes());
    let output=dir.0.join("api.md");let out=cli(&[Path::new("docs"),&input,Path::new("-o"),&output]);success(&out);assert!(out.stdout.is_empty());assert_eq!(std::fs::read_to_string(output).unwrap(),docs);
}
#[test]
fn invalid_programs_do_not_generate_or_overwrite_documentation(){
    let dir=Directory::new();let output=dir.write("api.md","keep");
    for source in [
        "/// invalid\nfunc f()->Int{\"wrong\"}",
        "/// invalid constant\nconst N=1/0",
        "/// cycle\nfunc a()->Int{b()} func b()->Int{a()}",
        "/// invalid recursion\nfunc f(n:Int)->Int{f(n)}",
        "/// invalid syntax\nfunc",
    ]{
        let input=dir.write("main.lia",source);let out=cli(&[Path::new("doc"),&input,Path::new("-o"),&output]);assert!(!out.status.success(),"{source}");assert_eq!(std::fs::read_to_string(&output).unwrap(),"keep");
    }
    let source="/// Keep this source.\nfunc value()->Int{1}";let input=dir.write("main.lia",source);
    let out=cli(&[Path::new("doc"),&input,Path::new("-o"),&input]);assert!(!out.status.success());assert_eq!(std::fs::read_to_string(input).unwrap(),source);
    for args in [vec!["doc"],vec!["doc","file","-x"],vec!["doc","file","-x","output"]]{let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(args).output().unwrap();assert!(!out.status.success());assert!(String::from_utf8_lossy(&out.stderr).contains("Usage: lia doc"));}
}
#[test]
fn imported_declarations_keep_metadata_without_polluting_local_api(){
    let dir=Directory::new();dir.write("library.lia","/// Imported constant.\nconst LIMIT=3\n/// Imported function.\nfunc value()->Int{LIMIT}");
    let input=dir.write("main.lia","import library.{value}\n/// Local wrapper.\nfunc wrapper()->Int{value()}");
    let raw=parse(&std::fs::read_to_string(&input).unwrap());let resolved=PackageManager::new(&dir.0).resolve_program(raw).unwrap();TypeChecker::new().check_program(&resolved).unwrap();
    for name in ["value","library.value"]{let f=resolved.declarations.iter().find_map(|d|match d{TopLevelDecl::Func(f) if f.name==name=>Some(f),_=>None}).unwrap();assert_eq!(f.documentation,"Imported function.");}
    let out=cli(&[Path::new("doc"),&input]);success(&out);let docs=String::from_utf8(out.stdout).unwrap();assert!(docs.contains("Local wrapper."));assert!(docs.contains("import library.{value}"));assert!(!docs.contains("Imported function."));assert!(!docs.contains("Imported constant."));
}
#[test]
fn documented_source_and_artifacts_execute_identically(){
    let dir=Directory::new();let artifact=dir.0.join("main.liac");let input=Path::new("programs/23_docstrings.lia");
    let out=cli(&[Path::new("run"),input]);success(&out);let expected=out.stdout;
    assert_eq!(String::from_utf8_lossy(&expected),"sum 5 7\nindex 0\nindex 1\nworker 2\n");
    success(&cli(&[Path::new("compile"),input,Path::new("-o"),&artifact]));
    let program:Program=serde_json::from_slice(&std::fs::read(&artifact).unwrap()).unwrap();
    let f=program.declarations.iter().find_map(|d|match d{TopLevelDecl::Func(f) if f.name=="add"=>Some(f),_=>None}).unwrap();assert!(f.documentation.contains("Soma dois inteiros"));
    let out=cli(&[Path::new("run"),&artifact]);success(&out);assert_eq!(out.stdout,expected);
    let mut legacy=serde_json::to_value(program).unwrap();
    fn remove_docs(v:&mut serde_json::Value){match v{serde_json::Value::Object(o)=>{o.remove("documentation");for v in o.values_mut(){remove_docs(v);}},serde_json::Value::Array(a)=>for v in a{remove_docs(v);},_=>{}}}
    remove_docs(&mut legacy);std::fs::write(&artifact,serde_json::to_vec(&legacy).unwrap()).unwrap();
    let out=cli(&[Path::new("run"),&artifact]);success(&out);assert_eq!(out.stdout,expected);
}
#[test]
fn complexity_annotations_preserve_docstring_association(){
    let dir=Directory::new();let input=dir.write("main.lia","/// First line.\n///\n/// Another paragraph.\nfunc value()->Int{3}");
    let before=parse(&std::fs::read_to_string(&input).unwrap());let output=dir.0.join("annotated.lia");
    for _ in 0..2{success(&cli(&[Path::new("annotate"),&input,Path::new("-o"),&output]));}
    let after=parse(&std::fs::read_to_string(output).unwrap());assert_eq!(before,after);
}
#[test]
fn signature_formatting_covers_generic_functions_references_and_data_constants(){
    let source="/// Generic callback.\nfunc make<T>(f:(T)->List[T])->(T)->List[T]{f}\n/// Borrowed value.\nfunc borrowed(x:Ref[Int])->Int{x}\n/// Limits.\nconst LIMITS:Dict[Json]={values:[1,2],name:\"docs\"}";
    let p=parse(source);TypeChecker::new().check_program(&p).unwrap();let docs=documentation::render_markdown(&p,"api");
    assert!(docs.contains("func make<T>(f: (T) -> List[T]) -> (T) -> List[T]"));
    assert!(docs.contains("func borrowed(x: Ref[Int]) -> Int"));
    assert!(docs.contains("const LIMITS: Dict[Json] ="));
    assert!(docs.contains("\"values\": [1, 2]"));
}
