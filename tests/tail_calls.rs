use std::process::{Command,Output};
use std::sync::atomic::{AtomicUsize,Ordering};
use lia::{ast::*,parser::Parser,runtime::actor::{SignalRouter,RuntimeValue},semantic::{type_checker::TypeChecker,dag::CallGraphValidator,termination::TerminationChecker,complexity::{Complexity,ComplexityAnalyzer},SemanticError}};
fn parse(s:&str)->Program{Parser::new(s).unwrap().parse_program().unwrap()}
fn checked(s:&str)->Program{let p=parse(s);TypeChecker::new().check_program(&p).unwrap();TerminationChecker::verify_program(&p).unwrap();p}
fn run(s:&str)->Output{
    static NEXT:AtomicUsize=AtomicUsize::new(0);
    let dir=std::env::temp_dir().join(format!("lia-tail-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();std::fs::write(dir.join("main.lia"),s).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();out
}
#[test]
fn deep_structural_recursion_uses_a_bounded_native_stack_with_or_without_annotation(){
    for keyword in ["tailcall ",""]{
        let source=format!("func count<T>(xs:List[T],acc:Int)->Int{{match xs{{Nil=>acc,Cons(_,tail)=>{keyword}count<T>(tail,acc+1)}}}}");
        let p=checked(&source);
        let handle=std::thread::Builder::new().stack_size(256*1024).spawn(move||{
            let router=SignalRouter::default();for d in p.declarations{if let TopLevelDecl::Func(f)=d{router.register_function(f);}}
            let input=RuntimeValue::List((0..2000).map(RuntimeValue::Int).collect());
            assert_eq!(router.evaluate_function_result("count",vec![input,RuntimeValue::Int(0)]),Ok(RuntimeValue::Int(2000)));
        }).unwrap();handle.join().unwrap();
    }
}
#[test]
fn deep_acyclic_tail_chain_between_different_functions_is_iterative(){
    std::thread::Builder::new().stack_size(256*1024).spawn(||{
        let router=SignalRouter::default();
        for i in 0..3000{
            let next=Expr::FunctionCall{name:format!("step{}",i+1),args:vec![]};
            let value=if i==2999{Expr::IntLiteral(7)}else if i%2==0{
                Expr::IndirectCall{target:Box::new(Expr::Closure{params:vec![],return_type:TypeSpec::Primitive("Int".into()),body:vec![Statement::ExprStmt(next)]}),args:vec![]}
            }else{next};
            router.register_function(FuncDecl{documentation:String::new(),name:format!("step{}",i),type_params:vec![],params:vec![],return_type:Some(TypeSpec::Primitive("Int".into())),body:vec![Statement::ExprStmt(value)]});
        }
        assert_eq!(router.evaluate_function_result("step0",vec![]),Ok(RuntimeValue::Int(7)));
    }).unwrap().join().unwrap();
}
#[test]
fn source_and_artifact_preserve_generic_closure_and_protected_tail_calls(){
    let expected="sum 15\ncount 2\nclosure 7\nhandled 3\n";
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["run","programs/20_tailcalls.lia"]).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
    let artifact=std::env::temp_dir().join(format!("lia-tail-{}.liac",std::process::id()));let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["compile","programs/20_tailcalls.lia","-o"]).arg(&artifact).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();std::fs::remove_file(artifact).unwrap();assert!(out.status.success());assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
}
#[test]
fn annotation_is_rejected_outside_a_function_result_position(){
    let prefix="func id(x:Int)->Int{x} ";
    for s in [
        "routine main(){tailcall id(1)}",
        "loop L(){tailcall id(1)}",
        "func f()->Int{let n=tailcall id(1) n}",
        "func f()->Int{tailcall id(1)+1}",
        "func f()->Int{id(tailcall id(1))}",
        "func f()->Int{for x in [1]{tailcall id(x)} 0}",
        "func f()->Int{tailcall id(1) 2}",
        "func f()->Int{throw tailcall id(1)}",
        "func f()->Int{try{tailcall id(1)}catch e{1} 2}",
        "func f()->Int{match tailcall id(1){_=>1}}",
        "func f()->Int{(tailcall id(1))(2)}",
    ]{let p=parse(&format!("{}{}",prefix,s));assert!(matches!(TypeChecker::new().check_program(&p),Err(SemanticError::InvalidTailCall(_))),"{s}");assert!(CallGraphValidator::new().build_and_validate(&p).is_err());}
    assert!(Parser::new("func f()->Int{tailcall 1}").unwrap().parse_program().is_err());
}
#[test]
fn tail_calls_keep_signature_and_termination_checks(){
    for s in [
        "func id(x:Int)->Int{x} func f()->Int{tailcall id(\"bad\")}",
        "func id(x:Int)->Int{x} func f()->String{tailcall id(1)}",
    ]{assert!(TypeChecker::new().check_program(&parse(s)).is_err());}
    let p=parse("func bad(xs:List[Int])->Int{tailcall bad(xs)}");TypeChecker::new().check_program(&p).unwrap();assert!(matches!(TerminationChecker::verify_program(&p),Err(SemanticError::NonDecreasingRecursion(_))));
    let p=parse("func bad(xs:List[Int])->Int{let f=bad match xs{Nil=>0,Cons(_,tail)=>tailcall f(tail)}}");assert!(CallGraphValidator::new().build_and_validate(&p).is_err());
    let p=parse("func a(x:Int)->Int{tailcall b(x)} func b(x:Int)->Int{tailcall a(x)}");assert!(CallGraphValidator::new().build_and_validate(&p).is_err());
}
#[test]
fn pending_handlers_and_catch_tail_calls_preserve_exception_order(){
    let out=run("import std func fail(x:Int)->Int{throw x} func inner(x:Int)->Int{try{tailcall fail(x)}catch e{tailcall fail(x+1)}} func outer()->Int{try{tailcall inner(7)}catch e{99}} func kept(x:Int)->Int{try{tailcall fail(x+1)}catch e{x}} routine main(){std.log(outer(),kept(5))}");assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"99 5\n");
    let out=run("import std func fail(x:Int)->Int{throw x} func protected(x:Int)->Int{try{tailcall fail(1/x)}catch e{9}} routine main(){std.log(protected(0))}");assert!(!out.status.success());assert!(out.stdout.is_empty());assert!(String::from_utf8_lossy(&out.stderr).contains("Runtime error"));
}
#[test]
fn many_protected_tail_calls_keep_handlers_off_the_native_stack(){
    let p=checked("func fail(x:Int)->Int{throw x} func count(xs:List[Int],acc:Int)->Int{try{match xs{Nil=>tailcall fail(acc),Cons(_,tail)=>tailcall count(tail,acc+1)}}catch e{acc}}");
    std::thread::Builder::new().stack_size(256*1024).spawn(move||{
        let router=SignalRouter::default();for d in p.declarations{if let TopLevelDecl::Func(f)=d{router.register_function(f);}}
        assert_eq!(router.evaluate_function_result("count",vec![RuntimeValue::List((0..200).map(RuntimeValue::Int).collect()),RuntimeValue::Int(0)]),Ok(RuntimeValue::Int(200)));
    }).unwrap().join().unwrap();
}
#[test]
fn argument_values_are_evaluated_before_replacing_the_frame(){
    let out=run("import std func fold(xs:List[Int],a:Int,b:Int)->Int{match xs{Nil=>a*10+b,Cons(_,tail)=>tailcall fold(tail,b,a)}} routine main(){std.log(fold([0],1,2),fold([0,0],1,2))}");assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"21 12\n");
}
#[test]
fn tail_arguments_preserve_left_to_right_effects(){
    let out=run("import std func mark(x:Int)->Int{std.log(\"arg\",x) x} func add(a:Int,b:Int)->Int{a+b} func go()->Int{tailcall add(mark(1),mark(2))} routine main(){std.log(\"result\",go())}");
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"arg 1\narg 2\nresult 3\n");
}
#[test]
fn tail_calls_keep_callback_costs(){
    let p=checked("func apply(f:(List[Int])->Int,xs:List[Int])->Int{tailcall f(xs)} func work(xs:List[Int])->Int{for x in xs{for y in xs{y}} 1} routine main(){apply(work,[1,2])}");
    let entries=ComplexityAnalyzer::new().try_analyze(&p).unwrap();assert_eq!(entries.iter().find(|e|e.name=="apply").unwrap().complexity,Complexity::Polynomial(2));
}
#[test]
fn generic_constructors_can_supply_a_tail_result(){
    let out=run("import std import option func present<T>(x:T)->Option[T]{tailcall Some<T>(x)} func absent<T>()->Option[T]{tailcall None<T>()} routine main(){std.log(present(4),absent<Int>())}");
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"Some(4) None\n");
}
#[test]
fn qualified_calls_are_preserved_in_package_tail_positions(){
    let dir=std::env::temp_dir().join(format!("lia-tail-package-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();
    std::fs::write(dir.join("ops.lia"),"func helper(x:Int)->Int{x*2} func call<T>(x:T,f:(T)->Int)->Int{tailcall f(x)} func run(x:Int)->Int{tailcall call<Int>(x,helper)}").unwrap();std::fs::write(dir.join("main.lia"),"import ops.run import std routine main(){std.log(ops.run(6))}").unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"12\n");
}
