use std::process::{Command,Output};
use std::sync::atomic::{AtomicUsize,Ordering};
use lia::{parser::Parser,semantic::{type_checker::TypeChecker,dag::CallGraphValidator,termination::TerminationChecker,complexity::ComplexityAnalyzer}};
fn parse(s:&str)->lia::ast::Program {Parser::new(s).unwrap().parse_program().unwrap()}
fn valid(s:&str)->bool {TypeChecker::new().check_program(&parse(s)).is_ok()}
fn run(s:&str)->Output {
    static NEXT:AtomicUsize=AtomicUsize::new(0);
    let dir=std::env::temp_dir().join(format!("lia-enum-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();
    std::fs::write(dir.join("main.lia"),s).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();out
}
#[test]
fn source_and_artifact_execute_recursive_generic_enums() {
    let expected="7\n9\n5\non\nworker 7\n";
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["run","programs/17_enums.lia"]).output().unwrap();
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
    let artifact=std::env::temp_dir().join(format!("lia-enum-{}.liac",std::process::id()));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["compile","programs/17_enums.lia","-o"]).arg(&artifact).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();std::fs::remove_file(artifact).unwrap();assert!(out.status.success());assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
}
#[test]
fn nullary_variants_and_constructor_function_values() {
    let out=run(r#"import std enum Maybe<T>{Some(T),Nothing}
        func unwrap(value:Maybe[Int])->Int{match value{Some(x)=>x,Nothing=>0}}
        routine main(){let empty:Maybe[Int]=Nothing let second=Nothing<Int> let make:(Int)->Maybe[Int]=Some<Int> std.log(unwrap(empty),unwrap(second),unwrap(make(8)))}"#);
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"0 0 8\n");
}
#[test]
fn constructor_signatures_and_patterns_are_checked() {
    for source in [
        "enum R{Ok(Int),Err(String)} routine main(){Ok(\"bad\")}",
        "enum R{Ok(Int),Err(String)} routine main(){Ok()}",
        "enum R{Ok(Int),Err(String)} routine main(){let x:R=Err(\"bad\") match x{Ok(a,b)=>a,Err(e)=>e}}",
        "enum A{One} enum B{Two} func wrong(x:A)->Int{match x{Two=>1}}",
        "enum R{Pair(Int,Int)} routine main(){match Pair(1,2){Pair(x,x)=>x}}",
        "enum R{Ok(Int),Err(String)} routine main(){match Ok(1){Ok(x)=>x}}",
        "enum R{Ok(Int)} func wrong(x:R)->String{match x{Ok(n)=>n}}",
        "enum R{Ok(Int)} routine main(){let x=Ok(1) let field=x.value}",
        "enum Int{Number} routine main(){}",
    ] {assert!(!valid(source),"{source}");}
}
#[test]
fn exhaustive_matrix_handles_nested_patterns_and_products() {
    let good="enum R{Pair(Bool,Bool),Other} func f(x:R)->Int{match x{Pair(True,True)=>1,Pair(True,False)=>2,Pair(False,_)=>3,Other=>4}}";
    assert!(valid(good));
    assert!(!valid("enum R{Pair(Bool,Bool),Other} func f(x:R)->Int{match x{Pair(True,_)=>1,Pair(False,True)=>2,Other=>4}}"));
    assert!(valid("enum R{Box(List[Bool]),Other} func f(x:R)->Int{match x{Box(Nil)=>0,Box(Cons(_, _))=>1,Other=>2}}"));
}
#[test]
fn references_inside_enum_payloads_cannot_cross_spawn() {
    for source in [
        "enum Shared{Pointer(Ref[Int])} routine Worker(x:Shared){} routine main(){spawn Worker(Pointer(ref 1))}",
        "enum Shared<T>{Data(T)} routine Worker(x:Shared[Ref[Int]]){} routine main(){spawn Worker(Data(ref 1))}",
    ] {assert!(!valid(source),"{source}");}
}
#[test]
fn function_payloads_participate_in_flow_cycles_and_cost() {
    let p=parse("enum Action{Call((Int)->Int)} func bad(x:Int)->Int{match Call(bad){Call(f)=>f(x)}}");
    TypeChecker::new().check_program(&p).unwrap();assert!(CallGraphValidator::new().build_and_validate(&p).is_err());assert!(TerminationChecker::verify_program(&p).is_err());assert!(ComplexityAnalyzer::new().try_analyze(&p).is_err());
    let out=run("import std enum Action{Call((Int)->Int)} func twice(x:Int)->Int{x*2} routine main(){let a=Call(twice) match a{Call(f)=>std.log(f(3))}}");
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"6\n");
}
#[test]
fn malformed_enum_declarations_are_rejected() {
    for s in ["enum Empty{}","enum Bad{A,A}","enum Bad{lower}"] {assert!(Parser::new(s).unwrap().parse_program().is_err());}
}

#[test]
fn equal_variant_names_require_nominal_qualification() {
    assert!(!valid("enum A{Same(Int)} enum B{Same(String)} routine main(){Same(1)}"));
    let out=run("import std enum A{Same(Int)} enum B{Same(String)} routine main(){let x=A.Same(4) let y=B.Same(\"other\") match x{A.Same(n)=>std.log(n)} match y{B.Same(s)=>std.log(s)}}");
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout),"4\nother\n");
    assert!(!valid("enum A{Same(Int)} enum B{Same(Int)} func wrong(x:A)->Int{match x{B.Same(n)=>n}}"));
}

#[test]
fn enum_callbacks_contribute_their_cost() {
    let p=parse("enum Action{Call((List[Int])->Int)} func invoke(a:Action,xs:List[Int])->Int{match a{Call(f)=>f(xs)}} func work(xs:List[Int])->Int{for x in xs{for y in xs{y}} 1} routine main(){invoke(Call(work),[1,2])}");
    TypeChecker::new().check_program(&p).unwrap();
    let entries=ComplexityAnalyzer::new().try_analyze(&p).unwrap();
    assert_eq!(entries.iter().find(|e|e.name=="invoke").unwrap().complexity,lia::semantic::complexity::Complexity::Polynomial(2));
}
#[test]
fn imported_enums_preserve_qualified_constructors_and_patterns() {
    let dir=std::env::temp_dir().join(format!("lia-enum-package-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();
    std::fs::write(dir.join("model.lia"),"enum Outcome<T>{Good(T),Bad(String)} func make()->Outcome[Int]{Good(12)}").unwrap();
    std::fs::write(dir.join("main.lia"),"import model import std routine main(){let x:model.Outcome[Int]=model.make() match x{model.Outcome.Good(n)=>std.log(n),model.Outcome.Bad(e)=>std.log(e)}}").unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"12\n");
}
