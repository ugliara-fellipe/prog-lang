use std::{path::PathBuf,process::{Command,Output},sync::atomic::{AtomicUsize,Ordering}};
use lia::{ast::*,parser::Parser,semantic::{packages::PackageManager,type_checker::TypeChecker,termination::TerminationChecker,complexity::{Complexity,ComplexityAnalyzer}}};
fn parse(s:&str)->Program{Parser::new(s).unwrap().parse_program().unwrap()}
struct Fixture(PathBuf);
impl Fixture{
    fn new(source:&str)->Self{
        static NEXT:AtomicUsize=AtomicUsize::new(0);
        let dir=std::env::temp_dir().join(format!("lia-externs-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();std::fs::write(dir.join("main.lia"),source).unwrap();Self(dir)
    }
    fn command(&self,args:&[&str])->Output{Command::new(env!("CARGO_BIN_EXE_lia")).args(args).output().unwrap()}
    fn report(&self,args:&[&str])->Output{Command::new(env!("CARGO_BIN_EXE_lia")).arg("externs").arg(self.0.join("main.lia")).args(args).output().unwrap()}
}
impl Drop for Fixture{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn json(out:&Output)->serde_json::Value{assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));serde_json::from_slice(&out.stdout).unwrap()}
#[test]
fn costs_parse_validate_and_propagate_through_callbacks_and_loops(){
    let p=parse("extern \"C\" fn native(n:Int)->Int cost \"O(N^2)\"; func apply(f:(Int)->Int,n:Int)->Int{f(n)} routine main(){for n in 3{apply(native,n)}}");
    let TopLevelDecl::Func(f)=&p.declarations[0]else{panic!()};assert_eq!(f.foreign.as_ref().unwrap().cost.as_deref(),Some("O(N^2)"));
    TypeChecker::new().check_program(&p).unwrap();TerminationChecker::verify_program(&p).unwrap();
    let costs=ComplexityAnalyzer::new().try_analyze(&p).unwrap();
    for n in ["native","apply","main"]{assert_eq!(costs.iter().find(|e|e.name==n).unwrap().complexity,Complexity::Polynomial(2));}
    let p=parse("extern \"C\" fn native(n:Int)->Int cost \"O(N)\"; routine main(limit:Int){for n in limit{native(n)}}");
    let costs=ComplexityAnalyzer::new().try_analyze(&p).unwrap();assert_eq!(costs.iter().find(|e|e.name=="main").unwrap().complexity,Complexity::Polynomial(2));
    let p=parse("extern \"C\" fn text()->String cost \"O(1)\";");
    assert_eq!(ComplexityAnalyzer::new().try_analyze(&p).unwrap().iter().find(|e|e.name=="text").unwrap().complexity,Complexity::Constant);
}
#[test]
fn invalid_costs_are_rejected_and_extreme_degrees_do_not_overflow(){
    for cost in ["O(N^0)","O(N^-1)","O(N^4294967296)","O(N log N)","fast","O(2)"]{
        let p=parse(&format!("extern \"C\" fn f()->Int cost {:?};",cost));assert!(TypeChecker::new().check_program(&p).is_err(),"{cost}");
    }
    let p=parse("extern \"C\" fn f()->Int cost \"O(N^4294967295)\"; routine main(n:Int){for x in n{f()}}");
    let entries=ComplexityAnalyzer::new().try_analyze(&p).unwrap();assert_eq!(entries.iter().find(|e|e.name=="main").unwrap().complexity,Complexity::Unknown);
}
#[test]
fn listing_tracks_indirect_calls_spawn_and_unused_bindings_without_loading_libraries(){
    let f=Fixture::new(r#"extern "C" fn direct(n:Int)->Int from "./missing.so" cost "O(1)";
extern "C" fn callback(n:Int)->Int from "./missing.so";
extern "C" fn unused()->Int from "./missing.so";
routine Worker(f:(Int)->Int){f(3)}
routine main(){direct(1) let c=callback >>> (|n:Int|->Int{n+1}) spawn Worker(c)}
"#);
    let rows=json(&f.report(&["--json"]));let rows=rows.as_array().unwrap();assert_eq!(rows.len(),2);
    let callback=rows.iter().find(|r|r["name"]=="callback").unwrap();assert!(callback["declared_cost"].is_null());assert!(callback["effective_cost"].as_str().unwrap().contains("unknown"));assert_eq!(callback["used"],true);assert_eq!(callback["termination"],"assumed by programmer contract");
    let direct=rows.iter().find(|r|r["name"]=="direct").unwrap();assert_eq!(direct["declared_cost"],"O(1)");assert_eq!(direct["line"],1);assert!(direct["source"].as_str().unwrap().ends_with("main.lia"));assert!(direct["callers"].as_array().unwrap().iter().any(|c|c=="main"));
    let all=json(&f.report(&["--all","--json"]));assert_eq!(all.as_array().unwrap().len(),3);assert_eq!(all.as_array().unwrap().iter().find(|r|r["name"]=="unused").unwrap()["used"],false);
    let text=f.report(&[]);assert!(text.status.success());let text=String::from_utf8(text.stdout).unwrap();assert!(text.contains("UNDECLARED"));assert!(text.contains("missing.so"));assert!(!text.contains("extern unused"));
}
#[test]
fn imports_artifacts_and_custom_entries_keep_cost_and_origin(){
    let f=Fixture::new("import module routine Other(){module.slow(2)} routine main(){}");
    std::fs::write(f.0.join("module.lia"),"/// Native slow function.\nextern \"C\" fn slow(n:Int)->Int from \"./missing.so\" as \"external_slow\" cost \"O(N^3)\";").unwrap();
    assert!(json(&f.report(&["--json"])).as_array().unwrap().is_empty());
    let rows=json(&f.report(&["--entry","Other","--json"]));let row=&rows[0];assert_eq!(row["name"],"module.slow");assert_eq!(row["symbol"],"external_slow");assert_eq!(row["declared_cost"],"O(N^3)");assert_eq!(row["line"],2);assert!(row["source"].as_str().unwrap().ends_with("module.lia"));
    let artifact=f.0.join("main.liac");let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("compile").arg(f.0.join("main.lia")).arg("-o").arg(&artifact).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    std::fs::remove_file(f.0.join("main.lia")).unwrap();std::fs::remove_file(f.0.join("module.lia")).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("externs").arg(artifact).args(["--entry","Other","--json"]).output().unwrap();assert_eq!(json(&out),rows);
}
#[test]
fn library_reports_exports_and_signalled_loops_are_included(){
    let f=Fixture::new("extern \"C\" fn native()->Int cost \"O(1)\"; func exported()->Int{native()}");
    assert!(!json(&f.report(&["--json"])).as_array().unwrap().is_empty());
    let f=Fixture::new("extern \"C\" fn native()->Int; loop Watch(){native() terminate} routine main(){signal Watch(1)}");
    let rows=json(&f.report(&["--json"]));assert_eq!(rows[0]["name"],"native");assert!(rows[0]["callers"].as_array().unwrap().iter().any(|c|c=="Watch"));
}
#[test]
fn legacy_metadata_defaults_and_documentation_preserve_the_contract(){
    let raw=r#"{"abi":"C","library":null,"symbol":"llabs"}"#;
    let foreign:ForeignDecl=serde_json::from_str(raw).unwrap();assert!(foreign.cost.is_none());assert!(foreign.source.is_none());
    let f=Fixture::new("/// Absolute value.\nextern \"C\" fn abs(n:Int)->Int as \"llabs\" cost \"O(1)\"; routine main(){abs(0-2)}");
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("doc").arg(f.0.join("main.lia")).output().unwrap();assert!(out.status.success());assert!(String::from_utf8_lossy(&out.stdout).contains("cost \"O(1)\""));
    let p=PackageManager::new(&f.0).resolve_source(parse(&std::fs::read_to_string(f.0.join("main.lia")).unwrap()),&f.0.join("main.lia")).unwrap();let rows=lia::foreign_report::inventory(&p,None,false).unwrap();assert_eq!(rows[0].line,Some(2));
    assert!(!f.report(&["--entry","missing"]).status.success());
    for args in [vec!["externs"],vec!["externs","file","--entry"],vec!["externs","file","--bad"]]{assert!(!f.command(&args).status.success());}
}
