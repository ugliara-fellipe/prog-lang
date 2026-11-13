use std::collections::HashMap;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use lia::ast::{Statement, TopLevelDecl};
use lia::parser::Parser;
use lia::runtime::actor::{RuntimeValue, SignalRouter};

fn run(source: &str) -> Output {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("lia-generic-runtime-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy("programs/std.lia", dir.join("std.lia")).unwrap();
    let path = dir.join("main.lia");
    std::fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(path).output().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    output
}

#[test]
fn generic_function_runtime_erases_type_parameters_and_preserves_values() {
    let program = Parser::new("func id<T>(value: T) -> T { value } routine main() { id<Int>(3) }").unwrap().parse_program().unwrap();
    let router = SignalRouter::default();
    for decl in &program.declarations {
        if let TopLevelDecl::Func(function) = decl { router.register_function(function.clone()); }
    }
    assert_eq!(router.evaluate_function_result("id", vec![RuntimeValue::Int(7)]), Ok(RuntimeValue::Int(7)));
    assert_eq!(router.evaluate_function_result("id", vec![RuntimeValue::String("hello".into())]), Ok(RuntimeValue::String("hello".into())));
    let TopLevelDecl::Routine(main) = &program.declarations[1] else { unreachable!() };
    let Statement::ExprStmt(expr) = &main.body[0] else { unreachable!() };
    assert_eq!(router.evaluate_expression_result(expr, &HashMap::new()), Ok(RuntimeValue::Int(3)));
}

#[test]
fn specialized_function_values_can_be_returned_and_copied() {
    let source = "func id<T>(value: T) -> T { value } func choose<T>() -> (T) -> T { id<T> }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    let router = SignalRouter::default();
    for decl in program.declarations { if let TopLevelDecl::Func(f) = decl { router.register_function(f); } }
    let value = router.evaluate_function_result("choose", vec![]).unwrap();
    let copy = value.clone();
    assert_eq!(value, RuntimeValue::Function("id".into()));
    let scope = [("copy".into(), copy)].into_iter().collect();
    let calls = Parser::new("routine main() { copy(4) }").unwrap().parse_program().unwrap();
    let TopLevelDecl::Routine(main) = &calls.declarations[0] else { unreachable!() };
    let Statement::ExprStmt(call) = &main.body[0] else { unreachable!() };
    assert_eq!(router.evaluate_expression_result(call, &scope), Ok(RuntimeValue::Int(4)));
}

#[test]
fn cli_runs_generic_calls_callbacks_and_generic_spawn() {
    let output = run(r#"import std
        func id<T>(value: T) -> T { value }
        func apply<T, U>(value: T, f: (T) -> U) -> U { f(value) }
        func twice(value: Int) -> Int { value * 2 }
        func relay<T>(f: (T) -> T) -> (T) -> T { f }
        routine Worker<T>(value: T, f: (T) -> T) { std.log("worker", f(value)) }
        routine main() {
            std.log(id(7), id("hello"), id<Bool>(True))
            let first: (Int) -> Int = id<Int>
            let copy = first
            std.log(apply(4, twice), relay<Int>(copy)(9))
            spawn Worker<Int>(5, id<Int>)
        }
    "#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "7 hello true\n8 9\nworker 5\n");
}

#[test]
fn cli_infers_generic_routine_arguments() {
    let output = run(r#"import std
        func id<T>(value: T) -> T { value }
        routine Worker<T>(value: T, f: (T) -> T) { std.log(f(value)) }
        routine main() { spawn Worker("copied", id<String>) }
    "#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "copied\n");
}

#[test]
fn cli_rejects_reference_instantiation_before_spawn() {
    let output = run(r#"import std
        routine Worker<T>(value: T) { std.log("must-not-run", value) }
        routine main() { let pointer = ref 1 spawn Worker(pointer) }
    "#);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("must-not-run"));
}

#[test]
fn cli_reports_generic_argument_evaluation_failure() {
    let output = run(r#"import std
        func id<T>(value: T) -> T { value }
        routine main() { std.log(id<Int>(1 / 0)) }
    "#);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Runtime error"));
}

#[test]
fn compiled_artifact_preserves_generic_declarations_and_specializations() {
    let dir = std::env::temp_dir().join(format!("lia-generic-artifact-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy("programs/std.lia", dir.join("std.lia")).unwrap();
    let source = dir.join("main.lia");
    let artifact = dir.join("main.liac");
    std::fs::write(&source, r#"import std func id<T>(value: T) -> T { value } routine main() { std.log(id<Int>(7), id<String>("saved")) }"#).unwrap();
    let compiled = Command::new(env!("CARGO_BIN_EXE_lia")).arg("compile").arg(&source).arg("-o").arg(&artifact).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7 saved\n");
}
