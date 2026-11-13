use lia::ast::{Expr, Program, Statement, TopLevelDecl};
use lia::parser::Parser;
use lia::semantic::{dag::CallGraphValidator, flow::FunctionFlow, termination::TerminationChecker, SemanticError};
use std::collections::HashSet;

fn parse(source: &str) -> Program { Parser::new(source).unwrap().parse_program().unwrap() }
fn named(items: &[&str]) -> HashSet<String> { items.iter().map(|s| (*s).into()).collect() }
fn last_call<'a>(program: &'a Program, owner: &str) -> &'a Expr {
    let f=program.declarations.iter().find_map(|d| match d { TopLevelDecl::Func(f) if f.name == owner => Some(f), _ => None }).unwrap();
    match f.body.last().unwrap() { Statement::ExprStmt(expr) => expr, _ => panic!("expected call") }
}

#[test]
fn targets_propagate_across_arguments_returns_and_local_copies() {
    let p=parse("func double(x: Int) -> Int { x * 2 } func apply(f: (Int) -> Int, x: Int) -> Int { f(x) } func choose() -> (Int) -> Int { double } func main() -> Int { let f = choose() let copy = f apply(copy, 3) }");
    let flow=FunctionFlow::analyze(&p).unwrap();
    assert_eq!(flow.targets_for(last_call(&p,"apply")),named(&["double"]));
    assert_eq!(flow.targets_for(last_call(&p,"main")),named(&["apply"]));
    assert!(CallGraphValidator::new().build_and_validate(&p).is_ok());
}

#[test]
fn equally_typed_unrelated_functions_do_not_create_false_cycles() {
    let p=parse("func leaf(x: Int) -> Int { x } func caller(x: Int) -> Int { let f = leaf f(x) }");
    let flow=FunctionFlow::analyze(&p).unwrap();
    assert_eq!(flow.targets_for(last_call(&p,"caller")),named(&["leaf"]));
    CallGraphValidator::new().build_and_validate(&p).unwrap();
}

#[test]
fn branch_return_values_are_joined() {
    let p=parse("func left(x: Int) -> Int { x } func right(x: Int) -> Int { x + 1 } func choose(flag: Bool) -> (Int) -> Int { match flag { True => left, False => right } } func main() -> Int { choose(True)(2) }");
    let flow=FunctionFlow::analyze(&p).unwrap();
    assert_eq!(flow.targets_for(last_call(&p,"main")),named(&["left","right"]));
}

#[test]
fn nested_list_values_and_patterns_preserve_targets() {
    let p=parse("func leaf(x: Int) -> Int { x } func use_list(fs: List[(Int) -> Int]) -> Int { match fs { Cons(f, tail) => f(1), Nil => 0 } } func main() -> Int { use_list([leaf]) }");
    let flow=FunctionFlow::analyze(&p).unwrap();
    assert!(flow.calls_by_owner()["use_list"].contains("leaf"));
}

#[test]
fn spawn_propagates_callback_arguments_without_synchronous_actor_edges() {
    let p=parse("func leaf(x: Int) -> Int { x } routine worker(f: (Int) -> Int) { f(3) spawn worker(f) } routine main() { spawn worker(leaf) }");
    let flow=FunctionFlow::analyze(&p).unwrap();
    assert_eq!(flow.calls_by_owner()["worker"],named(&["leaf"]));
    CallGraphValidator::new().build_and_validate(&p).unwrap();
}

#[test]
fn indirect_cycles_are_rejected_by_both_entry_points() {
    for source in [
        "func recursive(x: Int) -> Int { let f = recursive f(x) }",
        "func choose() -> (Int) -> Int { recursive } func recursive(x: Int) -> Int { choose()(x) }",
        "func apply(f: (Int) -> Int, x: Int) -> Int { f(x) } func recursive(x: Int) -> Int { apply(recursive,x) }",
    ] {
        let p=parse(source);
        assert!(matches!(CallGraphValidator::new().build_and_validate(&p),Err(SemanticError::CallGraphCycle(_))));
        assert!(matches!(TerminationChecker::verify_program(&p),Err(SemanticError::CallGraphCycle(_))));
    }
}

#[test]
fn unknown_indirect_destinations_are_rejected() {
    let p=parse("func call() -> Int { let f = missing f(1) }");
    assert!(matches!(FunctionFlow::analyze(&p),Err(SemanticError::UndeclaredSymbol(_))));
}

#[test]
fn qualified_named_function_values_are_resolved() {
    let mut p=parse("func double(x: Int) -> Int { x * 2 } func main() -> Int { let f = math.double f(2) }");
    if let TopLevelDecl::Func(f)=&mut p.declarations[0] { f.name="math.double".into(); }
    let flow=FunctionFlow::analyze(&p).unwrap();
    assert_eq!(flow.targets_for(last_call(&p,"main")),named(&["math.double"]));
}

#[test]
fn local_receivers_shadow_package_qualifiers() {
    let mut p=parse("func double(x: Int) -> Int { x * 2 } func main() -> Int { let math = 1 let f = math.double f(2) }");
    if let TopLevelDecl::Func(f)=&mut p.declarations[0] { f.name="math.double".into(); }
    assert!(matches!(FunctionFlow::analyze(&p),Err(SemanticError::UndeclaredSymbol(_))));
}

#[test]
fn structural_descent_must_come_from_the_inductive_argument() {
    for source in [
        "func recurse(xs: List[Int], ys: List[Int]) -> Int { match ys { Cons(head, tail) => recurse(tail,ys), Nil => 0 } }",
        "func recurse(xs: List[Int]) -> Int { let xs = [1] match xs { Cons(head, tail) => recurse(tail), Nil => 0 } }",
        "func recurse(xs: List[Int]) -> Int { match xs { Cons(head, tail) => { let tail = xs recurse(tail) }, Nil => 0 } }",
    ] {
        let p=parse(source);
        assert!(matches!(TerminationChecker::verify_program(&p),Err(SemanticError::NonDecreasingRecursion(_))),"{source}");
    }
}

#[test]
fn structural_aliases_preserve_valid_descent() {
    let p=parse("func recurse(xs: List[Int]) -> Int { let original = xs match original { Cons(head, tail) => { let smaller = tail recurse(smaller) }, Nil => 0 } }");
    TerminationChecker::verify_program(&p).unwrap();
}

#[test]
fn function_values_thrown_through_calls_reach_catch_returns() {
    let p=parse("func leaf(x: Int) -> Int { x } func raise(f: (Int) -> Int) { throw f } func choose(f: (Int) -> Int) -> (Int) -> Int { try { raise(f) } catch error { error } } func main() -> Int { choose(leaf)(2) }");
    let flow=FunctionFlow::analyze(&p).unwrap();
    assert_eq!(flow.targets_for(last_call(&p,"main")),named(&["leaf"]));
}

#[test]
fn catch_bindings_shadow_values_and_try_locals_do_not_leak() {
    let p=parse("func left(x: Int) -> Int { x } func right(x: Int) -> Int { x + 1 } func choose() -> (Int) -> Int { let f = right try { throw left } catch f { f } } func main() -> Int { choose()(2) }");
    let flow=FunctionFlow::analyze(&p).unwrap();
    assert_eq!(flow.targets_for(last_call(&p,"main")),named(&["left"]));
    let p=parse("func leaf(x: Int) -> Int { x } func choose() -> (Int) -> Int { try { let secret = leaf throw leaf } catch error { secret } } func main() -> Int { choose()(2) }");
    assert!(matches!(FunctionFlow::analyze(&p),Err(SemanticError::UndeclaredSymbol(_))));
}

#[test]
fn ordinary_prepend_function_uses_its_actual_return_values() {
    let p=parse("func leaf(x: Int) -> Int { x } func prepend(f: (Int) -> Int, fs: List[(Int) -> Int]) -> List[(Int) -> Int] { [recursive] } func use_list(fs: List[(Int) -> Int], x: Int) -> Int { match fs { Cons(f, tail) => f(x), Nil => 0 } } func recursive(x: Int) -> Int { use_list(prepend(leaf, []), x) }");
    assert!(matches!(CallGraphValidator::new().build_and_validate(&p),Err(SemanticError::CallGraphCycle(_))));
}

#[test]
fn certified_prepend_merges_the_head_and_tail_destinations() {
    let mut p=parse("func leaf(x: Int) -> Int { x } func other(x: Int) -> Int { x + 1 } func prepend(f: (Int) -> Int, fs: List[(Int) -> Int]) -> List[(Int) -> Int] {} func use_list(fs: List[(Int) -> Int]) -> Int { match fs { Cons(f, tail) => f(1), Nil => 0 } } func main() -> Int { use_list(std.prepend(leaf,[other])) }");
    if let TopLevelDecl::Func(f)=&mut p.declarations[2] { f.name="std.prepend".into(); }
    let flow=FunctionFlow::analyze(&p).unwrap();
    assert_eq!(flow.calls_by_owner()["use_list"],named(&["leaf","other"]));
}

#[test]
fn local_receivers_also_shadow_flattened_qualified_calls() {
    let mut p=parse("func double(x: Int) -> Int { x * 2 } func main() -> Int { let math = 1 math.double(2) }");
    if let TopLevelDecl::Func(f)=&mut p.declarations[0] { f.name="math.double".into(); }
    assert!(matches!(FunctionFlow::analyze(&p),Err(SemanticError::UndeclaredSymbol(_))));
}
