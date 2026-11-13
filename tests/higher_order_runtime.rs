use std::collections::HashMap;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use lia::ast::TopLevelDecl;
use lia::parser::Parser;
use lia::runtime::actor::{EvaluationFailure, RuntimeValue, SignalRouter};

fn router(source: &str) -> SignalRouter {
    let router = SignalRouter::default();
    for declaration in Parser::new(source).unwrap().parse_program().unwrap().declarations {
        if let TopLevelDecl::Func(function) = declaration { router.register_function(function); }
    }
    router
}

fn run(source: &str) -> Output {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("lia-higher-order-runtime-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy("programs/std.lia", dir.join("std.lia")).unwrap();
    let path = dir.join("main.lia");
    std::fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(path).output().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    output
}

#[test]
fn function_values_are_copied_returned_and_invoked() {
    let router = router(r#"
        func double(x: Int) -> Int { x * 2 }
        func choose() -> (Int) -> Int { double }
        func apply(f: (Int) -> Int, x: Int) -> Int { f(x) }
        func returned_call() -> Int { choose()(7) }
    "#);
    let function = router.evaluate_function_result("choose", vec![]).unwrap();
    let copy = function.clone();
    assert_eq!(router.evaluate_function_result("apply", vec![copy, RuntimeValue::Int(3)]), Ok(RuntimeValue::Int(6)));
    assert_eq!(router.evaluate_function_result("apply", vec![function, RuntimeValue::Int(4)]), Ok(RuntimeValue::Int(8)));
    assert_eq!(router.evaluate_function_result("returned_call", vec![]), Ok(RuntimeValue::Int(14)));
}

#[test]
fn throws_cross_indirect_calls_but_invalid_evaluation_is_not_caught() {
    let router = router(r#"
        func fail(x: Int) -> Int { throw x }
        func divide(x: Int) -> Int { 1 / x }
        func apply(f: (Int) -> Int, x: Int) -> Int { f(x) }
        func caught() -> Int { try { apply(fail, 9) } catch error { error } }
        func invalid() -> Int { try { apply(divide, 0) } catch error { 9 } }
    "#);
    assert_eq!(router.evaluate_function_result("caught", vec![]), Ok(RuntimeValue::Int(9)));
    assert_eq!(router.evaluate_function_result("invalid", vec![]), Err(EvaluationFailure::Invalid));
    assert_eq!(router.evaluate_function_result("fail", vec![RuntimeValue::Int(9)]), Err(EvaluationFailure::Thrown(RuntimeValue::Int(9))));
    let expr = Parser::new("routine main() { missing(1) }").unwrap().parse_program().unwrap();
    let TopLevelDecl::Routine(main) = &expr.declarations[0] else { unreachable!() };
    let lia::ast::Statement::ExprStmt(call) = &main.body[0] else { unreachable!() };
    assert_eq!(router.evaluate_expression_result(call, &HashMap::new()), Err(EvaluationFailure::Invalid));
}

#[test]
fn cli_executes_callbacks_copies_returned_calls_and_spawn() {
    let output = run(r#"import std
        func double(x: Int) -> Int { x * 2 }
        func choose() -> (Int) -> Int { double }
        func apply(f: (Int) -> Int, x: Int) -> Int { f(x) }
        routine worker(f: (Int) -> Int) { std.log("worker", f(5)) }
        routine main() {
            let f = choose()
            let copy = f
            std.log("local", apply(copy, 3), choose()(4))
            spawn worker(f)
        }
    "#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("local 6 8"), "{stdout}");
    assert!(stdout.contains("worker 10"), "{stdout}");
}

#[test]
fn cli_reports_failed_spawn_arguments_instead_of_inserting_none() {
    let output = run(r#"import std
        func divide(x: Int) -> Int { 1 / x }
        routine worker(value: Int) { std.log("worker-ran", value) }
        routine main() { spawn worker(divide(0)) }
    "#);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Runtime error"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("worker-ran"));
}

#[test]
fn cli_catches_exceptions_from_callback_in_a_routine() {
    let output = run(r#"import std
        func fail(x: Int) -> Int { throw x }
        func apply(f: (Int) -> Int, x: Int) -> Int { f(x) }
        routine main() {
            try { apply(fail, 9) } catch error { std.log("caught", error) }
        }
    "#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("caught 9"));
}

#[test]
fn cli_does_not_swallow_failed_expression_statements_or_catch_invalid_evaluation() {
    for body in ["divide(0)", "try { divide(0) } catch error { std.log(\"caught-invalid\") }"] {
        let source = format!("import std\n func divide(x: Int) -> Int {{ 1 / x }} routine main() {{ {body} }}");
        let output = run(&source);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Runtime error"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("caught-invalid"));
    }
}

#[test]
fn cli_executes_function_values_in_list_patterns_and_loop_scopes() {
    let output = run(r#"import std
        func double(x: Int) -> Int { x * 2 }
        func triple(x: Int) -> Int { x * 3 }
        routine main() {
            for f in [double, triple] { std.log("loop", f(2)) }
            match [double] { Cons(f, tail) => std.log("pattern", f(4)), _ => std.log("empty") }
        }
    "#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("loop 4"), "{stdout}");
    assert!(stdout.contains("loop 6"), "{stdout}");
    assert!(stdout.contains("pattern 8"), "{stdout}");
}

#[test]
fn runtime_rejects_main_parameters_instead_of_dispatching_unbound_values() {
    let output = run("routine main(value: Int) { value }");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("main"), "{stderr}");
}

#[test]
fn serialized_artifact_cannot_bypass_indirect_cycle_validation() {
    let source = "func recurse(x: Int) -> Int { let f = recurse f(x) } routine main() { recurse(1) }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    let path = std::env::temp_dir().join(format!("lia-unverified-indirect-cycle-{}.liac", std::process::id()));
    std::fs::write(&path, serde_json::to_vec(&program).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&path).output().unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.to_lowercase().contains("cycle"), "{stderr}");
}

#[test]
fn cli_evaluates_std_log_arguments_before_printing_and_reports_errors() {
    let output = run(r#"import std
        func divide(x: Int) -> Int { 1 / x }
        routine main() { std.log("must-not-print", divide(0)) }
    "#);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Runtime error"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("must-not-print"));
}

#[test]
fn cli_calls_imported_std_alias_as_a_function_value() {
    let output = run(r#"import std
        func apply(f: (Int, Int) -> Int, x: Int, y: Int) -> Int { f(x, y) }
        routine main() { std.log("maximum", apply(max, 3, 9)) }
    "#);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("maximum 9"));
}

#[test]
fn cli_does_not_copy_a_reference_to_a_callback_into_spawn() {
    let output = run(r#"import std
        func double(x: Int) -> Int { x * 2 }
        routine worker(f: (Int) -> Int) { std.log("worker-ran", f(3)) }
        routine main() { let f = ref double spawn worker(f) }
    "#);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("worker-ran"));
}

#[test]
fn qualified_function_references_and_calls_respect_local_namespace_shadowing() {
    use lia::ast::{Expr, Statement};
    let router = SignalRouter::default();
    let declarations = Parser::new("func double(x: Int) -> Int { x * 2 }").unwrap().parse_program().unwrap();
    let TopLevelDecl::Func(mut function) = declarations.declarations[0].clone() else { unreachable!() };
    function.name = "math.double".into();
    router.register_function(function);
    let program = Parser::new("routine main() { let f = math.double math.double(3) }").unwrap().parse_program().unwrap();
    let TopLevelDecl::Routine(main) = &program.declarations[0] else { unreachable!() };
    let Statement::VarDecl { value: reference, .. } = &main.body[0] else { unreachable!() };
    let Statement::ExprStmt(call) = &main.body[1] else { unreachable!() };
    assert_eq!(router.evaluate_expression_result(reference, &HashMap::new()), Ok(RuntimeValue::Function("math.double".into())));
    assert_eq!(router.evaluate_expression_result(call, &HashMap::new()), Ok(RuntimeValue::Int(6)));
    let scope = [("math".into(), RuntimeValue::Int(1))].into_iter().collect();
    for expr in [reference, call, &Expr::Variable("math.double".into())] {
        assert_eq!(router.evaluate_expression_result(expr, &scope), Err(EvaluationFailure::Invalid));
    }
}
