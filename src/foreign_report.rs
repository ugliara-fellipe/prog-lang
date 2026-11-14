//! Auditable inventory of the foreign bindings potentially used by a program.
use std::collections::{HashSet,VecDeque};
use serde::Serialize;
use crate::{ast::*,semantic::{SemanticError,flow::FunctionFlow}};

#[derive(Debug,Serialize)]
pub struct ForeignUsage{
    pub name:String,
    pub abi:String,
    pub library:Option<String>,
    pub symbol:String,
    pub source:Option<String>,
    pub line:Option<usize>,
    pub declared_cost:Option<String>,
    pub effective_cost:String,
    pub termination:&'static str,
    pub used:bool,
    pub callers:Vec<String>,
}
/// Input must be resolved/lowered, so the flow graph and closure identities
/// refer to this same AST. Reachability includes scheduled routines and loops.
pub fn inventory(program:&Program,entry:Option<&str>,all:bool)->Result<Vec<ForeignUsage>,SemanticError>{
    let flow=FunctionFlow::analyze(program)?;
    let declarations=program.declarations.iter().filter_map(|d|match d{TopLevelDecl::Func(f)=>Some(f.name.as_str()),TopLevelDecl::Routine(r)=>Some(r.name.as_str()),TopLevelDecl::Loop(l)=>Some(l.name.as_str()),_=>None}).collect::<HashSet<_>>();
    if let Some(entry)=entry{if !declarations.contains(entry){return Err(SemanticError::UndeclaredSymbol(entry.into()));}}
    let roots:Vec<String>=if let Some(entry)=entry{vec![entry.into()]}
        else if program.declarations.iter().any(|d|matches!(d,TopLevelDecl::Routine(r) if r.name=="main")){vec!["main".into()]}
        else{declarations.into_iter().filter(|n|!crate::stdlib::is_prelude_function(n)&&!crate::stdlib::is_total_standard_function(n)&&!crate::stdlib::is_bundled_native_function(n)).map(str::to_owned).collect()};
    let mut queue=VecDeque::from(roots);let mut reachable=HashSet::new();
    while let Some(owner)=queue.pop_front(){
        if !reachable.insert(owner.clone()){continue;}
        for edges in [flow.calls_by_owner(),flow.spawns_by_owner()]{if let Some(targets)=edges.get(&owner){queue.extend(targets.iter().cloned());}}
    }
    let mut result=Vec::new();
    for d in &program.declarations{
        let TopLevelDecl::Func(f)=d else{continue};let Some(foreign)=&f.foreign else{continue};
        let used=reachable.contains(&f.name);if !all && !used{continue;}
        let mut callers=flow.calls_by_owner().iter().filter(|(owner,targets)|targets.contains(&f.name)&&(all||reachable.contains(*owner))).map(|(owner,_)|crate::semantic::closures::display_name(owner).to_owned()).collect::<Vec<_>>();
        callers.sort();callers.dedup();
        result.push(ForeignUsage{name:f.name.clone(),abi:foreign.abi.clone(),library:foreign.library.clone(),symbol:foreign.symbol.clone(),source:foreign.source.clone(),line:foreign.line,declared_cost:foreign.cost.clone(),effective_cost:crate::ffi::effective_cost(f).to_big_o(),termination:"assumed by programmer contract",used,callers});
    }
    result.sort_by(|a,b|a.name.cmp(&b.name));Ok(result)
}
pub fn render_text(entries:&[ForeignUsage])->String{
    use std::fmt::Write;
    let mut result=String::new();
    for entry in entries{
        let location=match (&entry.source,entry.line){(Some(p),Some(line))=>format!("{}:{}",p,line),(Some(p),None)=>p.clone(),_=>"(origin not recorded)".into()};
        writeln!(result,"extern {} [{}]",entry.name,if entry.used{"potentially used"}else{"unused from selected entry"}).unwrap();
        writeln!(result,"  source: {}",location).unwrap();
        writeln!(result,"  ABI: {} | library: {} | symbol: {}",entry.abi,entry.library.as_deref().unwrap_or("(process)"),entry.symbol).unwrap();
        writeln!(result,"  cost: {} | effective: {}",entry.declared_cost.as_deref().unwrap_or("UNDECLARED"),entry.effective_cost).unwrap();
        writeln!(result,"  termination: {} | callers: {}",entry.termination,if entry.callers.is_empty(){"(entry/export)".into()}else{entry.callers.join(", ")}).unwrap();
    }
    if entries.is_empty(){result.push_str("No foreign functions used by the selected entry.\n");}
    result
}
