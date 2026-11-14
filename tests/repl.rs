use std::{io::Write,process::{Command,Stdio,Output}};
use lia::{repl::Session,runtime::actor::RuntimeValue as V};
fn run(source:&str)->Output{
    let mut child=Command::new(env!("CARGO_BIN_EXE_lia")).arg("repl").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(source.as_bytes()).unwrap();child.wait_with_output().unwrap()
}
fn output(source:&str,expected:&str){let out=run(source);assert!(out.status.success());assert!(out.stderr.is_empty(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);}
#[test]
fn expressions_bindings_types_and_reset_work_without_replaying_effects(){
    output("import std\nlet x=5\nstd.log(\"once\")\nx+2\n:type x\n:bindings\n:reset\n1+1\n:q\n","once\n7\nInt\nx: Int = 5\nSession reset.\n2\n");
}
#[test]
fn multiline_functions_comments_strings_docstrings_and_abort_are_supported(){
    output("// comment\n/// Adds one.\nfunc add(n:Int)->Int {\n n+1\n}\nadd(2)\nlet x =\n4\nx\nlet incomplete = {\n:abort\n\"braces { [ ( in a string\"\n:q\n","3\n4\nbraces { [ ( in a string\n");
    let out=run("func incomplete(\n");assert!(String::from_utf8_lossy(&out.stderr).contains("incomplete input"));
}
#[test]
fn invalid_entries_rollback_and_the_session_continues(){
    let out=run("let x=3\nlet x=1/0\nx\nlet bad:String=1\nmissing\nx+1\n:q\n");assert!(out.status.success());assert_eq!(String::from_utf8_lossy(&out.stdout),"3\n4\n");let e=String::from_utf8_lossy(&out.stderr);assert!(e.contains("invalid expression"));assert!(e.contains("type"));assert!(e.contains("missing"));
    let mut session=Session::new(".");session.submit("let x=3").unwrap();assert!(session.submit("let x=9 let y=1/0").is_err());assert_eq!(session.submit("x").unwrap(),vec![V::Int(3)]);
    assert!(session.submit("func f()->Int{g()}").is_err());session.submit("func f()->Int{2}").unwrap();assert!(session.submit("func f()->Int{3}").is_err());assert_eq!(session.submit("f()").unwrap(),vec![V::Int(2)]);
}
#[test]
fn closures_generics_composition_guards_and_copy_snapshots_persist(){
    output("import dict\nfunc id<T>(x:T)->T{x}\nlet n=2\nlet f=(|x:Int|->Int{x+n}) >>> id<Int>\nlet n=99\nf(3)\nlet d={f:f}\nmatch d[\"f\"]{Some(g) if g(1)>0=>{let local=g(4)},_=>{}}\nf(4)\n:q\n","5\n6\n");
}
#[test]
fn spawn_uses_the_scheduler_and_does_not_reexecute_previous_entries(){
    output("import std\nroutine Worker(n:Int){std.log(\"worker\",n)}\nspawn Worker(3)\nlet n=5\nn\nloop Once(){std.log(\"loop\") terminate}\nspawn Once()\n:q\n","worker 3\n5\nloop\n");
    let out=run("import std\nroutine Fail(){std.log(1/0)}\nspawn Fail()\n2+2\n:q\n");assert_eq!(String::from_utf8_lossy(&out.stdout),"4\n");assert!(String::from_utf8_lossy(&out.stderr).contains("routine 'Fail'"));
}
#[test]
fn references_cannot_escape_entries_and_type_queries_do_not_execute(){
    let mut s=Session::new(".");assert!(s.submit("let r=ref 1").is_err());assert!(s.bindings().is_empty());assert_eq!(s.expression_type("ref 1").unwrap(),"Ref[Int]");
    s.submit("func fail()->Int{throw \"not executed\"}").unwrap();assert_eq!(s.expression_type("fail()").unwrap(),"Int");assert!(s.submit("fail()").is_err());assert_eq!(s.submit("2").unwrap(),vec![V::Int(2)]);
}
#[test]
fn enum_values_remain_usable_when_imports_add_aliases(){
    output("import option.unwrap_or\nlet x=option.Option.Some(7)\nlet f=||->Int{option.unwrap_or(x,0)}\nimport option\noption.unwrap_or(x,0)\nf()\n:q\n","7\n7\n");
}
#[test]
fn constants_and_json_and_termination_checks_apply_to_every_entry(){
    output("import json\nimport result\nconst N=2*3\nN\njson.as_int(3)\n:type N\n:q\n","6\nSome(3)\nInt\n");
    let out=run("const N=1/0\nfunc bad(xs:List[Int])->Int{bad(xs)}\n1\n:q\n");assert_eq!(String::from_utf8_lossy(&out.stdout),"1\n");assert!(String::from_utf8_lossy(&out.stderr).contains("division by zero"));assert!(String::from_utf8_lossy(&out.stderr).contains("strictly smaller"));
}
#[test]
fn imported_packages_are_snapshots_and_failed_imports_do_not_poison_the_cache(){
    let dir=std::env::temp_dir().join(format!("lia-repl-packages-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
    let path=dir.join("library.lia");std::fs::write(&path,"func value()->Int{3}").unwrap();
    let mut s=Session::new(dir.clone());s.submit("import library").unwrap();s.submit("let x=library.value()").unwrap();
    std::fs::write(&path,"func value()->String{\"changed\"}").unwrap();assert_eq!(s.submit("library.value()").unwrap(),vec![V::Int(3)]);assert_eq!(s.expression_type("x").unwrap(),"Int");
    assert!(s.submit("import missing").is_err());std::fs::write(dir.join("missing.lia"),"func other()->Int{7}").unwrap();s.submit("import missing").unwrap();assert_eq!(s.submit("missing.other()").unwrap(),vec![V::Int(7)]);
    std::fs::remove_dir_all(dir).unwrap();
}
#[cfg(unix)]
#[test]
fn ctrl_c_stops_a_session_with_an_active_persistent_loop(){
    use std::{io::{BufRead,BufReader},time::{Duration,Instant}};
    let mut child=Command::new(env!("CARGO_BIN_EXE_lia")).arg("repl").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut input=child.stdin.take().unwrap();
    input.write_all(b"import std\nloop Forever() {}\nstd.log(\"ready\")\nspawn Forever()\n").unwrap();input.flush().unwrap();
    let mut output=BufReader::new(child.stdout.take().unwrap());let mut ready=String::new();output.read_line(&mut ready).unwrap();assert_eq!(ready,"ready\n");
    std::thread::sleep(Duration::from_millis(50));
    assert!(Command::new("kill").args(["-INT",&child.id().to_string()]).status().unwrap().success());
    let deadline=Instant::now()+Duration::from_secs(5);
    loop{
        if let Some(status)=child.try_wait().unwrap(){assert_eq!(status.code(),Some(130));break;}
        if Instant::now()>deadline{child.kill().unwrap();child.wait().unwrap();panic!("REPL ignored Ctrl-C");}
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn dictionary_and_optional_syntax_need_no_session_imports(){
    output("let d:Dict[Int]={value:3}\nd[\"value\"]\nlet empty:Option[Int]=None\nempty\nlet checked:Result[Int,String]=Ok(4)\nchecked\noption.unwrap_or(d[\"missing\"],7)\n:q\n","Some(3)\nNone\nOk(4)\n7\n");
}
