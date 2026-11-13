use std::process::{Command,Output};
use std::sync::atomic::{AtomicUsize,Ordering};
use lia::{ast::TopLevelDecl,parser::Parser,semantic::{packages::PackageManager,type_checker::TypeChecker,dag::CallGraphValidator,termination::TerminationChecker,complexity::{Complexity,ComplexityAnalyzer}}};
fn parse(s:&str)->lia::ast::Program{Parser::new(s).unwrap().parse_program().unwrap()}
fn resolved(s:&str)->lia::ast::Program{PackageManager::new("programs").resolve_program(parse(s)).unwrap()}
fn valid(s:&str)->bool{TypeChecker::new().check_program(&resolved(s)).is_ok()}
fn run(s:&str)->Output{
    static NEXT:AtomicUsize=AtomicUsize::new(0);
    let dir=std::env::temp_dir().join(format!("lia-option-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();std::fs::write(dir.join("main.lia"),s).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();out
}
#[test]
fn sum_declarations_support_generic_and_compact_syntax(){
    let p=parse("type Maybe<T>=Some(T)|None type Failure<T,E> = Good(T) | Bad(E) type Record { n: Int }");
    let TopLevelDecl::Type(maybe)=&p.declarations[0] else{panic!()};assert_eq!(maybe.type_params,["T"]);assert_eq!(maybe.variants.len(),2);assert!(maybe.fields.is_empty());
    let TopLevelDecl::Type(record)=&p.declarations[2] else{panic!()};assert!(record.variants.is_empty());
    TypeChecker::new().check_program(&p).unwrap();
    for s in ["type Maybe<T> =", "type Maybe<T> = Some(T) |", "type Bad = Same | Same", "type Bad = lower"]{assert!(Parser::new(s).unwrap().parse_program().is_err(),"{s}");}
}
#[test]
fn builtin_packages_are_explicit_and_need_no_source_files(){
    let p=PackageManager::new("/tmp/lia-no-package-files-required").resolve_program(parse("import option import result")).unwrap();
    TypeChecker::new().check_program(&p).unwrap();CallGraphValidator::new().build_and_validate(&p).unwrap();
    for source in ["routine main(){let x:Option[Int]=None}","routine main(){Some(1)}","routine main(){let x=null}"]{assert!(!valid(source),"{source}");}
}
#[test]
fn source_and_artifact_execute_optional_values_and_results(){
    let expected="option 7 true\nlazy 9\nbound 4\nresult 4 true\nfallback 12\ncustom\nworker 7\n";
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["run","programs/19_option_result.lia"]).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
    let artifact=std::env::temp_dir().join(format!("lia-option-{}.liac",std::process::id()));let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["compile","programs/19_option_result.lia","-o"]).arg(&artifact).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();std::fs::remove_file(artifact).unwrap();assert!(out.status.success());assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
}
#[test]
fn missing_patterns_and_incompatible_payloads_are_rejected(){
    for source in [
        "import option routine main(){let x:Option[Int]=Some(\"bad\")}",
        "import option routine main(){let x:Option[Int]=None<String>}",
        "import option func bad(x:Option[Int])->Int{match x{Some(n)=>n}}",
        "import result func bad(x:Result[Int,String])->Int{match x{Ok(n)=>n}}",
        "import result routine main(){let x:Result[Int,String]=Err(42)}",
        "import option routine main(){let x:Option[Int]=Some(1) match x{Some=>0,None=>1}}",
        "import option func id(x:Int)->Int{x} routine main(){id(Some(1))}",
        "import option routine main(){let x:Option[Int]=null}",
    ]{assert!(!valid(source),"{source}");}
}
#[test]
fn lazy_fallbacks_run_only_on_empty_or_error(){
    let out=run(r#"import std import option import result
        routine main(){
            let some:Option[Int]=Some(7)
            let ok:Result[Int,String]=Ok(8)
            std.log(option.unwrap_or_else(some,||->Int{throw "unexpected"}),result.unwrap_or_else(ok,|e:String|->Int{throw e}))
            try{std.log(Err<Int,String>("data error"))}catch e{std.log("unexpected catch")}
        }"#);
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"7 8\nErr(data error)\n");
    let out=run("import std import option routine main(){std.log(option.unwrap_or(Some(7),1/0))}");assert!(!out.status.success());assert!(out.stdout.is_empty());
}
#[test]
fn map_and_then_and_error_mapping_preserve_types_and_branches(){
    let out=run(r#"import std import option import result
        func next(x:Int)->Option[Int]{Some(x+1)}
        func checked(x:Int)->Result[Int,String]{Ok(x+2)}
        routine main(){
            std.log(option.map(None<Int>,|x:Int|->String{"unused"}))
            std.log(option.and_then(Some(3),next),option.and_then(None<Int>,next))
            let good:Result[Int,String]=Ok(2)
            let bad:Result[Int,String]=Err("bad")
            std.log(result.map(good,|x:Int|->String{"changed"}),result.map(bad,|x:Int|->String{"unused"}))
            std.log(result.map_err(good,|e:String|->Int{9}),result.map_err(bad,|e:String|->Int{9}))
            std.log(result.and_then(good,checked),result.and_then(bad,checked))
        }"#);
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"None\nSome(4) None\nOk(changed) Err(bad)\nOk(2) Err(9)\nOk(4) Err(bad)\n");
}
#[test]
fn nested_optional_patterns_and_custom_sum_types_work(){
    let out=run("import std type Maybe<T>=Some(T)|None func nested(x:Maybe[Maybe[Int]])->Int{match x{Some(Some(n))=>n,Some(None)=>1,None=>0}} routine main(){std.log(nested(Some(Some(9))),nested(Some(None<Int>)),nested(None<Maybe[Int]>))}");assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"9 1 0\n");
}
#[test]
fn copying_callback_payloads_to_routines_keeps_closure_snapshots(){
    let out=run("import std import option routine Worker(x:Option[(Int)->Int]){match x{Some(f)=>std.log(f(3)),None=>std.log(0)}} routine main(){let offset=4 let x=Some(|n:Int|->Int{offset+n}) let copy=x let offset=10 spawn Worker(copy)}");assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"7\n");
    assert!(!valid("import option routine Worker(x:Option[Ref[Int]]){} routine main(){spawn Worker(Some(ref 1))}"));
    assert!(!valid("import option routine Worker(x:Option[Ref[Int]]){} routine main(){spawn Worker(None<Ref[Int]>)}"));
    assert!(!valid("import result routine Worker(x:Result[Int,Ref[Int]]){} routine main(){spawn Worker(Err<Int,Ref[Int]>(ref 1))}"));
}
#[test]
fn optional_callbacks_still_participate_in_cycles_and_complexity(){
    let p=resolved("import option func bad(x:Int)->Int{let f=option.unwrap_or(Some(bad),bad) f(x)}");TypeChecker::new().check_program(&p).unwrap();assert!(CallGraphValidator::new().build_and_validate(&p).is_err());assert!(TerminationChecker::verify_program(&p).is_err());
    let p=resolved("import option func work(xs:List[Int])->Int{for x in xs{for y in xs{y}} 1} routine main(){option.map(Some([1,2]),work)}");TypeChecker::new().check_program(&p).unwrap();let entries=ComplexityAnalyzer::new().try_analyze(&p).unwrap();assert_eq!(entries.iter().find(|e|e.name=="main").unwrap().complexity,Complexity::Polynomial(2));
}
#[test]
fn helper_aliases_can_be_imported_selectively(){
    let out=run("import std import option.{Option,unwrap_or} routine main(){let x:Option[Int]=None std.log(unwrap_or(x,3))}");assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"3\n");
}
