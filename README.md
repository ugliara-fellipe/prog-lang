# ProgLang - Language with Finite Routines and Isolated Actors

This is the reference implementation in **Rust** of the compiler and runtime for **ProgLang**, a language designed to guarantee **deterministic finite termination per routine** and **complete state isolation between actors**.

---

## 🚀 Project Architecture

The project is structured into modular layers:

```text
proglang/
├── grammars/
│   └── ProgLang.g4         # ANTLR4 Grammar Specification (100% English)
├── programs/               # Test Suite Programs Covering Language Features
│   ├── 00_hello_world.proglang
│   ├── 01_estruturas_e_funcoes.proglang
│   ├── 02_lacos_delimitados.proglang
│   ├── 03_recursao_estrutural_inducao.proglang
│   ├── 04_atores_concorrencia_spawn.proglang
│   ├── 05_arenas_e_referencias_locais.proglang
│   ├── 06_pipeline_completo.proglang
│   ├── invalid_01_dag_ciclo.proglang
│   ├── invalid_02_recursao_infinita.proglang
│   ├── invalid_03_spawn_com_ref.proglang
│   ├── invalid_04_laco_while_proibido.proglang
│   └── README.md
├── src/
│   ├── ast.rs              # Abstract Syntax Tree (AST) Node Definitions
│   ├── parser.rs           # Lexer and Recursive Descent Parser
│   ├── semantic/
│   │   ├── mod.rs
│   │   ├── dag.rs          # Call Graph Validation (Cycle Detection / Cross-Recursion Prevention)
│   │   ├── type_checker.rs # Type Checking and Ref[T] Confinement (Copyable requirement in spawn)
│   │   └── termination.rs  # Structural Decrement Proof (Induction over Lists/Trees)
│   ├── runtime/
│   │   ├── mod.rs
│   │   ├── arena.rs        # Per-Routine O(1) Arena Memory Management
│   │   ├── actor.rs        # Messaging and Actor Concurrency
│   │   └── event_loop.rs   # Global Tokio Event Loop Runtime
│   ├── lib.rs              # Library Module Exports
│   └── main.rs             # CLI and Runtime Entry Point
└── tests/
    └── suite.rs            # Integration Tests for Test Suite Programs
```

---

## 🛡️ Safety and Termination Guarantees

1. **Absence of Unbounded Loops (`while`):** Prohibited at the parser/syntax level. Only strictly bounded loops `for x in collection` are accepted.
2. **Indefinite Recursion Prevention via DAG:** Pure functions cannot form cyclic cross-calling graphs ($A \to B \to A$).
3. **Structural Decrement Proof:** Direct recursion in pure functions or routines (`spawn`) is allowed only if the compiler proves the argument passed is a strictly smaller subcomponent (e.g. `tail` of a `List[T]`).
4. **Absolute Memory Isolation (Confined `Ref[T]`):** Mutable references do not implement `Copyable`. The `spawn` statement accepts only value-copyable data.
5. **Instant $O(1)$ Deallocation:** Every routine executes in an isolated `Arena` (`bumpalo`). Upon routine completion, the entire arena is destroyed with Zero Garbage Collector pauses.

---

## 🛠️ How to Build and Run

### Prerequisites
* **Rust & Cargo** (Edition 2021 or higher)

### Build Standalone Executable
```bash
cargo build --release
```

### Run Programs Directly
```bash
# Run Hello World
./proglang hello_world.prolang

# Run any test suite program
./proglang programs/06_pipeline_completo.proglang

# Run with full compiler & runtime telemetry
./proglang --verbose hello_world.prolang
```

### Compile Once to Binary Artifact and Run
```bash
# Compile to .progc artifact
./proglang compile hello_world.prolang

# Execute the pre-compiled artifact instantly
./proglang run hello_world.progc
```

### Run Test Suite
```bash
# CLI test runner
./proglang --test-all

# Automated integration tests
cargo test
```

---

## 📄 License
MIT License - Feel free to contribute!
