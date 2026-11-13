use std::process::Command;
use lia::parser::Parser;
use lia::semantic::{complexity::{Complexity, ComplexityAnalyzer}, dag::CallGraphValidator, packages::PackageManager, termination::TerminationChecker, type_checker::TypeChecker};

#[test]
fn complete_program_runs_from_source_and_artifact() {
    let expected = "copy 10 15\nreturned 12 9 4\nlist 6\nlist 9\npattern 8\nworker 18\n";
    let output = Command::new(env!("CARGO_BIN_EXE_lia")).args(["run", "programs/15_higher_order.lia"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    let artifact = std::env::temp_dir().join(format!("lia-higher-order-{}.liac", std::process::id()));
    let compile = Command::new(env!("CARGO_BIN_EXE_lia")).args(["compile", "programs/15_higher_order.lia", "-o"]).arg(&artifact).output().unwrap();
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let output = Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(&artifact).output().unwrap();
    std::fs::remove_file(artifact).unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

#[test]
fn selective_import_preserves_returned_function_dependencies() {
    let dir = std::env::temp_dir().join(format!("lia-hof-package-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy("programs/std.lia", dir.join("std.lia")).unwrap();
    std::fs::write(dir.join("ops.lia"), "func hidden(x: Int) -> Int { x * 2 } func choose() -> (Int) -> Int { hidden }").unwrap();
    let source = "import ops.choose import std func hidden(x: Int) -> Int { x * 3 } routine main() { let f = ops.choose() std.log(f(7)) }";
    std::fs::write(dir.join("main.lia"), source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lia")).arg("run").arg(dir.join("main.lia")).output().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "14\n");
}

#[test]
fn indirect_call_cost_includes_callback_work() {
    let source = "func apply(f: (List[Int]) -> Int, xs: List[Int]) -> Int { f(xs) } func quadratic(xs: List[Int]) -> Int { for x in xs { for y in xs { y } } 1 } routine main() { apply(quadratic, [1, 2]) }";
    let raw = Parser::new(source).unwrap().parse_program().unwrap();
    let program = PackageManager::new("programs").resolve_program(raw).unwrap();
    TypeChecker::new().check_program(&program).unwrap();
    CallGraphValidator::new().build_and_validate(&program).unwrap();
    TerminationChecker::verify_program(&program).unwrap();
    let entries = ComplexityAnalyzer::new().try_analyze(&program).unwrap();
    for name in ["apply", "main"] {
        assert_eq!(entries.iter().find(|entry| entry.name == name).unwrap().complexity, Complexity::Polynomial(2));
    }
}

#[test]
fn complexity_rejects_unresolved_indirect_calls_and_indirect_cycles() {
    for source in [
        "func bad(x: Int) -> Int { let f = bad f(x) }",
        "func bad() -> Int { let f = missing f(1) }",
    ] {
        let program = Parser::new(source).unwrap().parse_program().unwrap();
        assert!(ComplexityAnalyzer::new().try_analyze(&program).is_err());
    }
}
