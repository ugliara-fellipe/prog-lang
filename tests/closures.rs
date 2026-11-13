use std::process::{Command,Output};
use std::sync::atomic::{AtomicUsize,Ordering};
use lia::{parser::Parser,semantic::{type_checker::TypeChecker,dag::CallGraphValidator,termination::TerminationChecker,complexity::{Complexity,ComplexityAnalyzer},SemanticError}};
fn parse(s:&str)->lia::ast::Program{Parser::new(s).unwrap().parse_program().unwrap()}
fn check(s:&str)->Result<(),SemanticError>{TypeChecker::new().check_program(&parse(s))}
fn run(s:&str)->Output{
    static NEXT:AtomicUsize=AtomicUsize::new(0);
    let dir=std::env::temp_dir().join(format!("lia-closure-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();std::fs::write(dir.join("main.lia"),s).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();out
}
#[test]
fn closures_capture_snapshot_return_nest_compose_and_spawn_from_source_and_artifact(){
    let expected="snapshot 12 13 20\nreturned 7 6\nnested 32\nenum 14\nworker 19\n";
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["run","programs/18_closures.lia"]).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
    let artifact=std::env::temp_dir().join(format!("lia-closure-{}.liac",std::process::id()));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["compile","programs/18_closures.lia","-o"]).arg(&artifact).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();std::fs::remove_file(artifact).unwrap();assert!(out.status.success());assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
}
#[test]
fn only_free_lexical_variables_are_captured(){
    let out=run("import std routine main(){let pointer=ref 1 let n=9 let f=|n:Int|->Int{n+1} let g=||->Int{let local=3 local} std.log(f(2),g())}");
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"3 3\n");
}
#[test]
fn nested_instances_copy_current_invocation_parameters(){
    let out=run("import std routine main(){let make=|x:Int|->()->Int{||->Int{x}} let first=make(1) let second=make(2) std.log(first(),second(),(|x:Int|->Int{x+1})(5))}");
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"1 2 6\n");
}
#[test]
fn noncopyable_captures_are_rejected_even_before_spawn(){
    for s in [
        "routine main(){let p=ref 1 let f=||->Int{p}}",
        "routine main(){let xs=[ref 1] let f=||->Int{for x in xs{x} 0}}",
        "enum Shared{Pointer(Ref[Int])} routine main(){let x=Pointer(ref 1) let f=||->Int{match x{Pointer(n)=>n}}}",
        "type Shared{pointer:Ref[Int]} routine producer(x:Shared){let f=||->Shared{x}}",
        "func capture<T>(x:T)->()->T{||->T{x}}",
    ] {assert!(matches!(check(s),Err(SemanticError::UncopyableCapture(_))),"{s}");}
}
#[test]
fn signatures_return_paths_and_lexical_scope_are_checked(){
    for s in [
        "routine main(){let f=|x:Int|->Int{x} f(\"bad\")}",
        "routine main(){let f=|x:Int|->Int{x} f()}",
        "routine main(){let f=|x:Int|->String{x}}",
        "routine main(){let f=|x:Bool|->Int{match x{True=>1}}}",
        "routine main(){let f=||->Int{missing}}",
        "routine main(){let f=||->Int{let inner=1 inner} inner}",
    ] {assert!(check(s).is_err(),"{s}");}
}
#[test]
fn closures_have_function_context_and_cannot_capture_loop_context(){
    for s in [
        "routine Worker(){} routine main(){let f=||->Int{spawn Worker() 1}}",
        "loop Inbox(){let f=||->Int{message.pending} terminate}",
        "loop Inbox(){let f=||->String{let x=memory value x} terminate}",
        "routine main(){let f=||->Int{terminate 1}}",
    ] {assert!(check(s).is_err(),"{s}");}
}
#[test]
fn cycles_through_closure_bodies_are_rejected_by_all_analyses(){
    for s in [
        "func bad(x:Int)->Int{let recur=|y:Int|->Int{bad(y)} recur(x)}",
        "enum Handler{Run((Int)->Int)} func bad(x:Int)->Int{let h=Run(|y:Int|->Int{bad(y)}) match h{Run(f)=>f(x)}}",
    ] {let p=parse(s);TypeChecker::new().check_program(&p).unwrap();assert!(CallGraphValidator::new().build_and_validate(&p).is_err());assert!(TerminationChecker::verify_program(&p).is_err());assert!(ComplexityAnalyzer::new().try_analyze(&p).is_err());}
}
#[test]
fn called_closure_body_contributes_its_cost(){
    let p=parse("routine main(){let xs=[1,2] let f=||->Int{for x in xs{for y in xs{y}} 0} f()}");
    TypeChecker::new().check_program(&p).unwrap();
    let entries=ComplexityAnalyzer::new().try_analyze(&p).unwrap();assert_eq!(entries.iter().find(|e|e.name=="main").unwrap().complexity,Complexity::Polynomial(2));
}
#[test]
fn closures_work_with_parametric_callbacks_and_lists(){
    let out=run(r#"import std
        func map<T,U>(xs:List[T],f:(T)->U)->List[U]{match xs{Nil=>std.empty<U>(),Cons(x,tail)=>std.prepend<U>(f(x),map<T,U>(tail,f))}}
        func wrap<T>(f:(T)->Int)->(T)->Int{|x:T|->Int{f(x)}}
        routine main(){let offset=4 let f=|x:Int|->Int{x+offset} std.log(map([1,2],f),wrap<Int>(f)(3)) for g in [f,|x:Int|->Int{x*2}]{std.log(g(5))}}
    "#);
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"[5, 6] 7\n9\n10\n");
}
#[test]
fn exceptions_cross_closure_calls_and_evaluation_failures_remain_errors(){
    let out=run("import std routine main(){let error_value=9 let f=||->Int{throw error_value} try{f()}catch error{std.log(error)}}");assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"9\n");
    let out=run("import std routine main(){let zero=0 let f=||->Int{1/zero} try{std.log(f())}catch error{std.log(\"wrong\")}}");assert!(!out.status.success());assert!(out.stdout.is_empty());assert!(String::from_utf8_lossy(&out.stderr).contains("Runtime error"));
}
#[test]
fn malformed_closure_parameter_lists_are_rejected(){
    for s in ["routine main(){let f=|x:Int,x:Int|->Int{x}}","routine main(){let f=|x|->Int{x}}","routine main(){let f=||{1}}"]{assert!(Parser::new(s).unwrap().parse_program().is_err(),"{s}");}
}
#[test]
fn imported_closures_preserve_captures_and_package_private_helpers(){
    let dir=std::env::temp_dir().join(format!("lia-closure-package-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();
    std::fs::write(dir.join("ops.lia"),"func helper(x:Int)->Int{x*2} func make(n:Int)->(Int)->Int{|x:Int|->Int{helper(x)+n}}").unwrap();
    std::fs::write(dir.join("main.lia"),"import ops.make import std func helper(x:Int)->Int{x*3} routine main(){std.log(ops.make(3)(4))}").unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"11\n");
}
