use std::env;
use std::fs;
use std::path::Path;
use tokio::sync::mpsc;
use lia::ast::*;
use lia::parser::Parser;
use lia::semantic::dag::CallGraphValidator;
use lia::semantic::type_checker::TypeChecker;
use lia::semantic::termination::TerminationChecker;
use lia::semantic::complexity::ComplexityAnalyzer;
use lia::semantic::packages::PackageManager;
use lia::runtime::actor::{ActorHandle, Message};
use lia::runtime::event_loop::RuntimeEventLoop;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw_args: Vec<String> = env::args().collect();
    let verbose = raw_args.iter().any(|a| a == "--verbose" || a == "-v");
    let args: Vec<String> = raw_args
        .into_iter()
        .filter(|a| a != "--verbose" && a != "-v")
        .collect();

    if verbose {
        println!("============================================================");
        println!(" Lia - Compiler and Runtime for Finite Routines ");
        println!("============================================================\n");
    }

    if args.len() > 1 {
        let cmd = &args[1];

        // 1. Test suite runner command
        if cmd == "--test-all" || cmd == "--check-all" {
            run_test_suite_cli().await?;
            return Ok(());
        }

        // 2. Command: compile <file.lia> [-o <file.liac>]
        if cmd == "compile" || cmd == "build" {
            if args.len() < 3 {
                eprintln!("Usage: lia compile <file.lia> [-o <file.liac>]");
                std::process::exit(1);
            }
            let input_path = &args[2];
            let output_path = if args.len() >= 5 && args[3] == "-o" {
                args[4].clone()
            } else {
                format!("{}.liac", input_path.trim_end_matches(".lia"))
            };
            compile_to_artifact(input_path, &output_path)?;
            return Ok(());
        }

        // 3. Command: complexity <file.lia> <name>
        //    Or: complexity <file.lia> <func|routine|loop> <name>
        if cmd == "complexity" {
            if args.len() < 4 || args.len() > 5 {
                eprintln!("Usage: lia complexity <file.lia> [func|routine|loop] <name>");
                std::process::exit(1);
            }
            let input_path = &args[2];
            let (kind, name) = if args.len() == 5 {
                (Some(args[3].as_str()), args[4].as_str())
            } else {
                (None, args[3].as_str())
            };
            query_complexity(input_path, kind, name)?;
            return Ok(());
        }

        // 4. Command: annotate <file.lia> [-o <output>]
        //    Parses, validates and injects worst-case complexity comments.
        if cmd == "annotate" {
            if args.len() < 3 {
                eprintln!("Usage: lia annotate <file.lia> [-o <output.lia>]");
                std::process::exit(1);
            }
            let input_path = &args[2];
            let output_path = if args.len() >= 5 && args[3] == "-o" {
                args[4].clone()
            } else {
                input_path.clone() // overwrite in-place by default
            };
            annotate_complexity(input_path, &output_path)?;
            return Ok(());
        }

        // 5. Command: run <file.liac | file.lia>
        if cmd == "run" {
            if args.len() < 3 {
                eprintln!("Usage: lia run <file.liac | file.lia>");
                std::process::exit(1);
            }
            let target_path = &args[2];
            if target_path.ends_with(".liac") {
                run_compiled_artifact(target_path, verbose).await?;
            } else {
                compile_and_run_file(target_path, verbose).await?;
            }
            return Ok(());
        }

        // 4. Direct file execution
        if cmd.ends_with(".liac") {
            run_compiled_artifact(cmd, verbose).await?;
        } else {
            compile_and_run_file(cmd, verbose).await?;
        }
        return Ok(());
    }

    // Default fallback if no file argument is given
    let default_file = if Path::new("hello_world.lia").exists() {
        "hello_world.lia"
    } else {
        "programs/06_pipeline_completo.lia"
    };

    compile_and_run_file(default_file, verbose).await?;
    Ok(())
}

/// Prints the worst-case complexity of one validated declaration.
fn query_complexity(
    input_path: &str,
    requested_kind: Option<&str>,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = fs::read_to_string(input_path)?;
    let mut parser = Parser::new(&source)?;
    let raw_program = parser.parse_program()?;
    let base_dir = Path::new(input_path).parent().unwrap_or_else(|| Path::new("."));
    let mut pkg_mgr = PackageManager::new(base_dir);
    let program = pkg_mgr.resolve_program(raw_program)?;

    CallGraphValidator::new().build_and_validate(&program)?;
    TypeChecker::new().check_program(&program)?;
    TerminationChecker::verify_program(&program)?;

    let kind = requested_kind.map(normalize_complexity_kind).transpose()?;
    let entries = ComplexityAnalyzer::new().try_analyze(&program)?;
    let matches: Vec<_> = entries
        .iter()
        .filter(|entry| entry.name == name && kind.map_or(true, |kind| entry.kind == kind))
        .collect();

    match matches.as_slice() {
        [entry] => println!("{} {}: {}", entry.kind, entry.name, entry.complexity.to_big_o()),
        [] => return Err(format!("No declaration named '{}'{} found", name, kind.map(|k| format!(" of kind '{}'", k)).unwrap_or_default()).into()),
        _ => return Err(format!("Multiple declarations named '{}'; specify func, routine, or loop", name).into()),
    }
    Ok(())
}

fn normalize_complexity_kind(kind: &str) -> Result<&'static str, Box<dyn std::error::Error>> {
    match kind {
        "func" | "function" | "função" => Ok("func"),
        "routine" | "rotina" => Ok("routine"),
        "loop" => Ok("loop"),
        _ => Err(format!("Unknown declaration kind '{}'; use func, routine, or loop", kind).into()),
    }
}

/// Compiles Lia source code, runs formal verifications, and writes .liac artifact
fn compile_to_artifact(input_path: &str, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔨 Compiling '{}' -> '{}'...", input_path, output_path);
    let source = fs::read_to_string(input_path)?;

    // 1. Parsing
    println!("[1/4] Parsing source code...");
    let mut parser = Parser::new(&source)?;
    let raw_program = parser.parse_program()?;

    // Resolve packages/imports
    let base_dir = Path::new(input_path).parent().unwrap_or_else(|| Path::new("."));
    let mut pkg_mgr = PackageManager::new(base_dir);
    let program = pkg_mgr.resolve_program(raw_program)?;
    println!("  -> AST constructed with {} declarations (including imports).", program.declarations.len());

    // 2. Semantic Validations
    println!("[2/4] Validating Call Graph (DAG)...");
    CallGraphValidator::new().build_and_validate(&program)?;
    println!("  -> Success: No illegal cycles detected in Call Graph.");

    println!("[3/4] Validating Types and Ref[T] Confinement...");
    TypeChecker::new().check_program(&program)?;
    println!("  -> Success: Only Copyable data is passed across spawn.");

    println!("[4/4] Proving Termination via Structural Induction...");
    TerminationChecker::verify_program(&program)?;
    println!("  -> Success: All recursive calls are strictly decreasing.");

    // Write compiled serialized program to disk
    let serialized = serde_json::to_vec_pretty(&program)?;
    fs::write(output_path, serialized)?;

    println!("\n✅ Compilation successful! Artifact saved to '{}'", output_path);
    println!("To execute the compiled artifact instantly:");
    println!("  ./lia run {}", output_path);
    Ok(())
}

/// Loads and executes a pre-compiled artifact (.liac)
async fn run_compiled_artifact(artifact_path: &str, verbose: bool) -> Result<(), Box<dyn std::error::Error>> {
    if verbose {
        println!("⚡ Loading pre-compiled artifact: {}", artifact_path);
    }
    let bytes = fs::read(artifact_path)?;
    let program: Program = serde_json::from_slice(&bytes)?;
    // Serialized AST is an input format, not a proof of validation. Rebuild
    // flow information and recheck it before executing any artifact.
    CallGraphValidator::new().build_and_validate(&program)?;
    TypeChecker::new().check_program(&program)?;
    TerminationChecker::verify_program(&program)?;
    if verbose {
        println!("  -> Loaded verified program ({} declarations).", program.declarations.len());
    }

    execute_runtime(&program, verbose).await?;
    if verbose {
        println!("\n✅ Execution completed successfully!");
    }
    Ok(())
}

/// Compiles source and executes
async fn compile_and_run_file(file_path: &str, verbose: bool) -> Result<(), Box<dyn std::error::Error>> {
    if verbose {
        println!("📂 Reading source file: {}", file_path);
    }
    let source = fs::read_to_string(file_path)?;

    // 1. Parsing
    if verbose {
        println!("[1/4] Parsing source code...");
    }
    let mut parser = Parser::new(&source)?;
    let raw_program = match parser.parse_program() {
        Ok(prog) => prog,
        Err(e) => {
            eprintln!("❌ Syntax Error: {}", e);
            return Err(Box::new(e));
        }
    };

    let base_dir = Path::new(file_path).parent().unwrap_or_else(|| Path::new("."));
    let mut pkg_mgr = PackageManager::new(base_dir);
    let program = match pkg_mgr.resolve_program(raw_program) {
        Ok(prog) => {
            if verbose {
                println!("  -> Success: AST constructed with {} declarations (including imports).", prog.declarations.len());
            }
            prog
        }
        Err(e) => {
            eprintln!("❌ Package / Import Error: {}", e);
            return Err(Box::new(e));
        }
    };

    // 2. Semantic Validations
    if verbose {
        println!("[2/4] Validating Call Graph (DAG)...");
    }
    let mut dag_validator = CallGraphValidator::new();
    match dag_validator.build_and_validate(&program) {
        Ok(_) => {
            if verbose {
                println!("  -> Success: No illegal cycles detected in Call Graph.");
            }
        }
        Err(e) => {
            eprintln!("❌ Semantic Error (DAG Cycle): {}", e);
            return Err(Box::new(e));
        }
    }

    if verbose {
        println!("[3/4] Validating Types and Ref[T] Confinement...");
    }
    let mut type_checker = TypeChecker::new();
    match type_checker.check_program(&program) {
        Ok(_) => {
            if verbose {
                println!("  -> Success: Only Copyable data is passed across spawn.");
            }
        }
        Err(e) => {
            eprintln!("❌ Semantic Error (Type/Spawn): {}", e);
            return Err(Box::new(e));
        }
    }

    if verbose {
        println!("[4/4] Proving Termination via Structural Induction...");
    }
    match TerminationChecker::verify_program(&program) {
        Ok(_) => {
            if verbose {
                println!("  -> Success: All recursive calls are strictly decreasing.");
            }
        }
        Err(e) => {
            eprintln!("❌ Semantic Error (Termination): {}", e);
            return Err(Box::new(e));
        }
    }

    // 3. Runtime Execution
    execute_runtime(&program, verbose).await?;

    if verbose {
        println!("\n✅ Compilation and verification completed successfully!");
    }
    Ok(())
}

async fn execute_runtime(program: &Program, verbose: bool) -> Result<(), Box<dyn std::error::Error>> {
    // Declarations are library members by default. Only a routine literally
    // named `main` creates a runnable program entry point.
    let main_routine = program.declarations.iter().find_map(|decl| match decl {
        TopLevelDecl::Routine(routine) if routine.name == "main" => Some(routine),
        _ => None,
    });

    if let Some(main_routine) = main_routine {
        if !main_routine.params.is_empty() {
            return Err("Entry routine 'main' must not declare parameters".into());
        }
        if verbose {
            println!("\n------------------------------------------------------------");
            println!(" Starting Lia Runtime (Tokio EventLoop & Arenas)");
            println!("------------------------------------------------------------");
        }

        let (tx, rx) = mpsc::channel(32);
        let mut event_loop = RuntimeEventLoop::with_verbose(rx, verbose);
        let signal_router = event_loop.signal_router();
        signal_router.register_enums(program);
        let shutdown_handle = event_loop.shutdown_handle();
        let actor_handle = ActorHandle::new(tx);

        for function in program.declarations.iter().filter_map(|decl| match decl {
            TopLevelDecl::Func(function) => Some(function),
            _ => None,
        }) {
            signal_router.register_function(function.clone());
        }

        for routine in program.declarations.iter().filter_map(|decl| match decl {
            TopLevelDecl::Routine(r) => Some(r),
            _ => None,
        }) {
            if !signal_router.register_routine_with_params(&routine.name, routine.params.clone(), routine.body.clone()) {
                return Err(format!("Duplicate routine name at runtime: '{}'", routine.name).into());
            }
        }

        for loop_decl in program.declarations.iter().filter_map(|decl| match decl {
            TopLevelDecl::Loop(l) => Some(l),
            _ => None,
        }) {
            if !signal_router.register_loop(&loop_decl.name, loop_decl.body.clone()) {
                return Err(format!("Duplicate loop name at runtime: '{}'", loop_decl.name).into());
            }
        }

        let mut event_task = tokio::spawn(async move {
            event_loop.run().await;
        });

        if verbose {
            println!("  [Dispatch] Starting entry routine 'main'");
        }
        actor_handle.send_spawn(Message {
            routine_name: main_routine.name.clone(),
            payload: "Routine main started".to_string(),
            statements: main_routine.body.clone(),
            is_loop: false,
            bindings: Default::default(),
        }).await?;
        drop(actor_handle);
        tokio::select! {
            result = &mut event_task => { result?; }
            ctrl_c = tokio::signal::ctrl_c() => {
                ctrl_c?;
                eprintln!("\nInterrupt received; stopping all running routines and loops...");
                shutdown_handle.shutdown();
                event_task.await?;
            }
        }
        let errors = signal_router.take_errors();
        if !errors.is_empty() { return Err(errors.join("; ").into()); }
    } else if verbose {
        println!("\nProgram verified successfully! (Library: no `routine main()` entry point)");
    }
    Ok(())
}

async fn run_test_suite_cli() -> Result<(), Box<dyn std::error::Error>> {
    println!("Running test suite validation...\n");

    let valid_files = [
        "programs/00_hello_world.lia",
        "programs/01_structs_and_functions.lia",
        "programs/02_bounded_loops.lia",
        "programs/03_structural_recursion.lia",
        "programs/04_actors_concurrency_spawn.lia",
        "programs/05_arenas_and_local_refs.lia",
        "programs/06_full_pipeline.lia",
        "programs/07_packages_and_imports.lia",
        "programs/08_persistent_loop.lia",
        "programs/09_loop_signals.lia",
        "programs/10_exceptions.lia",
        "programs/11_search_and_sort.lia",
        "programs/12_main_entry.lia",
        "programs/13_library_consumer.lia",
        "programs/14_type_of.lia",
        "programs/15_higher_order.lia",
        "programs/16_generics.lia",
        "programs/17_enums.lia",
        "programs/18_closures.lia",
        "programs/19_option_result.lia",
        "programs/20_tailcalls.lia",
        "programs/21_dictionary_json.lia",
        "programs/math.lia",
        "programs/std.lia",
    ];

    println!("--- Verifying Valid Programs (Language Features) ---");
    for file in &valid_files {
        let source = fs::read_to_string(file)?;
        let mut parser = Parser::new(&source)?;
        let raw_program = parser.parse_program()?;
        let mut packages = PackageManager::new("programs");
        let prog = packages.resolve_program(raw_program)?;

        let mut dag = CallGraphValidator::new();
        dag.build_and_validate(&prog)?;

        let mut tc = TypeChecker::new();
        tc.check_program(&prog)?;

        TerminationChecker::verify_program(&prog)?;

        println!("  [OK] {} passed all checks.", file);
    }

    println!("\n--- Verifying Invalid Programs (Safety Guarantees) ---");
    // invalid_01
    {
        let src = fs::read_to_string("programs/invalid_01_dag_cycle.lia")?;
        let mut p = Parser::new(&src)?;
        let prog = p.parse_program()?;
        let mut dag = CallGraphValidator::new();
        assert!(dag.build_and_validate(&prog).is_err());
        println!("  [OK] invalid_01_dag_cycle successfully rejected (CallGraphCycle).");
    }
    // invalid_02
    {
        let src = fs::read_to_string("programs/invalid_02_infinite_recursion.lia")?;
        let mut p = Parser::new(&src)?;
        let prog = p.parse_program()?;
        assert!(TerminationChecker::verify_program(&prog).is_err());
        println!("  [OK] invalid_02_infinite_recursion successfully rejected (NonDecreasingRecursion).");
    }
    // invalid_03 – rejected at parse level (Ref in routine param)
    {
        let src = fs::read_to_string("programs/invalid_03_spawn_with_ref.lia")?;
        let res = Parser::new(&src).and_then(|mut p| p.parse_program());
        assert!(res.is_err());
        println!("  [OK] invalid_03_spawn_with_ref successfully rejected (SyntaxError).");
    }
    // invalid_04
    {
        let src = fs::read_to_string("programs/invalid_04_while_loop_prohibited.lia")?;
        let res = Parser::new(&src).and_then(|mut p| p.parse_program());
        assert!(res.is_err());
        println!("  [OK] invalid_04_while_loop_prohibited successfully rejected (SyntaxError).");
    }

    println!("\n✅ All suite programs validated with 100% success!");
    Ok(())
}

/// Reads a `.lia` file, runs all validations, computes worst-case
/// complexity for every `func` and `routine`, and writes the annotated
/// source back to `output_path`.
///
/// For each declaration the tool inserts (or replaces) a line like:
/// ```
/// // complexity: O(N)
/// ```
/// immediately before the `func`/`routine` keyword.
fn annotate_complexity(input_path: &str, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Analysing complexity of '{}'...", input_path);
    let source = fs::read_to_string(input_path)?;

    // Parse and validate first.
    let mut parser = Parser::new(&source)?;
    let raw_program = parser.parse_program()?;
    let base_dir = Path::new(input_path).parent().unwrap_or_else(|| Path::new("."));
    let mut pkg_mgr = PackageManager::new(base_dir);
    let program = pkg_mgr.resolve_program(raw_program)?;
    CallGraphValidator::new().build_and_validate(&program)?;
    TypeChecker::new().check_program(&program)?;
    TerminationChecker::verify_program(&program)?;

    // Compute complexities.
    let mut analyzer = ComplexityAnalyzer::new();
    let entries = analyzer.try_analyze(&program)?;

    println!("  Declarations analysed: {}", entries.len());
    for e in &entries {
        println!("    {} {} → {}", e.kind, e.name, e.complexity.to_big_o());
    }

    // Inject complexity comments into the source text.
    let annotated = inject_complexity_comments(&source, &entries);

    fs::write(output_path, &annotated)?;
    println!("\n✅ Annotated source written to '{}'", output_path);
    Ok(())
}

/// Injects `// complexity: O(…)` comments just before each `func`/`routine`
/// declaration line in the source text.
fn inject_complexity_comments(source: &str, entries: &[lia::semantic::complexity::ComplexityEntry]) -> String {
    // Build a lookup: (kind, name) -> big-o string.
    let mut lookup: std::collections::HashMap<(&str, &str), String> = std::collections::HashMap::new();
    for e in entries {
        lookup.insert((e.kind, e.name.as_str()), e.complexity.to_big_o());
    }

    let mut out = String::with_capacity(source.len() + entries.len() * 40);
    let complexity_prefix = "// complexity:";

    for line in source.lines() {
        let trimmed = line.trim_start();

        // Skip lines that are already complexity annotations (avoid duplicates
        // when the user runs `annotate` more than once on the same file).
        if trimmed.starts_with(complexity_prefix) {
            continue;
        }

        // Check if this line starts a func or routine declaration.
        let maybe_entry = if trimmed.starts_with("func ") {
            let name = trimmed["func ".len()..].split(['(', '<', ' ']).next().unwrap_or("").trim();
            lookup.get(&("func", name)).cloned()
        } else if trimmed.starts_with("routine ") {
            let name = trimmed["routine ".len()..].split(['(', '<', ' ']).next().unwrap_or("").trim();
            lookup.get(&("routine", name)).cloned()
        } else {
            None
        };

        if let Some(big_o) = maybe_entry {
            // Preserve the same indentation as the declaration line.
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            out.push_str(indent);
            out.push_str(complexity_prefix);
            out.push(' ');
            out.push_str(&big_o);
            out.push('\n');
        }

        out.push_str(line);
        out.push('\n');
    }

    out
}
