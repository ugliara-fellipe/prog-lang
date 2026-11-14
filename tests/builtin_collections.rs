use std::process::Command;
use lia::{ast::*,parser::Parser,semantic::{packages::PackageManager,type_checker::TypeChecker,dag::CallGraphValidator,termination::TerminationChecker,complexity::ComplexityAnalyzer}};
fn parse(s:&str)->Program{Parser::new(s).unwrap().parse_program().unwrap()}
#[test]
fn raw_type_checking_and_analyses_know_dictionary_and_optional_syntax(){
    let p=parse("func find(d:Dict[Int])->Option[Int]{d[\"key\"]} func checked()->Result[Int,String]{Ok(1)} routine main(){let x:Option[Int]=Some(1) let y:Option[Int]=None let d=dict.set({key:3},\"next\",4) find(d)}");
    TypeChecker::new().check_program(&p).unwrap();CallGraphValidator::new().build_and_validate(&p).unwrap();TerminationChecker::verify_program(&p).unwrap();ComplexityAnalyzer::new().try_analyze(&p).unwrap();
}
#[test]
fn prelude_is_idempotent_and_legacy_imports_still_work(){
    let p=PackageManager::new("/tmp/no-lia-files-needed").resolve_program(parse("import dict import option import result routine main(){let x:Option[Int]=None let d:Dict[Int]={} d[\"x\"]}")).unwrap();
    TypeChecker::new().check_program(&p).unwrap();
    assert!(lia::semantic::builtins::augment(&p).unwrap().is_none());
    for n in ["option.Option","result.Result","dict.get"]{assert_eq!(p.declarations.iter().filter(|d|match d{TopLevelDecl::Func(f)=>f.name==n,TopLevelDecl::Type(t)=>t.name==n,_=>false}).count(),1);}
}
#[test]
fn user_enums_can_shadow_implicit_short_constructors(){
    let p=parse("type Maybe<T>=Some(T)|None func custom(x:Maybe[Int])->Int{match x{Some(n)=>n,None=>0}} func optional(x:Option[Int])->Int{match x{Option.Some(n)=>n,Option.None=>0}} routine main(){custom(Some(2)) optional(Option.Some(3))}");
    TypeChecker::new().check_program(&p).unwrap();
    let p=PackageManager::new(".").resolve_program(p).unwrap();TypeChecker::new().check_program(&p).unwrap();TerminationChecker::verify_program(&p).unwrap();
}
#[test]
fn optional_callbacks_and_dictionary_callbacks_keep_cycle_and_copy_checks(){
    let p=parse("func bad()->Bool{match Some(bad){Some(f) if f()=>True,_=>False}}");TypeChecker::new().check_program(&p).unwrap();assert!(CallGraphValidator::new().build_and_validate(&p).is_err());
    for s in ["routine Worker(d:Dict[Ref[Int]]){} routine main(){spawn Worker({x:ref 1})}","routine main(){let x:Option[Int]=Some(\"wrong\")}","routine main(){let x:Result[Int,String]=Err(2)}"]{assert!(TypeChecker::new().check_program(&parse(s)).is_err(),"{s}");}
}
#[test]
fn standalone_source_and_compiled_artifacts_run_without_collection_imports(){
    let expected="lookup Some(3) None\nresult Ok(8) Err(missing)\ncopy 3 1\nworker 8\n";
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["run","programs/26_builtin_collections.lia"]).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
    let artifact=std::env::temp_dir().join(format!("lia-builtins-{}.liac",std::process::id()));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).args(["compile","programs/26_builtin_collections.lia","-o"]).arg(&artifact).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();std::fs::remove_file(artifact).unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
}
#[test]
fn imported_modules_keep_their_own_builtin_scope_and_generic_binders(){
    let dir=std::env::temp_dir().join(format!("lia-builtin-scope-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("library.lia"),"func find(d:Dict[Int])->Option[Int]{d[\"key\"]} func make()->Option[Int]{Some(2)}").unwrap();
    let raw=parse("import library type E=Error type T=Tag type Maybe<A>=Some(A)|None routine main(){let d={key:1} let value:option.Option[Int]=library.make() match library.find(d){option.Option.Some(n)=>n,option.Option.None=>0}}");
    let p=PackageManager::new(&dir).resolve_program(raw).unwrap();TypeChecker::new().check_program(&p).unwrap();TerminationChecker::verify_program(&p).unwrap();std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn old_artifacts_with_only_optional_declarations_keep_their_generic_binders(){
    let mut p=PackageManager::new(".").resolve_program(parse("import option.unwrap_or routine main(){let x:option.Option[Int]=Some(1) option.unwrap_or(x,0)}")).unwrap();
    p.declarations.retain(|d|match d{TopLevelDecl::Func(f)=>!f.name.starts_with("result.")&&!f.name.starts_with("dict."),TopLevelDecl::Type(t)=>t.name!="Option"&&t.name!="Result"&&!t.name.starts_with("result."),_=>true});
    let mut json=serde_json::to_value(p).unwrap();
    fn old_names(v:&mut serde_json::Value){match v{serde_json::Value::String(s)=>{if let Some(name)=s.strip_prefix(&format!("{}builtin_",char::from(0))){*s=name.to_owned();}},serde_json::Value::Array(a)=>for v in a{old_names(v);},serde_json::Value::Object(o)=>for v in o.values_mut(){old_names(v);},_=>{}}}
    old_names(&mut json);let old:Program=serde_json::from_value(json).unwrap();
    let upgraded=lia::semantic::builtins::augment(&old).unwrap().unwrap();TypeChecker::new().check_program(&upgraded).unwrap();TerminationChecker::verify_program(&upgraded).unwrap();
}
