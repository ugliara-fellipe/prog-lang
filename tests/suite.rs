use proglang::parser::{Parser, ParseError};
use proglang::semantic::dag::CallGraphValidator;
use proglang::semantic::type_checker::TypeChecker;
use proglang::semantic::termination::TerminationChecker;
use proglang::semantic::SemanticError;
use std::process::Command;

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
fn test_program_07_packages_and_imports() {
    let code = include_str!("../programs/07_packages_and_imports.proglang");
    let mut parser = Parser::new(code).expect("Parser should initialize");
    let raw_program = parser.parse_program().expect("Parse program");

    let mut pkg_mgr = proglang::semantic::packages::PackageManager::new("programs");
    let program = pkg_mgr.resolve_program(raw_program).expect("Resolve package imports");

    let mut dag_validator = CallGraphValidator::new();
    assert!(dag_validator.build_and_validate(&program).is_ok());

    let mut type_checker = TypeChecker::new();
    assert!(type_checker.check_program(&program).is_ok());

    assert!(TerminationChecker::verify_program(&program).is_ok());
}

#[test]
fn test_program_08_persistent_loop() {
    let code = include_str!("../programs/08_persistent_loop.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_09_loop_signals() {
    let code = include_str!("../programs/09_loop_signals.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_10_exceptions() {
    let code = include_str!("../programs/10_exceptions.proglang");
    assert!(compile_and_validate(code).is_ok());
}

#[test]
fn test_program_11_search_and_sort_match_literature_worst_case_complexities() {
    let code = include_str!("../programs/11_search_and_sort.proglang");
    assert!(compile_and_validate(code).is_ok());

    let mut parser = Parser::new(code).unwrap();
    let program = parser.parse_program().unwrap();
    let entries = proglang::semantic::complexity::ComplexityAnalyzer::new().analyze(&program);
    let complexities: std::collections::HashMap<_, _> = entries
        .into_iter()
        .map(|entry| (entry.name, entry.complexity.to_big_o()))
        .collect();

    // Linear search and maximum scan each element once; bubble and selection
    // sort compare element pairs. These are their standard worst-case bounds.
    assert_eq!(complexities.get("linear_search"), Some(&"O(N)".to_string()));
    assert_eq!(complexities.get("max_value"), Some(&"O(N)".to_string()));
    assert_eq!(complexities.get("bubble_sort"), Some(&"O(N²)".to_string()));
    assert_eq!(complexities.get("selection_sort"), Some(&"O(N²)".to_string()));
}

#[test]
fn test_program_11_executes_search_and_sort_algorithms() {
    let output = Command::new(env!("CARGO_BIN_EXE_proglang"))
        .arg("programs/11_search_and_sort.proglang")
        .output()
        .expect("run algorithm fixture");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("valid UTF-8 output");
    assert!(stdout.contains("linear search for 7: true"));
    assert!(stdout.contains("linear search for 8: false"));
    assert!(stdout.contains("maximum: 9"));
    assert!(stdout.contains("bubble sort: [1, 2, 7, 9]"));
    assert!(stdout.contains("selection sort: [1, 2, 7, 9]"));
}

#[test]
fn test_program_12_main_entry() {
    let code = include_str!("../programs/12_main_entry.proglang");
    assert!(compile_and_validate(code).is_ok());
}

/// The real executable must dispatch only `main`, wait for its spawned work,
/// and then exit without running unrelated routine declarations.
#[test]
fn test_main_is_the_only_automatic_runtime_entry_point() {
    let output = Command::new(env!("CARGO_BIN_EXE_proglang"))
        .arg("programs/12_main_entry.proglang")
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
    let output = Command::new(env!("CARGO_BIN_EXE_proglang"))
        .arg("programs/03_structural_recursion.proglang")
        .output()
        .expect("run library fixture");
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().trim().is_empty());
}

#[test]
fn test_complexity_cli_reports_requested_declaration() {
    let output = Command::new(env!("CARGO_BIN_EXE_proglang"))
        .args(["complexity", "programs/11_search_and_sort.proglang", "func", "bubble_sort"])
        .output()
        .expect("run complexity CLI");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "func bubble_sort: O(N²)");
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

    let mut analyzer = proglang::semantic::complexity::ComplexityAnalyzer::new();
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
    let code = include_str!("../programs/invalid_04_while_loop_prohibited.proglang");
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
    for item in queue {
        log(item)
    }
    terminate
}

"#;
    assert!(compile_and_validate(code).is_ok());

    let mut parser = Parser::new(code).unwrap();
    let program = parser.parse_program().unwrap();
    let entries = proglang::semantic::complexity::ComplexityAnalyzer::new().analyze(&program);
    assert!(entries.iter().any(|e| e.name == "Poller" && e.kind == "loop" && e.complexity.to_big_o() == "O(N)"));
}

#[test]
fn test_loop_iteration_termination_is_proven_independently_of_requeueing() {
    // No `terminate`: the runtime requeues this loop forever, but every
    // individual execution remains finite and is therefore valid.
    let valid_persistent_loop = r#"
loop Daemon() {
    log(receive.value)
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
    assert!(matches!(err.downcast_ref::<SemanticError>(), Some(SemanticError::NonDecreasingRecursion(_))));
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
    let router = proglang::runtime::actor::SignalRouter::default();
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
    let router = proglang::runtime::actor::SignalRouter::default();
    assert!(router.register_loop("Inbox", Vec::new()));
    assert!(!router.register_loop("Inbox", Vec::new()));
    assert!(router.start_loop("Inbox"));
    assert!(!router.start_loop("Inbox"));
    router.stop_loop("Inbox");
    assert!(router.start_loop("Inbox"));
}

#[test]
fn test_spawn_starts_a_dormant_loop_then_becomes_a_signal() {
    let router = proglang::runtime::actor::SignalRouter::default();
    assert!(router.register_loop("Inbox", Vec::new()));

    let first = router.spawn_or_signal("Inbox", Some("first".to_string()));
    assert!(matches!(first, proglang::runtime::actor::LoopSpawn::Started(_)));
    let second = router.spawn_or_signal("Inbox", Some("second".to_string()));
    assert!(matches!(second, proglang::runtime::actor::LoopSpawn::Signalled));

    assert_eq!(router.receive_next("Inbox").value.as_deref(), Some("first"));
    assert_eq!(router.receive_next("Inbox").value.as_deref(), Some("second"));
    router.stop_loop("Inbox");
    assert!(matches!(router.spawn_or_signal("Inbox", None), proglang::runtime::actor::LoopSpawn::Started(_)));
    let empty_start = router.receive_next("Inbox");
    assert_eq!(empty_start.value, None);
    assert_eq!(empty_start.pending, 0);
}

#[test]
fn test_each_loop_iteration_gets_next_signal_or_none_when_queue_is_empty() {
    let router = proglang::runtime::actor::SignalRouter::default();
    assert!(router.register_loop("Inbox", Vec::new()));
    assert!(matches!(router.spawn_or_signal("Inbox", None), proglang::runtime::actor::LoopSpawn::Started(_)));

    // First scheduled iteration has no parameter/signal.
    assert_eq!(router.receive_next("Inbox").value, None);

    // Signals that arrive before the next iteration are delivered FIFO.
    assert!(matches!(router.spawn_or_signal("Inbox", Some("one".to_string())), proglang::runtime::actor::LoopSpawn::Signalled));
    assert!(matches!(router.spawn_or_signal("Inbox", Some("two".to_string())), proglang::runtime::actor::LoopSpawn::Signalled));
    assert_eq!(router.receive_next("Inbox").value.as_deref(), Some("one"));
    assert_eq!(router.receive_next("Inbox").value.as_deref(), Some("two"));

    // The scheduler still runs a new iteration after the queue is empty.
    assert_eq!(router.receive_next("Inbox").value, None);
}

#[test]
fn test_receive_is_reserved_for_loop_iterations() {
    let code = r#"
routine Invalid() {
    log(receive.value)
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
