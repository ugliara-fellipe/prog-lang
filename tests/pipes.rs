use std::process::{Command,Output};
use std::sync::atomic::{AtomicUsize,Ordering};
use lia::{parser::Parser,semantic::{packages::PackageManager,type_checker::TypeChecker,pipes,dag::CallGraphValidator,termination::TerminationChecker,complexity::{Complexity,ComplexityAnalyzer}}};
fn parse(s:&str)->lia::ast::Program{Parser::new(s).unwrap().parse_program().unwrap()}
fn resolved(s:&str)->lia::ast::Program{PackageManager::new("programs").resolve_program(parse(s)).unwrap()}
fn valid(s:&str)->bool{let p=parse(s);TypeChecker::new().check_program(&p).is_ok()}
fn run(s:&str)->Output{
    static NEXT:AtomicUsize=AtomicUsize::new(0);
    let dir=std::env::temp_dir().join(format!("lia-pipes-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();std::fs::write(dir.join("main.lia"),s).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();out
}
fn output(s:&str,want:&str){let out=run(s);assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),want);}
#[test]
fn precedence_associativity_and_both_composition_spellings_work(){
    output(r#"import std
        func add(n:Int)->Int{n+1} func twice(n:Int)->Int{n*2}
        routine main(){
            std.log(1+2 |> add |> twice,twice <| add <| 1+2)
            std.log(3 |> add >>> twice,(add ->> twice)(3),(add >>> twice >>> add)(3))
            std.log((3 |> add)>0)
        }"#,"8 8\n8 8 9\ntrue\n");
}
#[test]
fn incompatible_types_arities_targets_and_const_initializers_are_rejected(){
    for s in [
        "routine main(){1 |> 2}", "routine main(){2 <| 1}",
        "func f(x:String)->Int{1} routine main(){1 |> f}",
        "func f(x:Int,y:Int)->Int{x+y} routine main(){1 |> f}",
        "func f()->Int{1} func g(x:Int)->Int{x} routine main(){let c=f >>> g}",
        "func f(x:Int)->Int{x} func g(x:String)->String{x} routine main(){let c=f >>> g}",
        "func f(x:Int)->Int{x} routine main(){let c=1 >>> f}",
        "func f<T>(x:T)->T{x} routine main(){let c=f >>> f}",
        "const C=1 |> missing",
        "func f(n:Int){} func g(n:Unknown)->Int{1} routine main(){let c=f >>> g}",
        "routine main(){let r=ref 1 let c=(|n:Int|->Int{r+n}) >>> (|n:Int|->Int{n})}",
    ]{assert!(!valid(s),"{s}");}
    for s in ["routine main(){1 |>}", "routine main(){let f=>>>}", "routine main(){let f=1 ->>}"]{assert!(Parser::new(s).unwrap().parse_program().is_err(),"{s}");}
}
#[test]
fn construction_evaluates_operands_once_and_defers_function_bodies(){
    output(r#"import std
        func make(label:String,offset:Int)->(Int)->Int{std.log(label) |n:Int|->Int{std.log(offset) n+offset}}
        routine main(){
            let c=make("left",1) >>> make("right",2)
            std.log("created")
            std.log(c(3),c(4))
        }"#,"left\nright\ncreated\n1\n2\n1\n2\n6 7\n");
}
#[test]
fn applications_follow_function_call_evaluation_order_and_propagate_failures(){
    output(r#"import std
        func input()->Int{std.log("input") 3}
        func factory()->(Int)->Int{std.log("target") |n:Int|->Int{n+1}}
        routine main(){std.log(input() |> factory(),factory() <| input())}
    "#,"target\ninput\ntarget\ninput\n4 4\n");
    output(r#"import std
        func bad(n:Int)->Int{throw "bad"}
        func next(n:Int)->Int{std.log("BAD") n}
        routine main(){let c=bad >>> next try{std.log(c(1))}catch e{std.log("caught")}}
    "#,"caught\n");
    let out=run("import std func bad(n:Int)->Int{n/0} func next(n:Int)->Int{std.log(999) n} routine main(){(bad >>> next)(1)}");assert!(!out.status.success());assert!(out.stdout.is_empty());
}
#[test]
fn compositions_return_copy_and_spawn_with_captured_values(){
    output(r#"import std import dict
        func chain<T>(f:(T)->T,g:(T)->T)->(T)->T{f >>> g}
        func build(n:Int)->(Int)->Int{(|x:Int|->Int{x+n}) ->> (|x:Int|->Int{x*2})}
        routine Worker(f:(Int)->Int){std.log("worker",f(3))}
        routine main(){
            let offset=2
            let c=chain(|n:Int|->Int{n+offset},|n:Int|->Int{n*2})
            let copy=c let offset=99
            let stored={f:copy}
            match stored["f"]{Some(f)=>std.log(f(2),build(3)(2)),None=>std.log(0)}
            spawn Worker(copy)
        }"#,"8 10\nworker 10\n");
}
#[test]
fn guards_nested_blocks_generic_specialization_and_shadowing_are_lowered(){
    output(r#"import std import option
        func id<T>(x:T)->T{x}
        func positive(n:Int)->Bool{n>0}
        routine main(){
            let plus=|n:Int|->Int{n+1}
            match Some(2){
                Some(n) if (n |> (id<Int> >>> positive))=>{
                    let plus=|n:Int|->Int{n*3}
                    std.log((plus >>> id<Int>)(n))
                },
                _=>std.log(0)
            }
            try{std.log(2 |> plus ->> id<Int>)}catch e{std.log(0)}
            for n in [1,2]{std.log((plus >>> id<Int>)(n))}
        }"#,"6\n3\n2\n3\n");
}
#[test]
fn indirect_cycles_and_complexity_include_composed_functions(){
    for s in [
        "func id(n:Int)->Int{n} func bad(n:Int)->Int{let c=bad >>> id c(n)}",
        "func id(n:Int)->Int{n} func bad(n:Int)->Int{n |> (id >>> bad)}",
    ]{
        let p=parse(s);TypeChecker::new().check_program(&p).unwrap();
        assert!(CallGraphValidator::new().build_and_validate(&p).is_err(),"{s}");
        assert!(TerminationChecker::verify_program(&p).is_err());
        assert!(ComplexityAnalyzer::new().try_analyze(&p).is_err());
    }
    let p=parse("func work(xs:List[Int])->Int{for x in xs{for y in xs{y}} 1} func next(n:Int)->Int{n+1} routine main(){let c=work >>> next c([1,2])}");
    let entries=ComplexityAnalyzer::new().try_analyze(&p).unwrap();
    assert_eq!(entries.iter().find(|e|e.name=="main").unwrap().complexity,Complexity::Polynomial(2));
}
#[test]
fn source_and_artifacts_run_with_no_remaining_composition_nodes(){
    let expected="forward 8\nbackward 8\ncomposition 8 10\nalternative 14\ncapture 16\noptional Some(6)\nguard 2\nworker 10\n";
    output(&std::fs::read_to_string("programs/25_pipes.lia").unwrap(),expected);
    let p=resolved(&std::fs::read_to_string("programs/25_pipes.lia").unwrap());assert!(!pipes::needs_lowering(&p));
    let artifact=std::env::temp_dir().join(format!("lia-pipes-{}.liac",std::process::id()));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["compile","programs/25_pipes.lia","-o"]).arg(&artifact).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();std::fs::remove_file(artifact).unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
}
#[test]
fn imported_composition_qualifies_types_calls_and_constants(){
    let dir=std::env::temp_dir().join(format!("lia-pipes-package-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();
    std::fs::write(dir.join("library.lia"),"const OFFSET=2 type Token=Item(Int)|End func wrap(n:Int)->Token{Item(n)} func read(x:Token)->Int{match x{Item(n)=>n+OFFSET,End=>0}} func build()->(Int)->Int{wrap >>> read}").unwrap();
    std::fs::write(dir.join("main.lia"),"import std import library routine main(){std.log(3 |> library.build())}").unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"5\n");
}

#[test]
fn named_pipe_calls_preserve_structural_recursion_and_variadic_logging(){
    output("import std func last(xs:List[Int])->Int{match xs{Cons(_,tail)=>tail |> last,Nil=>7}} routine main(){last([1,2]) |> std.log}", "7\n");
    let p=parse("func bad(n:Int)->Int{n |> bad}");
    assert!(TerminationChecker::verify_program(&p).is_err());
}
