use proglang::parser::{Parser, ParseError};
use proglang::semantic::dag::CallGraphValidator;
use proglang::semantic::type_checker::TypeChecker;
use proglang::semantic::termination::TerminationChecker;
use proglang::semantic::SemanticError;

fn compile_and_validate(source: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = Parser::new(source)?;
    let program = parser.parse_program()?;

    let mut dag_validator = CallGraphValidator::new();
    dag_validator.build_and_validate(&program)?;

    let mut type_checker = TypeChecker::new();
    type_checker.check_program(&program)?;

    TerminationChecker::verify_program(&program)?;

    Ok(())
}

#[test]
fn test_program_00_hello_world() {
    let code = include_str!("../programs/00_hello_world.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_01_structs_and_functions() {
    let code = include_str!("../programs/01_structs_and_functions.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_02_bounded_loops() {
    let code = include_str!("../programs/02_bounded_loops.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_03_structural_recursion() {
    let code = include_str!("../programs/03_structural_recursion.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_04_actors_concurrency_spawn() {
    let code = include_str!("../programs/04_actors_concurrency_spawn.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_05_arenas_and_local_refs() {
    let code = include_str!("../programs/05_arenas_and_local_refs.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_06_full_pipeline() {
    let code = include_str!("../programs/06_full_pipeline.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_invalid_01_dag_cycle() {
    let code = include_str!("../programs/invalid_01_dag_cycle.proglang");
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let program = parser.parse_program().expect("Syntax should be valid");

    let mut dag_validator = CallGraphValidator::new();
    let err = dag_validator.build_and_validate(&program);
    assert!(matches!(err, Err(SemanticError::CallGraphCycle(_))), "Expected CallGraphCycle, got {:?}", err);
}

#[test]
fn test_invalid_02_infinite_recursion() {
    let code = include_str!("../programs/invalid_02_infinite_recursion.proglang");
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let program = parser.parse_program().expect("Syntax should be valid");

    let err = TerminationChecker::verify_program(&program);
    assert!(matches!(err, Err(SemanticError::NonDecreasingRecursion(_))), "Expected NonDecreasingRecursion, got {:?}", err);
}

#[test]
fn test_invalid_03_spawn_with_ref() {
    let code = include_str!("../programs/invalid_03_spawn_with_ref.proglang");
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let err = parser.parse_program();
    assert!(
        matches!(err, Err(ParseError::SyntaxError { .. })),
        "Expected SyntaxError for Ref in routine parameters, got {:?}",
        err
    );
}

#[test]
fn test_invalid_04_while_loop_prohibited() {
    let code = include_str!("../programs/invalid_04_while_loop_prohibited.proglang");
    let res = Parser::new(code).and_then(|mut p| p.parse_program());
    assert!(matches!(res, Err(ParseError::SyntaxError { .. })), "Expected SyntaxError for forbidden while loop, got {:?}", res);
}
