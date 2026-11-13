use lia::parser::{Parser, ParseError};
use lia::semantic::dag::CallGraphValidator;
use lia::semantic::type_checker::TypeChecker;
use lia::semantic::termination::TerminationChecker;
use lia::semantic::SemanticError;
use std::process::Command;

fn compile_and_validate(source: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = Parser::new(source)?;
    let raw_program = parser.parse_program()?;
    let mut packages = lia::semantic::packages::PackageManager::new("programs");
    let program = packages.resolve_program(raw_program)?;

    let mut dag_validator = CallGraphValidator::new();
    dag_validator.build_and_validate(&program)?;

    let mut type_checker = TypeChecker::new();
    type_checker.check_program(&program)?;

    TerminationChecker::verify_program(&program)?;

    Ok(())
}

#[test]
fn test_function_types_and_named_values_are_supported() {
    let source = "func apply(f: (Int) -> Int, x: Int) -> Int { f(x) }";
    let mut parser = Parser::new(source).unwrap();
    let program = parser.parse_program().unwrap();
    TypeChecker::new().check_program(&program).unwrap();

    let indirect = "func double(x: Int) -> Int { x * 2 } routine main() { let f = double f(1) }";
    let program = Parser::new(indirect).unwrap().parse_program().unwrap();
    assert_eq!(TypeChecker::new().check_program(&program), Ok(()));
}

#[test]
fn test_infer_type_of_returned_function_and_its_call() {
    use lia::ast::{Expr, TypeSpec};
    let source = "func choose() -> (Int) -> Int { double } func double(x: Int) -> Int { x * 2 }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    let mut checker = TypeChecker::new();
    assert_eq!(checker.check_program(&program), Ok(()));
    let target = Expr::FunctionCall { name: "choose".into(), args: vec![] };
    assert_eq!(checker.infer_type(&target), TypeSpec::Function(vec![TypeSpec::Primitive("Int".into())], Box::new(TypeSpec::Primitive("Int".into()))));
    assert_eq!(checker.infer_type(&Expr::IndirectCall { target: Box::new(target), args: vec![Expr::IntLiteral(1)] }), TypeSpec::Primitive("Int".into()));
}

#[test]
fn test_function_signature_checks_argument_count_types_and_return_type() {
    use lia::ast::TypeSpec;
    let int = TypeSpec::Primitive("Int".into());
    let string = TypeSpec::Primitive("String".into());
    let callback = TypeSpec::Function(vec![int.clone()], Box::new(string.clone()));
    let target = TypeSpec::Function(vec![callback.clone()], Box::new(int.clone()));
    assert_eq!(TypeChecker::check_function_signature(&target, &[callback.clone()]), Ok(int.clone()));
    assert!(matches!(TypeChecker::check_function_signature(&target, &[]), Err(SemanticError::ArgumentCountMismatch { .. })));
    let wrong_callback = TypeSpec::Function(vec![string], Box::new(int));
    assert!(matches!(TypeChecker::check_function_signature(&target, &[wrong_callback]), Err(SemanticError::FunctionArgumentTypeMismatch { index: 1, .. })));
    let unknown = TypeSpec::Primitive("Unknown".into());
    assert!(matches!(TypeChecker::check_function_signature(&callback, &[unknown]), Err(SemanticError::FunctionArgumentTypeMismatch { .. })));
}

#[test]
fn direct_calls_in_initializers_validate_known_argument_types_and_arity() {
    for source in [
        "func identity(x: Int) -> Int { x } func run() { let value = identity(\"wrong\") }",
        "func identity(x: Int) -> Int { x } func run() { let value = identity() }",
    ] {
        let program = Parser::new(source).unwrap().parse_program().unwrap();
        assert!(matches!(TypeChecker::new().check_program(&program), Err(SemanticError::FunctionArgumentTypeMismatch { .. } | SemanticError::ArgumentCountMismatch { .. })));
    }
}

#[test]
fn test_function_value_calls_check_argument_types() {
    let source = "func apply(f: (Int) -> Int) -> Int { f(\"wrong\") }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert!(matches!(TypeChecker::new().check_program(&program), Err(SemanticError::FunctionArgumentTypeMismatch { index: 1, .. })));
    let source = "func choose() -> (Int) -> Int { wrong } func wrong(x: String) -> Int { 1 }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert!(matches!(TypeChecker::new().check_program(&program), Err(SemanticError::FunctionReturnTypeMismatch { .. })));
}

#[test]
fn function_value_arguments_infer_expression_types_and_reject_non_callable() {
    let valid = "func apply(f: (Int) -> Int) -> Int { f(1 + 2) }";
    let program = Parser::new(valid).unwrap().parse_program().unwrap();
    assert_eq!(TypeChecker::new().check_program(&program), Ok(()));
    let invalid = "func run() { let x = 1 x(2) }";
    let program = Parser::new(invalid).unwrap().parse_program().unwrap();
    assert!(matches!(TypeChecker::new().check_program(&program), Err(SemanticError::NotAFunction(_))));
}

#[test]
fn checker_does_not_leak_symbols_between_declarations_or_runs() {
    let first = Parser::new("func one(f: Int) { 1 }").unwrap().parse_program().unwrap();
    let second = Parser::new("func f() -> Int { 1 } func two() -> Int { f() }").unwrap().parse_program().unwrap();
    let mut checker = TypeChecker::new();
    checker.check_program(&first).unwrap();
    checker.check_program(&second).unwrap();
    let combined = Parser::new("func one(f: Int) { 1 } func f() -> Int { 1 } func two() -> Int { f() }").unwrap().parse_program().unwrap();
    checker.check_program(&combined).unwrap();
}

#[test]
fn runtime_matches_nested_list_patterns() {
    use lia::ast::TopLevelDecl;
    use lia::runtime::actor::{RuntimeValue, SignalRouter};
    let source = "func only_one(xs: List[Int]) -> Int { match xs { Cons(x, Nil) => x, _ => 0 } }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    let router = SignalRouter::default();
    for declaration in program.declarations {
        if let TopLevelDecl::Func(function) = declaration { router.register_function(function); }
    }
    assert_eq!(router.evaluate_function("only_one", vec![RuntimeValue::List(vec![RuntimeValue::Int(7)])]), Some(RuntimeValue::Int(7)));
    assert_eq!(router.evaluate_function("only_one", vec![RuntimeValue::List(vec![RuntimeValue::Int(7), RuntimeValue::Int(8)])]), Some(RuntimeValue::Int(0)));
}

#[test]
fn runtime_evaluates_comparisons_and_reports_integer_overflow() {
    use lia::ast::Expr;
    use lia::runtime::actor::{RuntimeValue, SignalRouter};
    let router = SignalRouter::default();
    let binary = |op: &str, a, b| Expr::BinaryOp { op: op.into(), left: Box::new(Expr::IntLiteral(a)), right: Box::new(Expr::IntLiteral(b)) };
    assert_eq!(router.evaluate_expression(&binary("<", 1, 2)), Some(RuntimeValue::Bool(true)));
    assert_eq!(router.evaluate_expression(&binary("==", 1, 2)), Some(RuntimeValue::Bool(false)));
    assert_eq!(router.evaluate_expression(&binary("+", i64::MAX, 1)), None);
    assert_eq!(router.evaluate_expression(&binary("/", i64::MIN, -1)), None);
}

#[test]
fn runtime_catches_explicit_throw_but_not_evaluation_failure() {
    use lia::ast::TopLevelDecl;
    use lia::runtime::actor::{RuntimeValue, SignalRouter};
    let source = "func raise() -> Int { throw 9 } func nested() -> Int { try { 1 + raise() } catch error { error } } func caught() -> Int { try { throw 7 } catch error { error } } func invalid() -> Int { try { 1 / 0 } catch error { 7 } }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    let router = SignalRouter::default();
    for decl in program.declarations { if let TopLevelDecl::Func(f) = decl { router.register_function(f); } }
    assert_eq!(router.evaluate_function("caught", vec![]), Some(RuntimeValue::Int(7)));
    assert_eq!(router.evaluate_function("nested", vec![]), Some(RuntimeValue::Int(9)));
    assert_eq!(router.evaluate_function("invalid", vec![]), None);
}

#[test]
fn routine_executes_each_loop_item_and_selects_matching_case() {
    let dir = std::env::temp_dir().join(format!("lia-routine-blocks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy("programs/std.lia", dir.join("std.lia")).unwrap();
    let source = r#"import std
        routine main() {
            for x in [1, 2] { std.log(x) }
            match True { False => std.log("wrong"), True => std.log("right") }
        }
    "#;
    let file = dir.join("main.lia");
    std::fs::write(&file, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&file).output().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().collect::<Vec<_>>(), vec!["1", "2", "right"]);
}

#[test]
fn test_higher_order_global_call_and_branch_return_types_are_checked() {
    let source = r#"
        func wrong(x: String) -> Int { 1 }
        func apply(f: (Int) -> Int) -> Int { f(1) }
        func caller() -> Int { apply(wrong) }
    "#;
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    // Check the caller separately so the provisional higher-order rejection
    // of `apply` does not prevent checking its callers.
    let mut declarations = program.declarations.clone();
    declarations.rotate_left(2);
    let reordered = lia::ast::Program { imports: vec![], declarations };
    assert!(matches!(TypeChecker::new().check_program(&reordered), Err(SemanticError::FunctionArgumentTypeMismatch { index: 1, .. })));

    let source = r#"
        func good(x: Int) -> Int { x }
        func wrong(x: String) -> Int { 1 }
        func choose(flag: Int) -> (Int) -> Int { match flag { 0 => good, _ => wrong } }
    "#;
    // Lia patterns use identifiers rather than integer literals.
    let source = source.replace("0 => good", "zero => good");
    let program = Parser::new(&source).unwrap().parse_program().unwrap();
    assert!(matches!(TypeChecker::new().check_program(&program), Err(SemanticError::FunctionReturnTypeMismatch { .. })));

    let source = r#"
        func good(x: Int) -> Int { x }
        func choose(flag: Int) -> (Int) -> Int { match flag { zero => good, _ => good } }
    "#;
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert_eq!(TypeChecker::new().check_program(&program), Ok(()));

    let source = "func choose() -> (Int) -> Int { let x = 1 }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert_eq!(TypeChecker::new().check_program(&program), Err(SemanticError::MissingFunctionReturn));
}

#[test]
fn function_return_pattern_binding_uses_the_matched_type() {
    let source = "func choose(f: (Int) -> Int) -> (Int) -> Int { match f { selected => selected } }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert_eq!(TypeChecker::new().check_program(&program), Ok(()));
}

#[test]
fn function_return_destructures_list_element_types() {
    let source = "func identity(x: Int) -> Int { x } func choose(fs: List[(Int) -> Int]) -> (Int) -> Int { match fs { Cons(f, tail) => f, _ => identity } }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert_eq!(TypeChecker::new().check_program(&program), Ok(()));
}

#[test]
fn function_values_inside_returned_lists_are_checked() {
    let source = "func choose() -> List[(Int) -> Int] { [wrong] } func wrong(x: String) -> Int { 1 }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert!(matches!(TypeChecker::new().check_program(&program), Err(SemanticError::FunctionReturnTypeMismatch { .. })));
    let source = "func choose() -> List[(Int) -> Int] { let x = 1 }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert_eq!(TypeChecker::new().check_program(&program), Err(SemanticError::MissingFunctionReturn));
}

#[test]
fn function_return_branch_locals_do_not_leak() {
    let source = r#"
        func good(x: Int) -> Int { x }
        func choose(flag: Bool) -> (Int) -> Int {
            match flag { True => { let f = good f }, _ => f }
        }
    "#;
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert_eq!(TypeChecker::new().check_program(&program), Err(SemanticError::UndeclaredSymbol("f".into())));
}

#[test]
fn calls_inside_list_literals_participate_in_cycle_detection() {
    let source = "func first() { [second()] } func second() { first() }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert!(matches!(CallGraphValidator::new().build_and_validate(&program), Err(SemanticError::CallGraphCycle(_))));
}

#[test]
fn graph_rejects_cycles_through_local_function_values() {
    for source in [
        "func recurse(x: Int) -> Int { let f = recurse f(x) }",
        "func choose() -> (Int) -> Int { recurse } func recurse(x: Int) -> Int { let f = choose() f(x) }",
        "func apply(f: (Int) -> Int, x: Int) -> Int { f(x) } func recurse(x: Int) -> Int { apply(recurse, x) }",
        "func recurse(x: Int) -> Int { match recurse { callback => callback(x) } }",
        "func apply(fs: List[(Int) -> Int], x: Int) -> Int { for f in fs { f(x) } x } func recurse(x: Int) -> Int { apply([recurse], x) }",
    ] {
        let program = Parser::new(source).unwrap().parse_program().unwrap();
        assert!(matches!(CallGraphValidator::new().build_and_validate(&program), Err(SemanticError::CallGraphCycle(_))), "{source}");
    }
}

#[test]
fn graph_does_not_call_unused_function_parameters() {
    let source = "func choose(f: (Int) -> Int, x: Int) -> Int { x } func recurse(x: Int) -> Int { choose(recurse, x) }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert!(CallGraphValidator::new().build_and_validate(&program).is_ok());
}

#[test]
fn termination_checks_calls_in_match_targets_and_lists() {
    for source in ["func f(x: Int) { match f(x) { _ => 1 } }", "func f(x: Int) { [f(x)] }", "func f() { f() }"] {
        let program = Parser::new(source).unwrap().parse_program().unwrap();
        assert!(matches!(TerminationChecker::verify_program(&program), Err(SemanticError::NonDecreasingRecursion(_))));
    }
}

#[test]
fn matching_a_whole_value_does_not_prove_recursive_descent() {
    let source = "func f(xs: List[Int]) -> Int { match xs { same => f(same) } }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert!(matches!(TerminationChecker::verify_program(&program), Err(SemanticError::NonDecreasingRecursion(_))));
}

#[test]
fn package_resolution_rejects_circular_imports() {
    let dir = std::env::temp_dir().join(format!("lia-cycle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.lia"), "import b").unwrap();
    std::fs::write(dir.join("b.lia"), "import a").unwrap();
    let program = Parser::new("import a").unwrap().parse_program().unwrap();
    let result = lia::semantic::packages::PackageManager::new(&dir).resolve_program(program);
    std::fs::remove_dir_all(dir).unwrap();
    assert!(matches!(result, Err(SemanticError::PackageError(message)) if message.contains("Circular")));
}

#[test]
fn complexity_includes_later_callees_in_nested_expressions() {
    use lia::semantic::complexity::{Complexity, ComplexityAnalyzer};
    let source = "func caller(xs: List[Int]) { [helper(xs)] } func helper(xs: List[Int]) { for x in xs { x } }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    let entries = ComplexityAnalyzer::new().analyze(&program);
    assert_eq!(entries.iter().find(|entry| entry.name == "caller").unwrap().complexity, Complexity::Polynomial(1));
}

#[test]
fn test_indirect_calls_cannot_hide_loop_only_expressions() {
    let code = "func maker() { 1 } routine main() { maker()(message.value) }";
    let program = Parser::new(code).unwrap().parse_program().unwrap();
    assert_eq!(TypeChecker::new().check_program(&program), Err(SemanticError::ReceiveOutsideLoop));
    let code = "func maker() { 1 } routine main() { maker()(memory key) }";
    let program = Parser::new(code).unwrap().parse_program().unwrap();
    assert_eq!(TypeChecker::new().check_program(&program), Err(SemanticError::MemoryOutsideLoop));
}

#[test]
fn test_runtime_can_copy_and_invoke_named_function_values() {
    use lia::ast::TopLevelDecl;
    use lia::runtime::actor::{RuntimeValue, SignalRouter};

    let source = r#"
        func double(x: Int) -> Int { x * 2 }
        func apply(f: (Int) -> Int, x: Int) -> Int { f(x) }
        func choose() -> (Int) -> Int { double }
        func invoke() -> Int { choose()(6) }
    "#;
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    let router = SignalRouter::default();
    for declaration in program.declarations {
        if let TopLevelDecl::Func(function) = declaration {
            router.register_function(function);
        }
    }
    let function = router.evaluate_function("choose", vec![]).unwrap();
    assert_eq!(function, RuntimeValue::Function("double".into()));
    assert_eq!(router.evaluate_function("apply", vec![function.clone(), RuntimeValue::Int(5)]), Some(RuntimeValue::Int(10)));
    assert_eq!(router.evaluate_function("apply", vec![function, RuntimeValue::Int(7)]), Some(RuntimeValue::Int(14)));
    assert_eq!(router.evaluate_function("apply", vec![RuntimeValue::Int(1), RuntimeValue::Int(5)]), None);
    assert_eq!(router.evaluate_function("invoke", vec![]), Some(RuntimeValue::Int(12)));
}

#[test]
fn test_spawned_routine_receives_copied_function_value() {
    use lia::ast::{Param, TypeSpec};
    use lia::runtime::actor::{RuntimeValue, SignalRouter};

    let router = SignalRouter::default();
    assert!(router.register_routine_with_params("Worker", vec![Param {
        name: "action".into(),
        param_type: TypeSpec::Function(vec![TypeSpec::Primitive("Int".into())], Box::new(TypeSpec::Primitive("Int".into()))),
    }], vec![]));
    let value = RuntimeValue::Function("double".into());
    let message = router.spawn_routine_with_args("Worker", String::new(), vec![value.clone()]).unwrap();
    assert_eq!(message.bindings.get("action"), Some(&value));
}

#[test]
fn spawn_checks_routine_arity_and_known_argument_types() {
    for source in [
        "routine worker(x: Int) {} routine main() { spawn worker() }",
        "routine worker(x: Int) {} routine main() { spawn worker(1, 2) }",
    ] {
        let program = Parser::new(source).unwrap().parse_program().unwrap();
        assert!(matches!(TypeChecker::new().check_program(&program), Err(SemanticError::ArgumentCountMismatch { .. })));
    }
    let source = "routine worker(x: Int) {} routine main() { spawn worker(\"wrong\") }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert!(matches!(TypeChecker::new().check_program(&program), Err(SemanticError::FunctionArgumentTypeMismatch { .. })));
    let source = "routine main() { spawn worker(wrong) } routine worker(f: (Int) -> Int) {} func wrong(x: String) -> Int { 1 }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert!(matches!(TypeChecker::new().check_program(&program), Err(SemanticError::FunctionArgumentTypeMismatch { .. })));
}

#[test]
fn runtime_rejects_routine_arguments_with_wrong_arity() {
    use lia::ast::{Param, TypeSpec};
    use lia::runtime::actor::{RuntimeValue, SignalRouter};
    let router = SignalRouter::default();
    router.register_routine_with_params("worker", vec![Param { name: "x".into(), param_type: TypeSpec::Primitive("Int".into()) }], vec![]);
    assert!(router.spawn_routine_with_args("worker", String::new(), vec![]).is_none());
    assert!(router.spawn_routine_with_args("worker", String::new(), vec![RuntimeValue::Int(1), RuntimeValue::Int(2)]).is_none());
    assert!(router.spawn_routine_with_args("worker", String::new(), vec![RuntimeValue::Int(1)]).is_some());
}

#[test]
fn test_call_graph_conservatively_detects_cycles_through_function_parameters() {
    let cyclic = r#"
        func apply(f: (Int) -> Int, x: Int) -> Int { f(x) }
        func bad(x: Int) -> Int { apply(bad, x) }
    "#;
    let program = Parser::new(cyclic).unwrap().parse_program().unwrap();
    assert!(matches!(CallGraphValidator::new().build_and_validate(&program), Err(SemanticError::CallGraphCycle(_))));

    let valid = r#"
        func apply(f: (Int) -> Int, x: Int) -> Int { f(x) }
        func double(x: Int) -> Int { x * 2 }
    "#;
    let program = Parser::new(valid).unwrap().parse_program().unwrap();
    assert!(CallGraphValidator::new().build_and_validate(&program).is_ok());

    let returned = r#"
        func choose() -> (Int) -> Int { bad }
        func bad(x: Int) -> Int { choose()(x) }
    "#;
    let program = Parser::new(returned).unwrap().parse_program().unwrap();
    assert!(matches!(CallGraphValidator::new().build_and_validate(&program), Err(SemanticError::CallGraphCycle(_))));

    let safe_returned = r#"
        func double(x: Int) -> Int { x * 2 }
        func choose() -> (Int) -> Int { double }
        func invoke() -> Int { choose()(5) }
    "#;
    let program = Parser::new(safe_returned).unwrap().parse_program().unwrap();
    assert!(CallGraphValidator::new().build_and_validate(&program).is_ok());
}

#[test]
fn test_termination_checks_calls_nested_in_indirect_call_targets_and_arguments() {
    let source = "func recur(x: Int) -> Int { recur(x)(1) }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert!(matches!(TerminationChecker::verify_program(&program), Err(SemanticError::NonDecreasingRecursion(_))));
    let source = "func recur(x: Int) -> Int { factory()(recur(x)) }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert!(matches!(TerminationChecker::verify_program(&program), Err(SemanticError::NonDecreasingRecursion(_))));
    let source = "func a() -> Int { chooser()(b()) } func b() -> Int { a() }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    assert!(matches!(CallGraphValidator::new().build_and_validate(&program), Err(SemanticError::UndeclaredSymbol(_))));
}

#[test]
fn test_program_00_hello_world() {
    let code = include_str!("../programs/00_hello_world.lia");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_01_structs_and_functions() {
    let code = include_str!("../programs/01_structs_and_functions.lia");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_02_bounded_loops() {
    let code = include_str!("../programs/02_bounded_loops.lia");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_03_structural_recursion() {
    let code = include_str!("../programs/03_structural_recursion.lia");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_04_actors_concurrency_spawn() {
    let code = include_str!("../programs/04_actors_concurrency_spawn.lia");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_05_arenas_and_local_refs() {
    let code = include_str!("../programs/05_arenas_and_local_refs.lia");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_06_full_pipeline() {
    let code = include_str!("../programs/06_full_pipeline.lia");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_07_packages_and_imports() {
    let code = include_str!("../programs/07_packages_and_imports.lia");
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let raw_program = parser.parse_program().expect("Parse program");

    let mut pkg_mgr = lia::semantic::packages::PackageManager::new("programs");
    let program = pkg_mgr.resolve_program(raw_program).expect("Resolve package imports");

    let mut dag_validator = CallGraphValidator::new();
    assert!(dag_validator.build_and_validate(&program).is_ok());

    let mut type_checker = TypeChecker::new();
    assert!(type_checker.check_program(&program).is_ok());

    assert!(TerminationChecker::verify_program(&program).is_ok());
}

#[test]
fn test_program_08_persistent_loop() {
    let code = include_str!("../programs/08_persistent_loop.lia");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_09_loop_signals() {
    let code = include_str!("../programs/09_loop_signals.lia");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_10_exceptions() {
    let code = include_str!("../programs/10_exceptions.lia");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_11_search_and_sort_match_literature_worst_case_complexities() {
    let code = include_str!("../programs/11_search_and_sort.lia");
    let mut parser = Parser::new(code).unwrap();
    let raw_program = parser.parse_program().unwrap();
    let mut packages = lia::semantic::packages::PackageManager::new("programs");
    let program = packages.resolve_program(raw_program).unwrap();

    let mut dag_validator = CallGraphValidator::new();
    assert!(dag_validator.build_and_validate(&program).is_ok());
    let mut type_checker = TypeChecker::new();
    assert!(type_checker.check_program(&program).is_ok());
    assert!(TerminationChecker::verify_program(&program).is_ok());

    let entries = lia::semantic::complexity::ComplexityAnalyzer::new().analyze(&program);
    let complexities: std::collections::HashMap<_, _> = entries
        .into_iter()
        .map(|entry| (entry.name, entry.complexity.to_big_o()))
        .collect();

    // Linear search and maximum scan each element once. Insertion sort calls
    // an O(N) insertion once per element, yielding the standard O(N²) bound.
    assert_eq!(complexities.get("linear_search"), Some(&"O(N)".to_string()));
    assert_eq!(complexities.get("max_value"), Some(&"O(N)".to_string()));
    assert_eq!(complexities.get("insert_sorted"), Some(&"O(N)".to_string()));
    assert_eq!(complexities.get("insertion_sort"), Some(&"O(N²)".to_string()));
}

#[test]
fn test_program_11_executes_search_and_sort_algorithms() {
    let output = Command::new(env!("CARGO_BIN_EXE_lia"))
        .arg("programs/11_search_and_sort.lia")
        .output()
        .expect("run algorithm fixture");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("valid UTF-8 output");
    assert!(stdout.contains("linear search for 7: true"));
    assert!(stdout.contains("linear search for 8: false"));
    assert!(stdout.contains("maximum: 9"));
    assert!(stdout.contains("insertion sort: [1, 2, 7, 9]"));
}

#[test]
fn test_program_12_main_entry() {
    let code = include_str!("../programs/12_main_entry.lia");
    assert!(compile_and_validate(code).is_ok());
}

/// Every non-negative fixture is either executable (`main`) or a library.
/// The compiler must accept both forms after resolving their explicit imports.
#[test]
fn test_all_valid_programs_compile_as_programs_or_libraries() {
    let fixtures = [
        "00_hello_world.lia",
        "01_structs_and_functions.lia",
        "02_bounded_loops.lia",
        "03_structural_recursion.lia",
        "04_actors_concurrency_spawn.lia",
        "05_arenas_and_local_refs.lia",
        "06_full_pipeline.lia",
        "07_packages_and_imports.lia",
        "08_persistent_loop.lia",
        "09_loop_signals.lia",
        "10_exceptions.lia",
        "11_search_and_sort.lia",
        "12_main_entry.lia",
        "13_library_consumer.lia",
        "math.lia",
        "std.lia",
    ];

    for fixture in fixtures {
        let output = Command::new(env!("CARGO_BIN_EXE_lia"))
            .arg(format!("programs/{fixture}"))
            .output()
            .expect("run valid fixture");
        assert!(
            output.status.success(),
            "{fixture} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn test_executable_examples_dispatch_their_main_routines() {
    let fixtures = [
        ("00_hello_world.lia", "Hello, World!"),
        ("07_packages_and_imports.lia", "Sum via qualified import: 30"),
        ("08_persistent_loop.lia", "poller finished its finite iteration"),
        ("09_loop_signals.lia", "received: new work is available still pending: 0"),
        ("10_exceptions.lia", "starting exception monitor"),
        ("13_library_consumer.lia", "sum from imported library: 15"),
    ];

    for (fixture, expected_output) in fixtures {
        let output = Command::new(env!("CARGO_BIN_EXE_lia"))
            .arg(format!("programs/{fixture}"))
            .output()
            .expect("run executable fixture");
        assert!(output.status.success(), "{fixture} did not execute");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(expected_output),
            "{fixture} did not produce expected output '{expected_output}'"
        );
    }
}

#[test]
fn test_type_expression_reports_local_and_function_parameter_types() {
    let output = Command::new(env!("CARGO_BIN_EXE_lia"))
        .arg("programs/14_type_of.lia")
        .output()
        .expect("run type example");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Int String List[Int] Int"));
}

/// The real executable must dispatch only `main`, wait for its spawned work,
/// and then exit without running unrelated routine declarations.
#[test]
fn test_main_is_the_only_automatic_runtime_entry_point() {
    let output = Command::new(env!("CARGO_BIN_EXE_lia"))
        .arg("programs/12_main_entry.lia")
        .output()
        .expect("run main-entry fixture");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("valid UTF-8 output");
    assert!(stdout.contains("main started"));
    assert!(stdout.contains("worker finished"));
    assert!(!stdout.contains("this must not run"));
}

/// A valid source without `routine main()` is a library: direct execution
/// validates it and exits without dispatching declarations as actor tasks.
#[test]
fn test_library_without_main_does_not_dispatch_runtime_work() {
    let output = Command::new(env!("CARGO_BIN_EXE_lia"))
        .arg("programs/03_structural_recursion.lia")
        .output()
        .expect("run library fixture");
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().trim().is_empty());
}

#[test]
fn test_complexity_cli_reports_requested_declaration() {
    let output = Command::new(env!("CARGO_BIN_EXE_lia"))
        .args(["complexity", "programs/11_search_and_sort.lia", "func", "insertion_sort"])
        .output()
        .expect("run complexity CLI");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "func insertion_sort: O(N²)");
}

#[test]
fn test_runtime_backed_standard_library_functions_require_an_import() {
    let code = r#"
routine main() {
    log(std.empty())
}
"#;
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let program = parser.parse_program().expect("Parse program");
    let error = TypeChecker::new().check_program(&program).expect_err("std must be imported");
    assert_eq!(error, SemanticError::MissingStandardLibraryImport("std.empty".to_string()));
}

#[test]
fn test_std_package_contains_only_certified_terminating_primitives() {
    let mut parser = Parser::new(include_str!("../programs/std.lia"))
        .expect("Parser should initialize");
    let program = parser.parse_program().expect("Parse std package");
    assert!(lia::stdlib::validate_standard_library(&program).is_ok());

    let declared: Vec<_> = program
        .declarations
        .iter()
        .filter_map(|declaration| match declaration {
            lia::ast::TopLevelDecl::Func(function) => Some(format!("std.{}", function.name)),
            _ => None,
        })
        .collect();
    assert_eq!(declared.len(), lia::stdlib::TOTAL_STANDARD_FUNCTIONS.len());
    assert!(declared
        .iter()
        .all(|name| lia::stdlib::is_total_standard_function(name)));
}

#[test]
fn test_invalid_01_dag_cycle() {
    let code = include_str!("../programs/invalid_01_dag_cycle.lia");
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let program = parser.parse_program().expect("Syntax should be valid");

    let mut dag_validator = CallGraphValidator::new();
    let err = dag_validator.build_and_validate(&program);
    assert!(matches!(err, Err(SemanticError::CallGraphCycle(_))), "Expected CallGraphCycle, got {:?}", err);
}

#[test]
fn test_invalid_02_infinite_recursion() {
    let code = include_str!("../programs/invalid_02_infinite_recursion.lia");
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let program = parser.parse_program().expect("Syntax should be valid");

    let err = TerminationChecker::verify_program(&program);
    assert!(matches!(err, Err(SemanticError::NonDecreasingRecursion(_))), "Expected NonDecreasingRecursion, got {:?}", err);
}

#[test]
fn test_invalid_03_spawn_with_ref() {
    let code = include_str!("../programs/invalid_03_spawn_with_ref.lia");
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let err = parser.parse_program();
    assert!(
        matches!(err, Err(ParseError::SyntaxError { .. })),
        "Expected SyntaxError for Ref in routine parameters, got {:?}",
        err
    );
}

#[test]
fn test_complexity_analysis() {
    let code = r#"
func constant_fn(x: Int) -> Int {
    let a = x + 1
    a * 2
}

func recursive_fn(l: List[Int]) -> Int {
    match l {
        Nil => 0,
        Cons(head, tail) => head + recursive_fn(tail)
    }
}

routine RoutineO1(x: Int) {
    let y = constant_fn(x)
    log(y)
}

routine RoutineON(l: List[Int]) {
    for x in l {
        let y = constant_fn(x)
        log(y)
    }
}

routine RoutineON2(matrix: List[List[Int]]) {
    for row in matrix {
        for col in row {
            log(col)
        }
    }
}

routine RoutineCallsRecursive(l: List[Int]) {
    let total = recursive_fn(l)
    log(total)
}
"#;
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let program = parser.parse_program().expect("Parse program");

    let mut analyzer = lia::semantic::complexity::ComplexityAnalyzer::new();
    let entries = analyzer.analyze(&program);

    let map: std::collections::HashMap<String, String> = entries
        .into_iter()
        .map(|e| (e.name, e.complexity.to_big_o()))
        .collect();

    assert_eq!(map.get("constant_fn").unwrap(), "O(1)");
    assert_eq!(map.get("recursive_fn").unwrap(), "O(N)");
    assert_eq!(map.get("RoutineO1").unwrap(), "O(1)");
    assert_eq!(map.get("RoutineON").unwrap(), "O(N)");
    assert_eq!(map.get("RoutineON2").unwrap(), "O(N²)");
    assert_eq!(map.get("RoutineCallsRecursive").unwrap(), "O(N)");
}

#[test]
fn test_routines_can_interact_and_recurse() {
    let code = r#"
routine RoutineSelf(x: Int) {
    spawn RoutineSelf(x)
}

routine RoutineA(x: Int) {
    spawn RoutineB(x)
}

routine RoutineB(x: Int) {
    spawn RoutineA(x)
}
"#;
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let program = parser.parse_program().expect("Syntax should be valid");
    let mut dag_validator = CallGraphValidator::new();
    assert!(dag_validator.build_and_validate(&program).is_ok());
    assert!(TerminationChecker::verify_program(&program).is_ok());
}

#[test]
fn test_invalid_04_while_loop_prohibited() {
    let code = include_str!("../programs/invalid_04_while_loop_prohibited.lia");
    let res = Parser::new(code).and_then(|mut p| p.parse_program());
    assert!(matches!(res, Err(ParseError::SyntaxError { .. })), "Expected SyntaxError for forbidden while loop, got {:?}", res);
}

// ---------------------------------------------------------------
// Routine termination safety (Item 1)
// ---------------------------------------------------------------

/// Routines that only use spawn for self/mutual invocation must pass
/// all checks – each routine body itself terminates.
#[test]
fn test_routine_spawn_self_is_safe() {
    let code = r#"
routine Pinger(x: Int) {
    log(x)
    spawn Pinger(x)
}
"#;
    assert!(compile_and_validate(code).is_ok());
}

/// Routines calling functions that form a valid DAG must pass.
#[test]
fn test_routine_calls_functions_dag_ok() {
    let code = r#"
func helper(x: Int) -> Int {
    x + 1
}

routine Worker(x: Int) {
    let y = helper(x)
    log(y)
}
"#;
    assert!(compile_and_validate(code).is_ok());
}

/// Routine body that calls a structurally-recursive function is safe
/// as long as the function itself is proven terminating.
#[test]
fn test_routine_calls_recursive_func_ok() {
    let code = r#"
func sum_list(l: List[Int]) -> Int {
    match l {
        Nil => 0,
        Cons(head, tail) => head + sum_list(tail)
    }
}

routine Aggregator(data: List[Int]) {
    let total = sum_list(data)
    log(total)
}
"#;
    assert!(compile_and_validate(code).is_ok());
}

/// A routine with a bounded for-loop calling a safe function is OK.
#[test]
fn test_routine_for_loop_with_functions() {
    let code = r#"
func double(x: Int) -> Int {
    x + x
}

routine BatchProcessor(items: List[Int]) {
    for item in items {
        let d = double(item)
        log(d)
    }
}
"#;
    assert!(compile_and_validate(code).is_ok());
}

/// Routines that contain functions with DAG cycles must be rejected.
#[test]
fn test_routine_with_cyclic_function_calls_rejected() {
    let code = r#"
func alpha(x: Int) -> Int {
    beta(x)
}

func beta(x: Int) -> Int {
    alpha(x)
}

routine Runner(x: Int) {
    let r = alpha(x)
    log(r)
}
"#;
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let program = parser.parse_program().expect("Syntax should be valid");
    let mut dag = CallGraphValidator::new();
    assert!(
        dag.build_and_validate(&program).is_err(),
        "Expected CallGraphCycle for functions called from routine"
    );
}

/// Routine with match + spawn + function calls – full integration.
#[test]
fn test_routine_match_spawn_functions_integration() {
    let code = r#"
type Task {
    id: Int,
    payload: String,
    subtasks: List[Task]
}

func sum_tail(l: List[Task]) -> Int {
    match l {
        Nil => 0,
        Cons(head, tail) => 1 + sum_tail(tail)
    }
}

func count_subtasks(t: Task) -> Int {
    match t.subtasks {
        Nil => 1,
        Cons(head, tail) => 1 + count_subtasks(head)
    }
}

routine TaskExecutor(task: Task) {
    let count = count_subtasks(task)
    log(count)

    match task.subtasks {
        Nil => {
            log("leaf")
        },
        Cons(head, tail) => {
            spawn TaskExecutor(head)
            for sub in tail {
                spawn TaskExecutor(sub)
            }
        }
    }
}
"#;
    assert!(compile_and_validate(code).is_ok());
}

// Persistent loops (Item 2)
#[test]
fn test_loop_parses_validates_and_has_per_iteration_complexity() {
    let code = r#"
loop Poller() {
    let queue = [1, 2]
    for item in queue {
        log(item)
    }
    terminate
}

"#;
    assert!(compile_and_validate(code).is_ok());

    let mut parser = Parser::new(code).unwrap();
    let program = parser.parse_program().unwrap();
    let entries = lia::semantic::complexity::ComplexityAnalyzer::new().analyze(&program);
    assert!(entries.iter().any(|e| e.name == "Poller" && e.kind == "loop" && e.complexity.to_big_o() == "O(N)"));
}

#[test]
fn test_loop_iteration_termination_is_proven_independently_of_requeueing() {
    // No `terminate`: the runtime requeues this loop forever, but every
    // individual execution remains finite and is therefore valid.
    let valid_persistent_loop = r#"
loop Daemon() {
    log(message.value)
}
"#;
    assert!(compile_and_validate(valid_persistent_loop).is_ok());

    // A synchronous self-call would prevent one isolated iteration from
    // finishing, so it is rejected even though loops themselves persist.
    let non_terminating_iteration = r#"
loop Bad() {
    Bad()
}
"#;
    let err = compile_and_validate(non_terminating_iteration).unwrap_err();
    assert!(matches!(err.downcast_ref::<SemanticError>(), Some(SemanticError::InvalidExpression(message)) if message.contains("spawn")));
}

#[test]
fn test_terminate_is_reserved_and_rejected_outside_a_loop() {
    let code = r#"
routine Invalid() {
    terminate
}
"#;
    let err = compile_and_validate(code).unwrap_err();
    assert!(err.downcast_ref::<SemanticError>() == Some(&SemanticError::TerminateOutsideLoop));
}

// Asynchronous loop signals (Item 3)
#[test]
fn test_signal_is_allowed_from_functions_routines_and_loops() {
    let code = r#"
func NotifyFromFunction(value: String) {
    signal Inbox(value)
}

routine NotifyFromRoutine(value: String) {
    signal Inbox(value)
}

loop Inbox() {
    signal Inbox("next iteration")
    terminate
}
"#;
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_loop_has_no_parameters_and_spawn_value_becomes_a_signal() {
    let invalid_parameter = r#"
loop Inbox(value: String) {}
"#;
    let parse_error = Parser::new(invalid_parameter).and_then(|mut parser| parser.parse_program());
    assert!(matches!(parse_error, Err(ParseError::SyntaxError { .. })));

    let spawn_signal = r#"
routine Producer() {
    spawn Inbox("payload")
}
loop Inbox() {}
"#;
    assert!(compile_and_validate(spawn_signal).is_ok());
}

#[test]
fn test_signal_router_queues_without_waiting_and_drains_at_iteration_start() {
    let router = lia::runtime::actor::SignalRouter::default();
    assert!(router.register_loop("Inbox", Vec::new()));

    assert!(router.emit("Inbox", "first".to_string()));
    assert!(router.emit("Inbox", "second".to_string()));
    let first = router.receive_next("Inbox");
    assert_eq!(first.value.as_deref(), Some("first"));
    assert_eq!(first.pending, 1);
    let second = router.receive_next("Inbox");
    assert_eq!(second.value.as_deref(), Some("second"));
    assert_eq!(second.pending, 0);
    assert_eq!(router.receive_next("Inbox").value, None);
    assert!(!router.emit("UnknownLoop", "ignored".to_string()));
}

#[test]
fn test_loop_names_are_unique_and_signals_have_a_specific_loop_target() {
    let duplicate = r#"
loop Inbox() {}
loop Inbox() {}
"#;
    let err = compile_and_validate(duplicate).unwrap_err();
    assert!(err.downcast_ref::<SemanticError>() == Some(&SemanticError::DuplicateLoopName("Inbox".to_string())));

    let unknown_target = r#"
routine Sender() {
    signal Missing("message")
}
"#;
    let err = compile_and_validate(unknown_target).unwrap_err();
    assert!(err.downcast_ref::<SemanticError>() == Some(&SemanticError::UnknownLoopSignal("Missing".to_string())));
}

#[test]
fn test_signal_router_allows_only_one_active_instance_per_loop_name() {
    let router = lia::runtime::actor::SignalRouter::default();
    assert!(router.register_loop("Inbox", Vec::new()));
    assert!(!router.register_loop("Inbox", Vec::new()));
    assert!(router.start_loop("Inbox"));
    assert!(!router.start_loop("Inbox"));
    router.stop_loop("Inbox");
    assert!(router.start_loop("Inbox"));
}

#[test]
fn test_spawn_starts_a_dormant_loop_then_becomes_a_signal() {
    let router = lia::runtime::actor::SignalRouter::default();
    assert!(router.register_loop("Inbox", Vec::new()));

    let first = router.spawn_or_signal("Inbox", Some("first".to_string()));
    assert!(matches!(first, lia::runtime::actor::LoopSpawn::Started(_)));
    let second = router.spawn_or_signal("Inbox", Some("second".to_string()));
    assert!(matches!(second, lia::runtime::actor::LoopSpawn::Signalled));

    assert_eq!(router.receive_next("Inbox").value.as_deref(), Some("first"));
    assert_eq!(router.receive_next("Inbox").value.as_deref(), Some("second"));
    router.stop_loop("Inbox");
    assert!(matches!(router.spawn_or_signal("Inbox", None), lia::runtime::actor::LoopSpawn::Started(_)));
    let empty_start = router.receive_next("Inbox");
    assert_eq!(empty_start.value, None);
    assert_eq!(empty_start.pending, 0);
}

#[test]
fn test_each_loop_iteration_gets_next_signal_or_none_when_queue_is_empty() {
    let router = lia::runtime::actor::SignalRouter::default();
    assert!(router.register_loop("Inbox", Vec::new()));
    assert!(matches!(router.spawn_or_signal("Inbox", None), lia::runtime::actor::LoopSpawn::Started(_)));

    // First scheduled iteration has no parameter/signal.
    assert_eq!(router.receive_next("Inbox").value, None);

    // Signals that arrive before the next iteration are delivered FIFO.
    assert!(matches!(router.spawn_or_signal("Inbox", Some("one".to_string())), lia::runtime::actor::LoopSpawn::Signalled));
    assert!(matches!(router.spawn_or_signal("Inbox", Some("two".to_string())), lia::runtime::actor::LoopSpawn::Signalled));
    assert_eq!(router.receive_next("Inbox").value.as_deref(), Some("one"));
    assert_eq!(router.receive_next("Inbox").value.as_deref(), Some("two"));

    // The scheduler still runs a new iteration after the queue is empty.
    assert_eq!(router.receive_next("Inbox").value, None);
}

#[test]
fn test_receive_is_reserved_for_loop_iterations() {
    let code = r#"
routine Invalid() {
    log(message.value)
}

"#;
    let err = compile_and_validate(code).unwrap_err();
    assert!(err.downcast_ref::<SemanticError>() == Some(&SemanticError::ReceiveOutsideLoop));
}

#[test]
fn test_exceptions_are_emitted_only_by_functions_and_caught_everywhere() {
    let valid = r#"
func Source() {
    try {
        throw "failure"
    } catch error {
        log(error)
    }
}
routine Consumer() {
    try { Source() } catch error { log(error) }
}
loop LoopConsumer() {
    try { Source() } catch error { log(error) }
    terminate
}
"#;
    assert!(compile_and_validate(valid).is_ok());

    let invalid_routine = r#"
routine Invalid() { throw "failure" }
"#;
    let err = compile_and_validate(invalid_routine).unwrap_err();
    assert!(err.downcast_ref::<SemanticError>() == Some(&SemanticError::ThrowOutsideFunction));

    let invalid_loop = r#"
loop Invalid() { throw "failure" }
"#;
    let err = compile_and_validate(invalid_loop).unwrap_err();
    assert!(err.downcast_ref::<SemanticError>() == Some(&SemanticError::ThrowOutsideFunction));
}
