//! Lexical free-variable analysis and stable identities within a Program AST.
use std::collections::HashSet;
use crate::ast::*;

pub fn identity(expr:&Expr)->String {format!("\0closure:{}",expr as *const Expr as usize)}
pub fn display_name(name:&str)->&str {if name.starts_with("\0closure:"){"anonymous closure"}else{name}}
pub fn signature(expr:&Expr)->TypeSpec {
    let Expr::Closure{params,return_type,..}=expr else {panic!("expected closure")};
    TypeSpec::Function(params.iter().map(|p|p.param_type.clone()).collect(),Box::new(return_type.clone()))
}
pub fn free_variables(params:&[Param],body:&[Statement])->HashSet<String>{
    let mut free=HashSet::new();let bound=params.iter().map(|p|p.name.clone()).collect();
    free_body(body,&bound,&mut free);free
}
fn bind_pattern(pattern:&Pattern,bound:&mut HashSet<String>){match pattern{Pattern::Identifier(n)=>{bound.insert(n.clone());},Pattern::Constructor(_,parts)=>for p in parts{bind_pattern(p,bound);},_=>{}}}
fn free_expr(expr:&Expr,bound:&HashSet<String>,free:&mut HashSet<String>){
    let mut read=|name:&str|{let root=name.split('.').next().unwrap_or(name);if !bound.contains(root){free.insert(root.into());}};
    match expr {
        Expr::Variable(n)|Expr::TypeOf(n)=>read(n),
        Expr::FunctionCall{name,args}=>{read(name);for arg in args{free_expr(arg,bound,free);}},
        Expr::IndirectCall{target,args}=>{free_expr(target,bound,free);for arg in args{free_expr(arg,bound,free);}},
        Expr::Closure{params,body,..}=>{let mut local=bound.clone();local.extend(params.iter().map(|p|p.name.clone()));free_body(body,&local,free);},
        Expr::BinaryOp{left,right,..}=>{free_expr(left,bound,free);free_expr(right,bound,free);},
        Expr::RefCreate(t)|Expr::TailCall(t)|Expr::MemberAccess{target:t,..}|Expr::Specialize{target:t,..}=>free_expr(t,bound,free),
        Expr::ListLiteral(items)=>for item in items{free_expr(item,bound,free);},
        Expr::DictLiteral(items)=>for (_,v) in items{free_expr(v,bound,free);},
        Expr::Index{target,key}=>{free_expr(target,bound,free);free_expr(key,bound,free);},
        _=>{}
    }
}
fn free_body(body:&[Statement],outer:&HashSet<String>,free:&mut HashSet<String>){
    let mut bound=outer.clone();
    for stmt in body{match stmt{
        Statement::VarDecl{name,value,..}=>{free_expr(value,&bound,free);bound.insert(name.clone());},
        Statement::ExprStmt(e)|Statement::Spawn(e)|Statement::Throw(e)|Statement::MemorySet{value:e,..}|Statement::Signal{payload:e,..}=>free_expr(e,&bound,free),
        Statement::ForLoop{var_name,collection,body}=>{free_expr(collection,&bound,free);let mut local=bound.clone();local.insert(var_name.clone());free_body(body,&local,free);},
        Statement::Match{target,cases}=>{free_expr(target,&bound,free);for case in cases{let mut local=bound.clone();bind_pattern(&case.pattern,&mut local);free_body(&case.body,&local,free);}},
        Statement::TryCatch{try_body,error_name,catch_body}=>{free_body(try_body,&bound,free);let mut local=bound.clone();local.insert(error_name.clone());free_body(catch_body,&local,free);},
        Statement::Terminate=>{}
    }}
}
pub fn collect(program:&Program)->Vec<&Expr>{
    let mut found=Vec::new();
    for decl in &program.declarations{match decl{TopLevelDecl::Func(f)=>collect_body(&f.body,&mut found),TopLevelDecl::Routine(r)=>collect_body(&r.body,&mut found),TopLevelDecl::Loop(l)=>collect_body(&l.body,&mut found),_=>{}}}
    found
}
pub fn collect_expr<'a>(expr:&'a Expr,found:&mut Vec<&'a Expr>){
    match expr{
        Expr::Closure{body,..}=>{found.push(expr);collect_body(body,found);},
        Expr::FunctionCall{args,..}|Expr::ListLiteral(args)=>for arg in args{collect_expr(arg,found);},
        Expr::DictLiteral(items)=>for (_,v) in items{collect_expr(v,found);},
        Expr::Index{target,key}=>{collect_expr(target,found);collect_expr(key,found);},
        Expr::IndirectCall{target,args}=>{collect_expr(target,found);for arg in args{collect_expr(arg,found);}},
        Expr::BinaryOp{left,right,..}=>{collect_expr(left,found);collect_expr(right,found);},
        Expr::RefCreate(t)|Expr::TailCall(t)|Expr::Specialize{target:t,..}|Expr::MemberAccess{target:t,..}=>collect_expr(t,found),_=>{}
    }
}
fn collect_body<'a>(body:&'a [Statement],found:&mut Vec<&'a Expr>){
    for stmt in body{match stmt{
        Statement::ExprStmt(e)|Statement::Spawn(e)|Statement::Throw(e)|Statement::VarDecl{value:e,..}|Statement::MemorySet{value:e,..}|Statement::Signal{payload:e,..}=>collect_expr(e,found),
        Statement::ForLoop{collection,body,..}=>{collect_expr(collection,found);collect_body(body,found);},
        Statement::Match{target,cases}=>{collect_expr(target,found);for case in cases{collect_body(&case.body,found);}},
        Statement::TryCatch{try_body,catch_body,..}=>{collect_body(try_body,found);collect_body(catch_body,found);},Statement::Terminate=>{}
    }}
}
