use lia::ast::{Expr, Statement, TopLevelDecl, TypeSpec};
use lia::parser::{ParseError, Parser};

#[test]
fn generic_declarations_record_bound_type_parameters() {
    let source = "type Box<T> { value: T } func map<T, U>(items: List[T], f: (T) -> U) -> List[U] { [] } routine Worker<T>(value: T) {}";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    let TopLevelDecl::Type(box_type) = &program.declarations[0] else { panic!("expected type") };
    assert_eq!(box_type.type_params, ["T"]);
    assert_eq!(box_type.fields[0].field_type, TypeSpec::Primitive("T".into()));
    let TopLevelDecl::Func(map) = &program.declarations[1] else { panic!("expected function") };
    assert_eq!(map.type_params, ["T", "U"]);
    assert_eq!(map.return_type, Some(TypeSpec::Generic("List".into(), vec![TypeSpec::Primitive("U".into())])));
    let TopLevelDecl::Routine(worker) = &program.declarations[2] else { panic!("expected routine") };
    assert_eq!(worker.type_params, ["T"]);
}

#[test]
fn explicit_specialization_supports_calls_values_and_qualified_names() {
    let source = "routine main() { let result = identity<Int>(1) let f = identity<String> let qualified = pkg.identity<List[Int]> }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    let TopLevelDecl::Routine(main) = &program.declarations[0] else { panic!("expected routine") };
    let Statement::VarDecl { value: Expr::IndirectCall { target, args }, .. } = &main.body[0] else { panic!("expected specialized call") };
    assert!(matches!(target.as_ref(), Expr::Specialize { target, type_args } if matches!(target.as_ref(), Expr::Variable(name) if name == "identity") && type_args == &[TypeSpec::Primitive("Int".into())]));
    assert_eq!(args, &[Expr::IntLiteral(1)]);
    assert!(matches!(&main.body[1], Statement::VarDecl { value: Expr::Specialize { type_args, .. }, .. } if type_args == &[TypeSpec::Primitive("String".into())]));
    assert!(matches!(&main.body[2], Statement::VarDecl { value: Expr::Specialize { target, .. }, .. } if matches!(target.as_ref(), Expr::MemberAccess { field, .. } if field == "identity")));
}

#[test]
fn explicit_function_types_and_relational_operators_do_not_conflict() {
    let source = "routine main(a: Int, b: Int, c: Int) { let first = a < b let chain = a < b > c let f = identity<(Int) -> Int> }";
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    let TopLevelDecl::Routine(main) = &program.declarations[0] else { panic!("expected routine") };
    assert!(matches!(&main.body[0], Statement::VarDecl { value: Expr::BinaryOp { op, .. }, .. } if op == "<"));
    assert!(matches!(&main.body[1], Statement::VarDecl { value: Expr::BinaryOp { op, .. }, .. } if op == ">"));
    assert!(matches!(&main.body[2], Statement::VarDecl { value: Expr::Specialize { type_args, .. }, .. } if matches!(type_args[0], TypeSpec::Function(_, _))));
}

#[test]
fn empty_and_duplicate_type_parameter_lists_are_rejected() {
    for source in ["func bad<T, T>(x: T) -> T { x }", "routine bad<T, T>() {}", "type Bad<T, T> {}", "func bad<>() {}"] {
        assert!(matches!(Parser::new(source).unwrap().parse_program(), Err(ParseError::SyntaxError { .. })), "{source}");
    }
}

#[test]
fn old_serialized_declarations_default_to_no_type_parameters() {
    let function: lia::ast::FuncDecl = serde_json::from_str(r#"{"name":"old","params":[],"return_type":null,"body":[]}"#).unwrap();
    let routine: lia::ast::RoutineDecl = serde_json::from_str(r#"{"name":"old","params":[],"body":[]}"#).unwrap();
    let record: lia::ast::TypeDecl = serde_json::from_str(r#"{"name":"Old","fields":[]}"#).unwrap();
    assert!(function.type_params.is_empty() && routine.type_params.is_empty() && record.type_params.is_empty());
}

#[test]
fn function_refs_are_allowed_but_routine_refs_remain_forbidden() {
    assert!(Parser::new("func read<T>(x: Ref[T]) -> T { x }").unwrap().parse_program().is_ok());
    assert!(Parser::new("routine read<T>(x: Ref[T]) {}").unwrap().parse_program().is_err());
}
