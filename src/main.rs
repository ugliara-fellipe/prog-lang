use std::env;
use std::fs;
use std::path::Path;
use tokio::sync::mpsc;
use proglang::ast::*;
use proglang::parser::Parser;
use proglang::semantic::dag::CallGraphValidator;
use proglang::semantic::type_checker::TypeChecker;
use proglang::semantic::termination::TerminationChecker;
use proglang::runtime::actor::{ActorHandle, Message};
use proglang::runtime::event_loop::RuntimeEventLoop;

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
        println!(" ProgLang - Compiler and Runtime for Finite Routines ");
        println!("============================================================\n");
    }

    if args.len() > 1 {
        let cmd = &args[1];

        // 1. Test suite runner command
        if cmd == "--test-all" || cmd == "--check-all" {
            run_test_suite_cli().await?;
            return Ok(());
        }

        // 2. Command: compile <file.prolang> [-o <file.progc>]
        if cmd == "compile" || cmd == "build" {
            if args.len() < 3 {
                eprintln!("Usage: proglang compile <file.prolang> [-o <file.progc>]");
                std::process::exit(1);
            }
            let input_path = &args[2];
            let output_path = if args.len() >= 5 && args[3] == "-o" {
                args[4].clone()
            } else {
                format!("{}.progc", input_path.trim_end_matches(".prolang").trim_end_matches(".proglang"))
            };
            compile_to_artifact(input_path, &output_path)?;
            return Ok(());
        }

        // 3. Command: run <file.progc | file.prolang>
        if cmd == "run" {
            if args.len() < 3 {
                eprintln!("Usage: proglang run <file.progc | file.prolang>");
                std::process::exit(1);
            }
            let target_path = &args[2];
            if target_path.ends_with(".progc") {
                run_compiled_artifact(target_path, verbose).await?;
            } else {
                compile_and_run_file(target_path, verbose).await?;
            }
            return Ok(());
        }

        // 4. Direct file execution
        if cmd.ends_with(".progc") {
            run_compiled_artifact(cmd, verbose).await?;
        } else {
            compile_and_run_file(cmd, verbose).await?;
        }
        return Ok(());
    }

    // Default fallback if no file argument is given
    let default_file = if Path::new("hello_world.prolang").exists() {
        "hello_world.prolang"
    } else {
        "programs/06_pipeline_completo.proglang"
    };

    compile_and_run_file(default_file, verbose).await?;
    Ok(())
}

/// Compiles ProgLang source code, runs formal verifications, and writes .progc artifact
fn compile_to_artifact(input_path: &str, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔨 Compiling '{}' -> '{}'...", input_path, output_path);
    let source = fs::read_to_string(input_path)?;

    // 1. Parsing
    println!("[1/4] Parsing source code...");
    let mut parser = Parser::new(&source)?;
    let program = parser.parse_program()?;
    println!("  -> AST constructed with {} declarations.", program.declarations.len());

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
    println!("  ./proglang run {}", output_path);
    Ok(())
}

/// Loads and executes a pre-compiled artifact (.progc)
async fn run_compiled_artifact(artifact_path: &str, verbose: bool) -> Result<(), Box<dyn std::error::Error>> {
    if verbose {
        println!("⚡ Loading pre-compiled artifact: {}", artifact_path);
    }
    let bytes = fs::read(artifact_path)?;
    let program: Program = serde_json::from_slice(&bytes)?;
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
    let program = match parser.parse_program() {
        Ok(prog) => {
            if verbose {
                println!("  -> Success: AST constructed with {} declarations.", prog.declarations.len());
            }
            prog
        }
        Err(e) => {
            eprintln!("❌ Syntax Error: {}", e);
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
    let routines: Vec<&RoutineDecl> = program
        .declarations
        .iter()
        .filter_map(|decl| match decl {
            TopLevelDecl::Routine(r) => Some(r),
            _ => None,
        })
        .collect();

    if !routines.is_empty() {
        if verbose {
            println!("\n------------------------------------------------------------");
            println!(" Starting ProgLang Runtime (Tokio EventLoop & Arenas)");
            println!("------------------------------------------------------------");
        }

        let (tx, rx) = mpsc::channel(32);
        let mut event_loop = RuntimeEventLoop::with_verbose(rx, verbose);
        let actor_handle = ActorHandle::new(tx);

        tokio::spawn(async move {
            event_loop.run().await;
        });

        for routine in routines {
            if verbose {
                println!("  [Dispatch] Dispatching execution of routine '{}'", routine.name);
            }
            actor_handle
                .send_spawn(Message {
                    routine_name: routine.name.clone(),
                    payload: format!("Routine {} started", routine.name),
                    statements: routine.body.clone(),
                })
                .await?;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    } else if verbose {
        println!("\nProgram verified successfully! (No 'routine' declarations to dispatch to runtime)");
    }
    Ok(())
}

async fn run_test_suite_cli() -> Result<(), Box<dyn std::error::Error>> {
    println!("Running test suite validation...\n");

    let valid_files = [
        "programs/00_hello_world.proglang",
        "programs/01_estruturas_e_funcoes.proglang",
        "programs/02_lacos_delimitados.proglang",
        "programs/03_recursao_estrutural_inducao.proglang",
        "programs/04_atores_concorrencia_spawn.proglang",
        "programs/05_arenas_e_referencias_locais.proglang",
        "programs/06_pipeline_completo.proglang",
    ];

    println!("--- Verifying Valid Programs (Language Features) ---");
    for file in &valid_files {
        let source = fs::read_to_string(file)?;
        let mut parser = Parser::new(&source)?;
        let prog = parser.parse_program()?;
        
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
        let src = fs::read_to_string("programs/invalid_01_dag_ciclo.proglang")?;
        let mut p = Parser::new(&src)?;
        let prog = p.parse_program()?;
        let mut dag = CallGraphValidator::new();
        assert!(dag.build_and_validate(&prog).is_err());
        println!("  [OK] invalid_01_dag_ciclo successfully rejected (CallGraphCycle).");
    }
    // invalid_02
    {
        let src = fs::read_to_string("programs/invalid_02_recursao_infinita.proglang")?;
        let mut p = Parser::new(&src)?;
        let prog = p.parse_program()?;
        assert!(TerminationChecker::verify_program(&prog).is_err());
        println!("  [OK] invalid_02_recursao_infinita successfully rejected (NonDecreasingRecursion).");
    }
    // invalid_03
    {
        let src = fs::read_to_string("programs/invalid_03_spawn_com_ref.proglang")?;
        let mut p = Parser::new(&src)?;
        let prog = p.parse_program()?;
        let mut tc = TypeChecker::new();
        assert!(tc.check_program(&prog).is_err());
        println!("  [OK] invalid_03_spawn_com_ref successfully rejected (UncopyableSpawnArgument).");
    }
    // invalid_04
    {
        let src = fs::read_to_string("programs/invalid_04_laco_while_proibido.proglang")?;
        let res = Parser::new(&src).and_then(|mut p| p.parse_program());
        assert!(res.is_err());
        println!("  [OK] invalid_04_laco_while_proibido successfully rejected at syntax level (while forbidden).");
    }

    println!("\n✅ All suite programs validated with 100% success!");
    Ok(())
}
