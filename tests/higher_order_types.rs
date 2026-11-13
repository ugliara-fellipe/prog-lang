use lia::parser::Parser;
use lia::semantic::{SemanticError, type_checker::TypeChecker};

fn check(source: &str) -> Result<(), SemanticError> {
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    TypeChecker::new().check_program(&program)
}

#[test]
fn pass_return_copy_and_invoke_function_values() {
    check(r#"
        func twice(x: Int) -> Int { x * 2 }
        func apply(f: (Int) -> Int, x: Int) -> Int { f(x) }
        func choose() -> (Int) -> Int { twice }
        routine Worker(f: (Int) -> Int) { f(2) }
        routine main() {
            let original: (Int) -> Int = twice
            let copy = original
            let result = apply(copy, choose()(4))
            spawn Worker(copy)
        }
    "#).unwrap();
}

#[test]
fn branches_and_nested_pattern_types_are_preserved() {
    check(r#"
        func identity(x: Int) -> Int { x }
        func choose(flag: Bool) -> (Int) -> Int {
            match flag { True => identity, False => identity }
        }
        func first(fs: List[(Int) -> Int]) -> (Int) -> Int {
            match fs { Nil => identity, Cons(f, rest) => f }
        }
        func recover() -> (Int) -> Int {
            try { identity } catch error { identity }
        }
        routine Worker(fs: List[(Int) -> Int]) { for f in fs { f(2) } }
    "#).unwrap();
}

#[test]
fn signatures_are_checked_in_every_expression_position() {
    for source in [
        r#"func id(x: Int) -> Int { x } routine main() { let x = id("bad") }"#,
        r#"func id(x: Int) -> Int { x } func f() -> Int { id() + 2 }"#,
        r#"func id(x: Int) -> Int { x } func maker() -> (Int) -> Int { id } routine main() { maker()("bad") }"#,
        r#"func id(x: Int) -> Int { x } routine main() { let f = id f() }"#,
        r#"func wrong(x: String) -> Int { 1 } func use(f: (Int) -> Int) -> Int { f(2) } routine main() { let x = use(wrong) }"#,
        r#"routine Worker(f: (Int) -> Int) {} func wrong(x: String) -> Int { 1 } routine main() { spawn Worker(wrong) }"#,
    ] {
        assert!(matches!(check(source), Err(SemanticError::FunctionArgumentTypeMismatch { .. } | SemanticError::ArgumentCountMismatch { .. })), "{source}");
    }
}

#[test]
fn return_type_and_missing_paths_are_rejected() {
    for source in [
        r#"func id(x: Int) -> Int { x } func choose() -> (String) -> Int { id }"#,
        r#"func id(x: Int) -> Int { x } func choose(flag: Bool) -> (Int) -> Int { match flag { True => id } }"#,
        r#"func id(x: Int) -> Int { x } func choose() -> (Int) -> Int { try { id } catch error { let x = 1 } }"#,
        r#"func choose() -> Int { let x = 1 }"#,
        r#"func choose() -> Int { "bad" }"#,
    ] {
        assert!(matches!(check(source), Err(SemanticError::FunctionReturnTypeMismatch { .. } | SemanticError::MissingFunctionReturn)), "{source}");
    }
}

#[test]
fn scoped_bindings_do_not_escape_match_try_or_loop() {
    for source in [
        r#"func id(x: Int) -> Int { x } func use(f: (Int) -> Int) -> Int { f(2) } routine main() { match True { _ => { let local = id } } use(local) }"#,
        r#"func id(x: Int) -> Int { x } func use(f: (Int) -> Int) -> Int { f(2) } routine main() { try { let local = id } catch error {} use(local) }"#,
        r#"func id(x: Int) -> Int { x } func use(f: (Int) -> Int) -> Int { f(2) } routine main() { for local in [id] {} use(local) }"#,
    ] {
        assert!(matches!(check(source), Err(SemanticError::FunctionArgumentTypeMismatch { .. } | SemanticError::UndeclaredSymbol(_))), "{source}");
    }
}

#[test]
fn references_nested_in_named_types_cannot_cross_spawn() {
    let source = r#"type Shared { pointer: Ref[Int] } routine Worker(x: Shared) {} routine Producer(x: Shared) { spawn Worker(x) }"#;
    assert!(matches!(check(source), Err(SemanticError::UncopyableSpawnArgument(_))));
    let source = r#"type Shared { pointer: Ref[Int] } loop Inbox() { terminate } routine Producer(values: List[Shared]) { for x in values { spawn Inbox(x) } }"#;
    assert!(matches!(check(source), Err(SemanticError::UncopyableSpawnArgument(_))));
}

#[test]
fn actors_require_asynchronous_spawn() {
    assert!(matches!(check("routine Worker() {} routine main() { Worker() }"), Err(SemanticError::InvalidExpression(_))));
    assert!(matches!(check("loop Worker() { terminate } routine main() { Worker() }"), Err(SemanticError::InvalidExpression(_))));
    assert!(matches!(check("func bad() { spawn Worker() } routine Worker() {}"), Err(SemanticError::SpawnInsideFunction)));
    assert!(matches!(check("func id(x: Int) -> Int { x } routine Worker() {} routine main() { let Worker = id spawn Worker() }"), Err(SemanticError::InvalidExpression(_))));
}

#[test]
fn invalid_pattern_arity_and_function_assignment_are_rejected() {
    assert!(matches!(check("routine main(xs: List[Int]) { match xs { Cons(x) => x } }"), Err(SemanticError::InvalidPattern(_))));
    assert!(matches!(check("func id(x: Int) -> Int { x } routine main() { let wrong: (String) -> Int = id }"), Err(SemanticError::FunctionReturnTypeMismatch { .. })));
}

#[test]
fn scalar_annotations_operations_and_fake_intrinsics_are_checked() {
    assert!(matches!(check("routine main() { let value: Int = \"bad\" }"), Err(SemanticError::FunctionReturnTypeMismatch { .. })));
    assert!(matches!(check("func id(x: Int) -> Int { x } func choose() -> (Int) -> Int { id + id }"), Err(SemanticError::InvalidExpression(_))));
    assert!(matches!(check("func max(x: Int, y: Int) -> Int {}"), Err(SemanticError::MissingFunctionReturn)));
    assert!(matches!(check("routine main() { for f in 1 {} }"), Err(SemanticError::InvalidExpression(_))));
}

#[test]
fn only_normal_paths_require_a_return_value() {
    check("func failed() -> (Int) -> Int { throw \"failure\" let ignored = 1 }").unwrap();
    assert!(matches!(check("func failed() -> Int { try { throw \"failure\" } catch error { let ignored = 1 } }"), Err(SemanticError::MissingFunctionReturn)));
}

#[test]
fn qualified_function_references_have_function_types() {
    let program = Parser::new("import std routine main() { let f: (Int, Int) -> Int = std.max f(1, 2) }").unwrap().parse_program().unwrap();
    let program = lia::semantic::packages::PackageManager::new("programs").resolve_program(program).unwrap();
    TypeChecker::new().check_program(&program).unwrap();
}

#[test]
fn lexical_bindings_shadow_qualified_package_names() {
    for source in [
        "import std routine main() { let std = 1 std.max(1, 2) }",
        "import std routine main() { let std = 1 let f: (Int, Int) -> Int = std.max }",
    ] {
        let program = Parser::new(source).unwrap().parse_program().unwrap();
        let program = lia::semantic::packages::PackageManager::new("programs").resolve_program(program).unwrap();
        assert!(TypeChecker::new().check_program(&program).is_err(), "{source}");
    }
}
