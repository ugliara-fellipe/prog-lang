//! Composition is elaborated to ordinary typed closures before flow analysis.
use crate::ast::*;
use super::{SemanticError,type_checker::TypeChecker};

pub fn lower_program(program:&Program)->Result<Program,SemanticError>{
    if !needs_lowering(program){return Ok(program.clone());}
    let program=super::constants::lower_program(program)?;
    let mut checker=TypeChecker::new();
    checker.check_program(&program)?;
    checker.lower_compositions(program)
}
pub fn needs_lowering(program:&Program)->bool{
    fn expr(e:&Expr)->bool{match e{
        Expr::Compose{..}=>true,
        Expr::Closure{body,..}=>body_has(body),
        Expr::ListLiteral(v)=>v.iter().any(expr),Expr::DictLiteral(v)=>v.iter().any(|(_,e)|expr(e)),
        Expr::FunctionCall{args,..}=>args.iter().any(expr),
        Expr::IndirectCall{target,args}=>expr(target)||args.iter().any(expr),
        Expr::Index{target,key}|Expr::BinaryOp{left:target,right:key,..}=>expr(target)||expr(key),
        Expr::RefCreate(t)|Expr::TailCall(t)|Expr::MemberAccess{target:t,..}|Expr::Specialize{target:t,..}=>expr(t),
        _=>false
    }}
    fn body_has(body:&[Statement])->bool{body.iter().any(|s|match s{
        Statement::Const(c)=>expr(&c.value),
        Statement::ForLoop{collection,body,..}=>expr(collection)||body_has(body),
        Statement::Match{target,cases}=>expr(target)||cases.iter().any(|c|c.guard.as_ref().is_some_and(expr)||body_has(&c.body)),
        Statement::TryCatch{try_body,catch_body,..}=>body_has(try_body)||body_has(catch_body),
        Statement::VarDecl{value:e,..}|Statement::ExprStmt(e)|Statement::Spawn(e)|Statement::Throw(e)|Statement::MemorySet{value:e,..}|Statement::Signal{payload:e,..}=>expr(e),
        _=>false
    })}
    program.declarations.iter().any(|d|match d{TopLevelDecl::Const(c)=>expr(&c.value),TopLevelDecl::Func(f)=>body_has(&f.body),TopLevelDecl::Routine(r)=>body_has(&r.body),TopLevelDecl::Loop(l)=>body_has(&l.body),_=>false})
}
