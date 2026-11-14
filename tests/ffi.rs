use std::{path::PathBuf,process::{Command,Output},sync::atomic::{AtomicUsize,Ordering}};
use lia::{ast::*,parser::Parser,semantic::{packages::PackageManager,type_checker::TypeChecker,termination::TerminationChecker,complexity::{Complexity,ComplexityAnalyzer}}};
fn parse(s:&str)->Program{Parser::new(s).unwrap().parse_program().unwrap()}
struct Fixture(PathBuf);
impl Fixture{
    fn new()->Self{
        static NEXT:AtomicUsize=AtomicUsize::new(0);
        let dir=std::env::temp_dir().join(format!("lia-ffi-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ops.c"),r#"
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
int64_t add(int64_t a,int64_t b){return a+b;}
bool positive(int64_t n){return n>0;}
int64_t mixed(int64_t a,bool b,const char *s){return a+(b?10:0)+(unsigned char)s[0];}
char *change(char *s){s[0]='X';return s;}
const char *text(void){return "native";}
const char *bad_utf8(void){static const char s[]={ (char)0xff, 0 };return s;}
const char *null_text(void){return NULL;}
uint8_t bad_bool(void){return 42;}
void noop(void){}
int64_t counter(void){static int64_t n=0;return ++n;}
int64_t many(int64_t a,int64_t b,int64_t c,int64_t d,int64_t e,int64_t f,int64_t g,int64_t h){return a+b+c+d+e+f+g+h;}
"#).unwrap();
        let out=Command::new("cc").args(["-shared","-fPIC","ops.c","-o","libops.so"]).current_dir(&dir).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
        Self(dir)
    }
    fn run(&self,source:&str)->Output{std::fs::write(self.0.join("main.lia"),source).unwrap();Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(self.0.join("main.lia")).output().unwrap()}
}
impl Drop for Fixture{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
#[test]
fn typed_c_calls_copy_strings_and_preserve_native_library_state(){
    let f=Fixture::new();let out=f.run(r#"import std
        extern "C" fn add(a:Int,b:Int)->Int from "./libops.so";
        extern "C" fn positive(n:Int)->Bool from "./libops.so";
        extern "C" fn mixed(n:Int,b:Bool,s:String)->Int from "./libops.so";
        extern "C" fn change(s:String)->String from "./libops.so";
        extern "C" fn text()->String from "./libops.so";
        extern "C" fn counter()->Int from "./libops.so";
        extern "C" fn noop() from "./libops.so";
        extern "C" fn many(a:Int,b:Int,c:Int,d:Int,e:Int,f:Int,g:Int,h:Int)->Int from "./libops.so";
        routine main(){let s="abc" std.log(add(2,3),positive(1),positive(0),mixed(2,True,"A")) std.log(change(s),s,text()) std.log(counter(),counter(),many(1,2,3,4,5,6,7,8)) noop()}
    "#);assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"5 true false 77\nXbc abc native\n1 2 36\n");
}
#[test]
fn foreign_functions_pass_return_compose_tailcall_guard_and_spawn(){
    let f=Fixture::new();let out=f.run(r#"import std
        extern "C" fn native(n:Int)->Int as "llabs";
        extern "C" fn positive(n:Int)->Bool from "./libops.so";
        func returned()->(Int)->Int{native}
        func forward(n:Int)->Int{tailcall native(n)}
        routine Worker(f:(Int)->Int){std.log("worker",f(0-3))}
        routine main(){
            let f=returned() >>> (|n:Int|->Int{n+1})
            let d={f:f}
            match d["f"]{Some(g) if positive(g(0-2))=>std.log(g(0-2),forward(0-4)),_=>std.log(0)}
            spawn Worker(f)
        }"#);
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"3 4\nworker 4\n");
}
#[test]
fn invalid_abi_types_arity_and_references_are_rejected(){
    for source in [
        "extern \"Rust\" fn f(n:Int)->Int;",
        "extern \"C\" fn f(n:Ref[Int])->Int;",
        "extern \"C\" fn f(n:List[Int])->Int;",
        "extern \"C\" fn f(n:Dict[Int])->Int;",
        "extern \"C\" fn f(n:(Int)->Int)->Int;",
        "extern \"C\" fn f(n:Unit)->Int;",
        "extern \"C\" fn f()->Option[Int];",
        "extern \"C\" fn f(n:Int,n:Int)->Int;",
        "extern \"C\" fn f(n:Int)->Int; routine main(){f(\"wrong\")}",
        "extern \"C\" fn f(n:Int)->Int; routine main(){f()}",
        "extern \"C\" fn f()->Int; func f()->Int{2}",
    ]{assert!(TypeChecker::new().check_program(&parse(source)).is_err(),"{source}");}
    for s in ["extern \"C\" fn f()->Int {}","extern \"C\" fn f<T>(n:T)->T;","extern \"C\" fn f()->Int"]{assert!(Parser::new(s).unwrap().parse_program().is_err(),"{s}");}
}
#[test]
fn loader_null_and_encoding_failures_are_errors_without_silent_none(){
    let f=Fixture::new();
    for declaration in [
        "extern \"C\" fn missing()->Int from \"./missing.so\";",
        "extern \"C\" fn missing()->Int from \"./libops.so\";",
        "extern \"C\" fn bad()->String from \"./libops.so\" as \"null_text\";",
        "extern \"C\" fn bad()->String from \"./libops.so\" as \"bad_utf8\";",
        "extern \"C\" fn bad()->Bool from \"./libops.so\" as \"bad_bool\";",
    ]{
        let call=if declaration.contains("missing()"){"missing()"}else{"bad()"};
        let out=f.run(&format!("import std {} routine main(){{std.log({})}}",declaration,call));
        assert!(!out.status.success(),"{declaration}");assert!(out.stdout.is_empty());assert!(String::from_utf8_lossy(&out.stderr).contains("FFI:"),"{}",String::from_utf8_lossy(&out.stderr));
    }
    let p=parse("extern \"C\" fn change(s:String)->String;");
    let TopLevelDecl::Func(func)=&p.declarations[0]else{panic!()};
    assert!(lia::ffi::invoke(func,&[lia::runtime::actor::RuntimeValue::String(format!("a{}b",char::from(0)))]).unwrap_err().contains("NUL"));
}
#[test]
fn compilation_does_not_load_libraries_and_artifacts_keep_signatures(){
    let f=Fixture::new();let source=r#"import std
        /// Native addition.
        extern "C" fn sum(a:Int,b:Int)->Int from "./libops.so" as "add";
        routine main(){std.log(sum(5,7))}
    "#;std::fs::write(f.0.join("main.lia"),source).unwrap();
    let artifact=f.0.join("main.liac");
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("compile").arg(f.0.join("main.lia")).arg("-o").arg(&artifact).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    std::fs::remove_file(f.0.join("main.lia")).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(artifact).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"12\n");
    let p=PackageManager::new(&f.0).resolve_program(parse("extern \"C\" fn absent()->Int from \"./not-present.so\";")).unwrap();TypeChecker::new().check_program(&p).unwrap();TerminationChecker::verify_program(&p).unwrap();
}
#[test]
fn native_complexity_is_unknown_and_propagates_through_composition(){
    let p=parse("extern \"C\" fn native(n:Int)->Int; func other(n:Int)->Int{n+1} routine main(){let f=native >>> other f(1)}");
    let entries=ComplexityAnalyzer::new().try_analyze(&p).unwrap();
    for name in ["native","main"]{assert_eq!(entries.iter().find(|e|e.name==name).unwrap().complexity,Complexity::Unknown);}
}
#[test]
fn imported_native_symbols_keep_original_names_and_module_relative_paths(){
    let f=Fixture::new();std::fs::create_dir(f.0.join("module")).unwrap();std::fs::copy(f.0.join("libops.so"),f.0.join("module/libops.so")).unwrap();
    std::fs::write(f.0.join("module/mod.lia"),"extern \"C\" fn add(a:Int,b:Int)->Int from \"./libops.so\";").unwrap();
    let out=f.run("import std import module.{add} routine main(){std.log(module.add(1,2),add(3,4))}");
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"3 7\n");
}
#[test]
fn repl_docs_and_annotations_support_foreign_declarations(){
    use std::io::Write;
    let mut child=Command::new(env!("CARGO_BIN_EXE_lia")).arg("repl").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"extern \"C\" fn absolute(n:Int)->Int as \"llabs\";\nabsolute(0-9)\n:type absolute\n:q\n").unwrap();
    let out=child.wait_with_output().unwrap();assert!(out.status.success());assert!(out.stderr.is_empty(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"9\n(Int) -> Int\n");
    let f=Fixture::new();std::fs::write(f.0.join("main.lia"),"/// Native absolute value.\nextern \"C\" fn absolute(n:Int)->Int as \"llabs\";").unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("doc").arg(f.0.join("main.lia")).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));let docs=String::from_utf8_lossy(&out.stdout);assert!(docs.contains("extern \"C\" fn absolute"));assert!(docs.contains("Native absolute value."));
    let annotated=f.0.join("annotated.lia");let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("annotate").arg(f.0.join("main.lia")).arg("-o").arg(&annotated).output().unwrap();assert!(out.status.success());assert!(std::fs::read_to_string(annotated).unwrap().contains("// complexity: unknown (foreign code)"));
}
