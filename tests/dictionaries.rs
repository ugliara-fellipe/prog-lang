use std::process::{Command,Output};
use std::sync::atomic::{AtomicUsize,Ordering};
use lia::{ast::TopLevelDecl,parser::Parser,semantic::{packages::PackageManager,type_checker::TypeChecker,dag::CallGraphValidator,termination::TerminationChecker,complexity::{Complexity,ComplexityAnalyzer}}};
fn parse(s:&str)->lia::ast::Program{Parser::new(s).unwrap().parse_program().unwrap()}
fn resolved(s:&str)->lia::ast::Program{PackageManager::new("programs").resolve_program(parse(s)).unwrap()}
fn valid(s:&str)->bool{TypeChecker::new().check_program(&resolved(s)).is_ok()}
fn run(s:&str)->Output{
    static NEXT:AtomicUsize=AtomicUsize::new(0);
    let dir=std::env::temp_dir().join(format!("lia-dict-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();std::fs::write(dir.join("main.lia"),s).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();out
}
fn output(source:&str,expected:&str){let out=run(source);assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);}
#[test]
fn source_and_artifact_execute_dictionary_copies_and_json(){
    let expected="copy 3 8\nmissing true 0\ncapture 3\ncallback 5\njson 5 Some(Lia)\nnull true\ninvalid true\nkey a\nkey b\nworker 8\n";
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["run","programs/21_dictionary_json.lia"]).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
    let artifact=std::env::temp_dir().join(format!("lia-dict-{}.liac",std::process::id()));let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["compile","programs/21_dictionary_json.lia","-o"]).arg(&artifact).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();std::fs::remove_file(artifact).unwrap();assert!(out.status.success());assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
}
#[test]
fn invalid_keys_payloads_and_non_json_values_are_rejected(){
    for source in [
        "import dict routine main(){let d={x:1} let v=d[1]}",
        "import dict routine main(){dict.set({x:1},\"x\",\"bad\")}",
        "import dict routine main(){let d:Dict[String]={x:1}}",
        "import dict routine main(){let d:Dict[Int,String]={}}",
        "import dict routine main(){let d:Dict[Int]={x:1} let v=d.missing}",
        "import json routine main(){json.stringify(ref 1)}",
        "import json func f()->Int{1} routine main(){json.stringify({x:f})}",
        "import json routine main(){let x:Json=||->Int{1}}",
        "import json func serialize<T>(x:T)->Result[String,String]{json.stringify(x)}",
        "import dict routine Worker(d:Dict[Ref[Int]]){} routine main(){spawn Worker({x:ref 1})}",
        "import dict routine main(){let d={x:ref 1} let f=||->Int{dict.len(d)}}",
        "import dict func f()->Int{{x:missing()} 1}",
        "import dict routine main(){let d={x:message}}",
    ]{assert!(!valid(source),"{source}");}
    for source in ["routine main(){let d={x:1,x:2}}","routine main(){let d={1:2}}"]{
        assert!(Parser::new(source).unwrap().parse_program().is_err(),"{source}");
    }
}
#[test]
fn empty_nested_and_heterogeneous_json_literals_are_contextual(){
    output(r#"import std import dict import json
    routine main(){
        let d:Dict[Int]={}
        let mixed={n:1,s:"text",b:True}
        let inferred={items:[1,"mixed",True],empty:[]}
        let nested:Dict[Json]={empty:[],obj:{},list:[{},[],True]}
        std.log(dict.len(d),dict.len(mixed),json.stringify(nested))
        let a=dict.empty<Int>()
        let b:Dict[Json]=dict.empty()
        std.log(dict.len(a),dict.len(dict.set(b,"n",1)))
    }"#,"0 3 Ok({\"empty\":[],\"list\":[{},[],true],\"obj\":{}})\n0 1\n");
}
#[test]
fn json_preserves_numbers_unicode_null_and_reports_conversion_errors(){
    output(r#"import std import json import dict import option import result
    routine main(){
        let text="{\"large\":123456789012345678901234567890,\"decimal\":1.234567890123456789,\"unicode\":\"\\u00e7\",\"null\":null}"
        match json.parse_dict(text){Ok(d)=>{
            std.log(json.stringify(d))
            std.log(json.is_null(option.unwrap_or(d["null"],json.null())),option.is_none(d["missing"]))
        },Err(e)=>std.log(e)}
        std.log(result.is_err(json.parse("bad")),result.is_err(json.parse_dict("[]")))
        std.log(json.as_int(3),json.as_string(3),json.as_bool(True),json.as_array([1,2]),json.as_dict({x:1}))
    }"#,"Ok({\"decimal\":1.234567890123456789,\"large\":123456789012345678901234567890,\"null\":null,\"unicode\":\"ç\"})\ntrue true\ntrue true\nSome(3) None Some(true) Some([1, 2]) Some({\"x\": 1})\n");
}
#[test]
fn dictionary_native_aliases_and_function_values_work(){
    output(r#"import std.{log} import dict.{empty,get,set,keys,values,has,remove,len}
    routine main(){
        let make:()->Dict[Int]=dict.empty<Int>
        let d=set(make(),"b",2)
        let e=set(d,"a",1)
        std.log(get(e,"a"),keys(e),values(e),has(e,"b"),len(remove(e,"b")),len(empty<Int>()))
    }"#,"Some(1) [a, b] [1, 2] true 1 0\n");
}
#[test]
fn dictionary_callbacks_are_included_in_cycles_and_complexity(){
    for source in [
        "import dict func bad(n:Int)->Int{match {f:bad}[\"f\"]{Some(f)=>f(n),None=>0}}",
        "import dict func bad(n:Int)->Int{let d=dict.set(dict.empty<(Int)->Int>(),\"f\",bad) match dict.get(d,\"f\"){Some(f)=>f(n),None=>0}}",
        "import dict func bad(n:Int)->Int{for f in dict.values({f:bad}){f(n)} 0}",
        "import dict func bad(n:Int)->Int{let d={f:|x:Int|->Int{bad(x)}} match d[\"f\"]{Some(f)=>f(n),None=>0}}",
        "import dict func bad(n:Int)->Int{let d={x:1} let v=d[key(n)] 0} func key(n:Int)->String{bad(n) \"x\"}",
    ]{let p=resolved(source);TypeChecker::new().check_program(&p).unwrap();assert!(CallGraphValidator::new().build_and_validate(&p).is_err(),"{source}");assert!(TerminationChecker::verify_program(&p).is_err());assert!(ComplexityAnalyzer::new().try_analyze(&p).is_err());}
    let p=resolved("import dict func work(xs:List[Int])->Int{for x in xs{for y in xs{y}} 1} routine main(){match {f:work}[\"f\"]{Some(f)=>f([1,2]),None=>0}}");
    TypeChecker::new().check_program(&p).unwrap();let c=ComplexityAnalyzer::new().try_analyze(&p).unwrap();assert_eq!(c.iter().find(|e|e.name=="main").unwrap().complexity,Complexity::Polynomial(2));
}
#[test]
fn native_stringify_refuses_unvalidated_runtime_callables(){
    use lia::runtime::actor::{RuntimeValue as V,SignalRouter};
    let p=resolved("import json");let router=SignalRouter::default();router.register_enums(&p);
    for d in p.declarations {if let TopLevelDecl::Func(f)=d{router.register_function(f);}}
    let result=router.evaluate_function_result("json.stringify",vec![V::Function("not_json".into())]).unwrap();
    assert!(matches!(result,V::Enum{variant,..} if variant=="Err"));
}
#[test]
fn dictionary_evaluation_does_not_hide_failures(){
    for s in [
        "import std import dict routine main(){std.log({x:1/0})}",
        "import std import dict routine main(){let d={x:1} std.log(d[key()])} func key()->String{throw \"failure\"}",
        "import std import dict routine Worker(d:Dict[Int]){} routine main(){spawn Worker({x:1/0})}",
    ]{let out=run(s);assert!(!out.status.success());assert!(out.stdout.is_empty());}
}
#[test]
fn package_factories_return_callbacks_and_tail_calls_through_dictionaries(){
    let dir=std::env::temp_dir().join(format!("lia-dict-package-{}",std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();
    std::fs::write(dir.join("factory.lia"),r#"import dict
        func increment(x:Int)->Int{x+1}
        func callbacks()->Dict[(Int)->Int]{{next:increment}}
        func captured(offset:Int)->Dict[(Int)->Int]{{next:|x:Int|->Int{x+offset}}}
        func choose(d:Dict[(Int)->Int],n:Int)->Int{match d["next"]{Some(f)=>tailcall f(n),None=>0}}
    "#).unwrap();
    std::fs::write(dir.join("main.lia"),r#"import std import factory
        routine Worker(d:Dict[(Int)->Int]){std.log(factory.choose(d,3))}
        routine main(){let a=factory.callbacks() let b=factory.captured(10)
            std.log(factory.choose(a,1),factory.choose(b,2))
            spawn Worker(b)
        }"#).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"2 12
13
");
}
#[test]
fn dictionary_updates_and_removals_preserve_nested_snapshots(){
    output(r#"import std import dict import option
        routine main(){
            let old={a:{n:1}}
            let copy=old
            let changed=dict.set(old,"a",{n:2})
            let erased=dict.remove(changed,"a")
            std.log(copy,changed,erased,dict.has(copy,"a"))
            let calls={f:||->Int{7}}
            let newer=dict.set(calls,"f",||->Int{9})
            match calls["f"]{Some(f)=>std.log(f()),None=>std.log(0)}
            match newer["f"]{Some(f)=>std.log(f()),None=>std.log(0)}
        }"#,r#"{"a": {"n": 1}} {"a": {"n": 2}} {} true
7
9
"#);
}
