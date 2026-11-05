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
fn test_program_01_estruturas_e_funcoes() {
    let code = include_str!("../programs/01_estruturas_e_funcoes.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_02_lacos_delimitados() {
    let code = include_str!("../programs/02_lacos_delimitados.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_03_recursao_estrutural_inducao() {
    let code = include_str!("../programs/03_recursao_estrutural_inducao.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_04_atores_concorrencia_spawn() {
    let code = include_str!("../programs/04_atores_concorrencia_spawn.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_05_arenas_e_referencias_locais() {
    let code = include_str!("../programs/05_arenas_e_referencias_locais.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_06_pipeline_completo() {
    let code = include_str!("../programs/06_pipeline_completo.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_invalid_01_dag_ciclo() {
    let code = include_str!("../programs/invalid_01_dag_ciclo.proglang");
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let program = parser.parse_program().expect("Syntax should be valid");
    
    let mut dag_validator = CallGraphValidator::new();
    let err = dag_validator.build_and_validate(&program);
    assert!(matches!(err, Err(SemanticError::CallGraphCycle(_))), "Expected CallGraphCycle, got {:?}", err);
}

#[test]
fn test_invalid_02_recursao_infinita() {
    let code = include_str!("../programs/invalid_02_recursao_infinita.proglang");
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let program = parser.parse_program().expect("Syntax should be valid");
    
    let err = TerminationChecker::verify_program(&program);
    assert!(matches!(err, Err(SemanticError::NonDecreasingRecursion(_))), "Expected NonDecreasingRecursion, got {:?}", err);
}

#[test]
fn test_invalid_03_spawn_com_ref() {
    let code = include_str!("../programs/invalid_03_spawn_com_ref.proglang");
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let program = parser.parse_program().expect("Syntax should be valid");
    
    let mut type_checker = TypeChecker::new();
    let err = type_checker.check_program(&program);
    assert!(matches!(err, Err(SemanticError::UncopyableSpawnArgument(_))), "Expected UncopyableSpawnArgument, got {:?}", err);
}

#[test]
fn test_invalid_04_laco_while_proibido() {
    let code = include_str!("../programs/invalid_04_laco_while_proibido.proglang");
    let res = Parser::new(code).and_then(|mut p| p.parse_program());
    assert!(matches!(res, Err(ParseError::SyntaxError { .. })), "Expected SyntaxError for forbidden while loop, got {:?}", res);
}
