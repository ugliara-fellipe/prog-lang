use std::process::{Command,Output};
use std::sync::atomic::{AtomicUsize,Ordering};
use lia::{ast::*,parser::Parser,runtime::actor::{SignalRouter,RuntimeValue as V,EvaluationFailure},semantic::{packages::PackageManager,type_checker::TypeChecker,dag::CallGraphValidator,termination::TerminationChecker,complexity::{Complexity,ComplexityAnalyzer},SemanticError}};
fn parse(s:&str)->Program{Parser::new(s).unwrap().parse_program().unwrap()}
fn resolved(s:&str)->Program{PackageManager::new("programs").resolve_program(parse(s)).unwrap()}
fn valid(s:&str)->bool{TypeChecker::new().check_program(&resolved(s)).is_ok()}
fn run(s:&str)->Output{
    static NEXT:AtomicUsize=AtomicUsize::new(0);
    let dir=std::env::temp_dir().join(format!("lia-guards-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();std::fs::write(dir.join("main.lia"),s).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();out
}
fn output(s:&str,want:&str){let out=run(s);assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),want);}
#[test]
fn parser_preserves_optional_guards_and_rejects_malformed_syntax(){
    let p=parse("routine main(){match 1{n if n>0=>n,_=>0}}");
    let TopLevelDecl::Routine(r)=&p.declarations[0]else{panic!()};
    let Statement::Match{cases,..}=&r.body[0]else{panic!()};
    assert!(matches!(&cases[0].guard,Some(Expr::BinaryOp{op,..}) if op==">"));assert!(cases[1].guard.is_none());
    for s in ["routine main(){match 1{_ if =>0,_=>1}}","routine main(){match 1{_ if True if False=>0,_=>1}}","routine main(){if True{}}"]{assert!(Parser::new(s).unwrap().parse_program().is_err(),"{s}");}
}
#[test]
fn guards_require_booleans_and_pattern_bindings_stay_inside_the_arm(){
    for s in [
        "routine main(){match 1{n if n=>0,_=>1}}",
        "routine main(){match 1{_ if \"yes\"=>0,_=>1}}",
        "routine main(){match 1{_ if ref True=>0,_=>1}}",
        "routine main(){match 1{_ if []=>0,_=>1}}",
        "routine main(){match 1{_ if missing=>0,_=>1}}",
        "import option routine main(){match Some(1){Some(n) if n>\"wrong\"=>0,_=>1}}",
        "import option routine main(){match Some(1){Some(n) if n>0=>0,_ if n>0=>1,_=>2}}",
        "import option routine main(){match Some(1){Some(n) if n>0=>0,_=>1} let escaped=n}",
        "import option routine main(){match Some(1){Some(n) if n(1)=>0,_=>1}}",
        "routine main(){match 1{_ if message.pending>0=>0,_=>1}}",
        "func f()->Bool{match 1{_ if memory state==\"ready\"=>True,_=>False}}",
        "routine main(){match 1{_ if std.predicate()=>0,_=>1}}",
        "import option routine main(){match Some(1){Some(n,n) if True=>0,_=>1}}",
        "routine main(){match 1{_ if False=>missing,_=>0}}",
    ]{assert!(!valid(s),"{s}");}
    assert!(valid("func predicate<T>()->Bool{True} func test()->Int{match 1{_ if predicate<Int>()=>1,_=>0}}"));
}
#[test]
fn guarded_rows_never_establish_exhaustiveness_or_return_coverage(){
    for s in [
        "func f(n:Int)->Int{match n{_ if True=>1}}",
        "routine main(){match True{True if True=>0,False=>1}}",
        "import option func f(x:Option[Int])->Int{match x{Some(n) if n>0=>n,None=>0}}",
        "import option func f(x:Option[Bool])->Int{match x{Some(True) if True=>1,Some(False)=>2,None=>0}}",
        "func f()->Int{match 1{_ if True=>{throw \"error\"}}}",
        "func f()->Int{match 1{_ if True=>1,_=>{let missing=1}}}",
    ]{assert!(!valid(s),"{s}");}
    assert!(valid("import option func f(x:Option[Int])->Int{match x{Some(n) if n>0=>n,Some(_)=>0,None=>0}}"));
    assert!(valid("func f()->Int{match 1{_ if True=>{throw \"one\"},_=>{throw \"two\"}}}"));
}
#[test]
fn guards_evaluate_once_in_order_after_matching_and_stop_at_the_first_true_arm(){
    output(r#"import std import option
        func input()->Option[Int]{std.log("target") Some(2)}
        func mark(label:String,value:Bool)->Bool{std.log(label) value}
        func select()->String{
            match input(){
                None if mark("SKIP",True)=>"bad",
                Some(n) if mark("first",False)=>"bad",
                Some(n) if mark("second",n>0)=>"hit",
                _ if mark("LATE",True)=>"bad",
                _=>"fallback"
            }
        }
        routine main(){
            std.log("function",select())
            match input(){
                None if mark("SKIP",True)=>std.log("bad"),
                Some(n) if mark("first",False)=>std.log("bad"),
                Some(n) if mark("second",n>0)=>std.log("routine",n),
                _ if mark("LATE",True)=>std.log("bad"),
                _=>std.log("fallback")
            }
        }"#,"target\nfirst\nsecond\nfunction hit\ntarget\nfirst\nsecond\nroutine 2\n");
}
#[test]
fn guard_failures_propagate_and_are_never_treated_as_false(){
    output(r#"import std
        func failed()->Bool{throw "guard"}
        func protected()->Int{try{match 1{_ if failed()=>1,_=>0}}catch reason{7}}
        routine main(){
            std.log(protected())
            try{match 1{_ if failed()=>std.log("BAD"),_=>std.log("BAD fallback")}}catch reason{std.log("caught")}
        }"#,"7\ncaught\n");
    for source in [
        "import std func failed()->Bool{1/0 True} routine main(){match 1{_ if failed()=>std.log(1),_=>std.log(0)}}",
        "import std func failed()->Bool{throw \"guard\"} routine main(){match 1{_ if failed()=>std.log(1),_=>std.log(0)}}",
    ]{let out=run(source);assert!(!out.status.success());assert!(out.stdout.is_empty());}
    let p=parse("func f()->Int{match 1{_ if 1=>7,_=>9}}");let router=SignalRouter::default();
    for d in p.declarations{if let TopLevelDecl::Func(f)=d{router.register_function(f);}}
    assert_eq!(router.evaluate_function_result("f",vec![]),Err(EvaluationFailure::Invalid));
}
#[test]
fn guard_only_captures_are_copied_and_ref_captures_are_rejected(){
    output(r#"import std import option
        const n=99
        routine Worker(f:(Int)->Int){std.log("worker",f(3))}
        routine main(){
            let limit=2
            let select=|value:Int|->Int{match value{n if n>limit=>n,_=>0}}
            let limit=100
            match Some(3){Some(n) if n>2=>std.log("pattern",n),_=>std.log(0)}
            let n="outer"
            match Some(4){Some(n) if type n=="Int"=>std.log("shadow",n),_=>std.log("BAD")}
            std.log("outer",n)
            spawn Worker(select)
        }"#,"pattern 3\nshadow 4\nouter outer\nworker 3\n");
    assert!(!valid("routine main(){let captured=ref 1 let f=|n:Int|->Int{match n{x if captured==x=>1,_=>0}}}"));
}
#[test]
fn loop_guards_can_access_the_current_signal_and_private_memory(){
    output(r#"import std import option
        loop Watch(){
            memory state="ready"
            match Some(3){
                Some(n) if memory state!=memory missing=>{
                    match n{
                        x if message.value=="go"=>{std.log(x,message.value) terminate},
                        _=>{terminate}
                    }
                },
                _=>{terminate}
            }
        }
        routine main(){spawn Watch("go")}
    "#,"3 go\n");
}
#[test]
fn guard_calls_participate_in_cycles_termination_and_complexity(){
    for source in [
        "func a()->Bool{match 1{_ if b()=>True,_=>False}} func b()->Bool{a()}",
        "import option func bad()->Bool{match Some(bad){Some(test) if test()=>True,_=>False}}",
        "func bad()->Bool{match 1{_ if (||->Bool{bad()})()=>True,_=>False}}",
    ]{
        let p=resolved(source);TypeChecker::new().check_program(&p).unwrap();
        assert!(CallGraphValidator::new().build_and_validate(&p).is_err(),"{source}");
        assert!(TerminationChecker::verify_program(&p).is_err());
        assert!(ComplexityAnalyzer::new().try_analyze(&p).is_err());
    }
    let p=parse("func bad(xs:List[Int])->Bool{match xs{_ if bad(xs)=>True,_=>False}}");
    TypeChecker::new().check_program(&p).unwrap();assert!(matches!(TerminationChecker::verify_program(&p),Err(SemanticError::NonDecreasingRecursion(_))));
    let p=resolved("import option func work(xs:List[Int])->Bool{for x in xs{for y in xs{y}} True} routine main(){match Some(work){Some(test) if test([1,2])=>0,_=>1}}");
    TypeChecker::new().check_program(&p).unwrap();let entries=ComplexityAnalyzer::new().try_analyze(&p).unwrap();
    assert_eq!(entries.iter().find(|e|e.name=="main").unwrap().complexity,Complexity::Polynomial(2));
}
#[test]
fn guarded_tail_calls_and_decreasing_guard_calls_preserve_structural_proofs(){
    let p=parse("func count(xs:List[Int],acc:Int)->Int{match xs{Cons(n,tail) if n>0=>tailcall count(tail,acc+1),Cons(_,tail)=>tailcall count(tail,acc),Nil=>acc}} func has(xs:List[Int])->Bool{match xs{Cons(_,tail) if has(tail)=>True,Cons(_,_)=>False,Nil=>True}}");
    TypeChecker::new().check_program(&p).unwrap();TerminationChecker::verify_program(&p).unwrap();
    let router=SignalRouter::default();for d in p.declarations{if let TopLevelDecl::Func(f)=d{router.register_function(f);}}
    let list=V::List((0..300).map(V::Int).collect());assert_eq!(router.evaluate_function_result("count",vec![list,V::Int(0)]),Ok(V::Int(299)));
    assert_eq!(router.evaluate_function_result("has",vec![V::List(vec![V::Int(1),V::Int(2)])]),Ok(V::Bool(true)));
    assert!(!valid("func predicate()->Bool{True} func f()->Int{match 1{_ if tailcall predicate()=>1,_=>0}}"));
}
#[test]
fn constants_and_closures_inside_guards_use_pattern_scope(){
    output(r#"import std import option
        const n=100
        const LIMIT=2
        routine main(){
            match Some(3){
                Some(n) if (|x:Int|->Bool{const EXTRA=1 x>LIMIT+EXTRA})(n)=>std.log("BAD"),
                Some(n) if (|x:Int|->Bool{const EXTRA=1 x>=LIMIT+EXTRA})(n)=>std.log(n),
                _=>std.log(0)
            }
        }"#,"3\n");
}
#[test]
fn imported_guards_qualify_calls_types_and_constants_without_rewriting_bindings(){
    let dir=std::env::temp_dir().join(format!("lia-guards-package-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();
    std::fs::write(dir.join("library.lia"),r#"import option
        const LIMIT=2
        type Token=Item(Int)|End
        func check<T>(value:T,n:Int)->Bool{n>LIMIT}
        func inspect(value:Token)->Int{match value{Item(n) if check<Token>(value,n)=>n,_=>0}}
        func closure(value:Token)->Bool{match value{_ if (|x:Token|->Bool{const MIN=LIMIT match x{Item(n) if n>MIN=>True,_=>False}})(value)=>True,_=>False}}
        func apply(test:(Int)->Bool,n:Int)->Int{match Some(test){Some(check) if check(n)=>1,_=>0}}
    "#).unwrap();
    std::fs::write(dir.join("main.lia"),r#"import std import library
        routine main(){std.log(library.inspect(Item(4)),library.inspect(Item(1)),library.closure(Item(3)),library.apply(|n:Int|->Bool{n>0},2))}
    "#).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"4 0 true 1\n");
}
#[test]
fn guarded_source_and_artifacts_execute_and_legacy_cases_default_to_no_guard(){
    let expected="positive zero negative missing\naccepted rejected\nclosure 5 0\ndictionary 7\nworker 4\n";
    output(&std::fs::read_to_string("programs/24_pattern_guards.lia").unwrap(),expected);
    let artifact=std::env::temp_dir().join(format!("lia-guards-{}.liac",std::process::id()));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["compile","programs/24_pattern_guards.lia","-o"]).arg(&artifact).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();std::fs::remove_file(artifact).unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
    let p=parse("func f()->Int{match True{True=>1,False=>0}}");
    let json=serde_json::to_string(&p).unwrap();assert!(!json.contains("guard"));
    let legacy:Program=serde_json::from_str(&json).unwrap();TypeChecker::new().check_program(&legacy).unwrap();assert_eq!(legacy,p);
}
