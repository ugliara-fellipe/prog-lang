use std::process::{Command,Output};
use std::sync::atomic::{AtomicUsize,Ordering};
use lia::{ast::*,parser::Parser,semantic::{constants,packages::PackageManager,type_checker::TypeChecker,dag::CallGraphValidator,termination::TerminationChecker,complexity::{Complexity,ComplexityAnalyzer},SemanticError}};
fn parse(s:&str)->Program{Parser::new(s).unwrap().parse_program().unwrap()}
fn resolved(s:&str)->Result<Program,SemanticError>{PackageManager::new("programs").resolve_program(parse(s))}
fn run(s:&str)->Output{
    static NEXT:AtomicUsize=AtomicUsize::new(0);
    let dir=std::env::temp_dir().join(format!("lia-constant-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();std::fs::write(dir.join("main.lia"),s).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();out
}
fn output(s:&str,want:&str){let out=run(s);assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),want);}
#[test]
fn declarations_are_parsed_and_folded_before_semantic_analysis(){
    let raw=parse("const N:Int=M*2 const M=3 func get()->Int{const LOCAL=N+1 LOCAL}");
    assert!(matches!(&raw.declarations[0],TopLevelDecl::Const(c) if c.name=="N"));
    let folded=constants::lower_program(&raw).unwrap();assert!(!constants::needs_lowering(&folded));assert_eq!(folded.declarations.len(),1);
    let TopLevelDecl::Func(f)=&folded.declarations[0] else{panic!()};
    assert!(matches!(&f.body[1],Statement::ExprStmt(Expr::ConstValue{value,value_type}) if **value==Expr::IntLiteral(7) && *value_type==TypeSpec::Primitive("Int".into())));
    TypeChecker::new().check_program(&raw).unwrap();CallGraphValidator::new().build_and_validate(&raw).unwrap();TerminationChecker::verify_program(&raw).unwrap();ComplexityAnalyzer::new().try_analyze(&raw).unwrap();
    assert_eq!(constants::lower_program(&folded).unwrap(),folded);
}
#[test]
fn compile_time_failures_are_diagnosed_even_when_unused(){
    for source in [
        "const N=1/0", "const N=9223372036854775807+1",
        "const N=(0-9223372036854775807-1)/(0-1)",
        "const N=3037000500*3037000500",
        "const N=(0-9223372036854775807-1)-1",
        "const A=B const B=A", "const A=A", "const A=MISSING",
        "const A=1 const A=2", "const A=1 func A()->Int{1}",
        "func A()->Int{1} const A=1", "const True=1",
        "type Maybe=Some(Int)|None const Some=1",
        "const N:String=1", "const N:Ref[Int]=ref 1",
        "const N:(Int)->Int=|x:Int|->Int{x}", "const N=std.log(1)",
        "func value()->Int{2} const N=value()",
        "const N={x:|n:Int|->Int{n}}", "const N=message", "const N=memory value",
        "routine main(){let n=2 const N=n+1}",
        "routine main(n:Int){const N=n}",
        "routine main(){const N=LATER const LATER=1}",
        "const N=1 routine main(){let N=2 const M=N}",
        "routine main(){const False=1}",
        "const D={x:1} const X=D[\"x\"]",
        "const N=True+1", "const N=1==True",
        "const N:List[Int]=[1,\"bad\"]", "const N:Dict[String]={x:1}",
        "routine main(){try{const N=1/0}catch e{}}",
    ]{
        let p=parse(source);
        assert!(matches!(constants::lower_program(&p),Err(SemanticError::ConstantError(_))),"{source}");
        assert!(TypeChecker::new().check_program(&p).is_err(),"{source}");
        assert!(CallGraphValidator::new().build_and_validate(&p).is_err(),"{source}");
        assert!(TerminationChecker::verify_program(&p).is_err(),"{source}");
        assert!(ComplexityAnalyzer::new().try_analyze(&p).is_err(),"{source}");
    }
    for s in ["const mut N=1","const N","routine main(){const mut N=1}"]{assert!(Parser::new(s).unwrap().parse_program().is_err());}
}
#[test]
fn constants_preserve_annotations_and_reject_invalid_uses(){
    for source in [
        "const N:Json=1 routine main(){let x:Int=N}",
        "const N=1 routine main(){N(2)}",
        "const N=1 routine main(){let N=2 let f=N()}",
        "routine main(){match True{True=>{const N=1},False=>{}} let x=N}",
        "routine main(){for i in 1{const N=1} let x=N}",
        "func f()->Int{const N=1}",
        "const N:Json=1 func f()->Int{N}",
        "const ITEMS:List[Json]=[1] routine main(){let x:List[Int]=ITEMS}",
    ]{
        let p=resolved(source);
        assert!(p.is_err() || TypeChecker::new().check_program(&p.unwrap()).is_err(),"{source}");
    }
}
#[test]
fn lexical_shadowing_patterns_closures_and_exceptions_keep_constant_scope(){
    output(r#"import std
        const N=3
        const n=100
        func f(N:Int)->Int{N}
        routine main(){
            const LOCAL=N+1
            let saved=||->Int{LOCAL}
            let LOCAL=9
            std.log(saved(),LOCAL,f(7))
            match 8{n=>std.log(n)}
            match True{True=>{const INNER=N+2 std.log(INNER)},False=>std.log(0)}
            try{const INNER=N+3 std.log(INNER)}catch e{const INNER=N+4 std.log(INNER)}
            for N in 2{std.log(N)}
            std.log(N)
        }"#,"4 9 7\n8\n5\n6\n0\n1\n3\n");
}
#[test]
fn checked_arithmetic_collections_boolean_and_type_constants_work(){
    output(r#"import std import dict import json
        const NEG=0-3
        const EQ={a:1,b:2}=={b:2,a:1}
        const DATA:Dict[Json]={items:[1,"mixed",True],nested:{},empty:[]}
        const TYPE=type DATA
        const EMPTY:List[Int]=[]
        routine main(){
            std.log(NEG,9/2,EQ,2<3,3>=3)
            std.log(json.stringify(DATA),TYPE,type EMPTY)
            std.log(dict.len(DATA))
        }"#,r#"-3 4 true true true
Ok({"empty":[],"items":[1,"mixed",true],"nested":{}}) Dict[Json] List[Int]
3
"#);
}
#[test]
fn fixed_integer_loops_are_lazy_bounded_and_keep_existing_collection_loops(){
    output(r#"import std const N=3
        func f()->Int{for i in N{std.log("f",i)} 7}
        routine main(){for i in 0{std.log("never")} std.log(f()) for i in N{std.log("r",i)}}
    "#,"f 0\nf 1\nf 2\n7\nr 0\nr 1\nr 2\n");
    let out=run("import std routine main(){for i in (0-1){std.log(i)}}");assert!(!out.status.success());assert!(out.stdout.is_empty());
    let p=resolved("const N=3 func fixed()->Int{for i in N{i} 0} func dynamic(n:Int)->Int{for i in n{i} 0}").unwrap();
    TypeChecker::new().check_program(&p).unwrap();let costs=ComplexityAnalyzer::new().try_analyze(&p).unwrap();
    assert_eq!(costs.iter().find(|e|e.name=="fixed").unwrap().complexity,Complexity::Constant);
    assert_eq!(costs.iter().find(|e|e.name=="dynamic").unwrap().complexity,Complexity::Polynomial(1));
}
#[test]
fn source_and_artifact_execute_precomputed_data(){
    let expected="constants 4 true 5\ntypes Int Dict[Int]\ndict Some(4)\nindex 0\nindex 1\nitem 2\nitem 4\nworker 0\nworker 1\n";
    let source=std::fs::read_to_string("programs/22_constants.lia").unwrap();output(&source,expected);
    let artifact=std::env::temp_dir().join(format!("lia-constants-{}.liac",std::process::id()));
    let compiled=Command::new(env!("CARGO_BIN_EXE_lia")).args(["compile","programs/22_constants.lia","-o"]).arg(&artifact).output().unwrap();
    assert!(compiled.status.success(),"{}",String::from_utf8_lossy(&compiled.stderr));
    let p:Program=serde_json::from_slice(&std::fs::read(&artifact).unwrap()).unwrap();assert!(!constants::needs_lowering(&p));assert!(!p.declarations.iter().any(|d|matches!(d,TopLevelDecl::Const(_))));
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();std::fs::remove_file(artifact).unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),expected);
}
#[test]
fn imported_constants_and_factories_keep_package_identity(){
    let dir=std::env::temp_dir().join(format!("lia-constants-package-{}",std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();std::fs::copy("programs/std.lia",dir.join("std.lia")).unwrap();
    std::fs::write(dir.join("settings.lia"),"const BASE=2 const LIMIT=BASE*3 func value()->Int{LIMIT} func captured()->()->Int{const N=LIMIT+1 ||->Int{N}}").unwrap();
    std::fs::write(dir.join("consumer.lia"),"import settings.{LIMIT} const TOTAL=LIMIT+1 func get()->Int{TOTAL}").unwrap();
    std::fs::write(dir.join("main.lia"),r#"import std import settings.{LIMIT} import consumer
        const COMBINED=settings.BASE+consumer.TOTAL
        routine main(){std.log(LIMIT,settings.LIMIT,settings.value(),settings.captured()(),consumer.get(),COMBINED)}
    "#).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert_eq!(String::from_utf8_lossy(&out.stdout),"6 6 6 7 7 9\n");
}
#[test]
fn precomputed_artifacts_cannot_hide_calls_or_fake_types(){
    let ty=TypeSpec::Primitive("Int".into());
    for value in [Expr::FunctionCall{name:"danger".into(),args:vec![]},Expr::Variable("runtime".into()),Expr::StringLiteral("bad".into())]{
        let p=Program{imports:vec![],declarations:vec![TopLevelDecl::Func(FuncDecl{documentation:String::new(),name:"f".into(),type_params:vec![],params:vec![],return_type:Some(ty.clone()),body:vec![Statement::ExprStmt(Expr::ConstValue{value:Box::new(value),value_type:ty.clone()})]})]};
        assert!(TypeChecker::new().check_program(&p).is_err());
        assert!(CallGraphValidator::new().build_and_validate(&p).is_err());
        assert!(TerminationChecker::verify_program(&p).is_err());
        assert!(ComplexityAnalyzer::new().try_analyze(&p).is_err());
        assert!(constants::lower_program(&p).is_err());
    }
}
#[test]
fn large_integer_bounds_do_not_allocate_a_list(){
    output(r#"import std
        const LIMIT=9223372036854775807
        loop Worker(){for i in LIMIT{std.log(i) terminate}}
        routine main(){spawn Worker()}
    "#,"0\n");
}

#[test]
fn integer_ranges_do_not_bypass_structural_recursion_checks(){
    let p=parse("const LIMIT=3 func bad(n:Int)->Int{for i in n{bad(i)} 0} routine main(){bad(LIMIT)}");
    TypeChecker::new().check_program(&p).unwrap();
    assert!(matches!(TerminationChecker::verify_program(&p),Err(SemanticError::NonDecreasingRecursion(_))));
}
#[test]
fn source_and_serialized_constant_failures_happen_before_runtime(){
    let out=run("import std const BAD=1/0 routine main(){std.log(123)}");
    assert!(!out.status.success());assert!(out.stdout.is_empty());assert!(String::from_utf8_lossy(&out.stderr).contains("Compile-time Constant Error"));
    let mut p=resolved("import std routine main(){std.log(123)}").unwrap();
    p.declarations.push(TopLevelDecl::Const(ConstDecl{documentation:String::new(),name:"BAD".into(),type_annot:None,value:Expr::BinaryOp{op:"/".into(),left:Box::new(Expr::IntLiteral(1)),right:Box::new(Expr::IntLiteral(0))}}));
    let artifact=std::env::temp_dir().join(format!("lia-invalid-constant-{}.liac",std::process::id()));
    std::fs::write(&artifact,serde_json::to_vec(&p).unwrap()).unwrap();
    let out=Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();std::fs::remove_file(artifact).unwrap();
    assert!(!out.status.success());assert!(out.stdout.is_empty());assert!(String::from_utf8_lossy(&out.stderr).contains("division by zero"));
}
