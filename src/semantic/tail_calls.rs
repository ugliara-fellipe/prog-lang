//! Tail positions follow function results through final match/try branches.
//! A protected tail call retains its exception handler in the trampoline.
use crate::ast::*;
use crate::semantic::SemanticError;

pub fn validate_program(program:&Program)->Result<(),SemanticError>{
    for decl in &program.declarations {match decl {
        TopLevelDecl::Func(f)=>body(&f.body,true)?,
        TopLevelDecl::Routine(r)=>body(&r.body,false)?,
        TopLevelDecl::Loop(l)=>body(&l.body,false)?,_=>{}
    }}Ok(())
}
fn body(statements:&[Statement],tail:bool)->Result<(),SemanticError>{
    for (i,stmt) in statements.iter().enumerate(){let last=tail && i+1==statements.len();match stmt{
        Statement::Const(c)=>expr(&c.value,false)?,
        Statement::ExprStmt(e)=>expr(e,last)?,
        Statement::VarDecl{value:e,..}|Statement::Spawn(e)|Statement::Throw(e)|Statement::MemorySet{value:e,..}|Statement::Signal{payload:e,..}=>expr(e,false)?,
        Statement::Match{target,cases}=>{expr(target,false)?;for case in cases{if let Some(g)=&case.guard{expr(g,false)?;}body(&case.body,last)?;}},
        Statement::TryCatch{try_body,catch_body,..}=>{body(try_body,last)?;body(catch_body,last)?;},
        Statement::ForLoop{collection,body:inner,..}=>{expr(collection,false)?;body(inner,false)?;},
        Statement::Terminate=>{}
    }}Ok(())
}
fn expr(e:&Expr,tail:bool)->Result<(),SemanticError>{
    match e{
        Expr::ConstValue{value,value_type}=>crate::semantic::constants::validate_value(value,value_type)?,
        Expr::TailCall(call)=>{
            if !tail || !matches!(call.as_ref(),Expr::FunctionCall{..}|Expr::IndirectCall{..}) {return Err(SemanticError::InvalidTailCall("a function call must be the final result of a function or closure".into()));}
            expr(call,false)?;
        },
        Expr::Closure{body:inner,..}=>body(inner,true)?,
        Expr::FunctionCall{args,..}|Expr::ListLiteral(args)=>for a in args{expr(a,false)?;},
        Expr::DictLiteral(items)=>for (_,v) in items{expr(v,false)?;},
        Expr::Index{target,key}=>{expr(target,false)?;expr(key,false)?;},
        Expr::IndirectCall{target,args}=>{expr(target,false)?;for a in args{expr(a,false)?;}},
        Expr::Compose{left,right}|Expr::BinaryOp{left,right,..}=>{expr(left,false)?;expr(right,false)?;},
        Expr::Specialize{target,..}|Expr::RefCreate(target)|Expr::MemberAccess{target,..}=>expr(target,false)?,_=>{}
    }Ok(())
}
