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
│   ├── 01_structs_and_functions.proglang
│   ├── 02_bounded_loops.proglang
│   ├── 03_structural_recursion.proglang
│   ├── 04_actors_concurrency_spawn.proglang
│   ├── 05_arenas_and_local_refs.proglang
│   ├── 06_full_pipeline.proglang
│   ├── invalid_01_dag_cycle.proglang
│   ├── invalid_02_infinite_recursion.proglang
│   ├── invalid_03_spawn_with_ref.proglang
│   ├── invalid_04_while_loop_prohibited.proglang
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
4. **Absolute Memory Isolation (Confined `Ref[T]`):** Mutable references are not copyable. The `spawn` statement accepts only value-copyable data. `Ref[T]` is forbidden in routine parameters (enforced at parse time).
5. **Instant $O(1)$ Deallocation:** Every routine executes in an isolated `Arena` (`bumpalo`). Upon routine completion, the entire arena is destroyed with Zero Garbage Collector pauses.

---

## ✍️ Syntax Overview

ProgLang does **not** use semicolons. Statements are newline-separated:

```proglang
// Package import
import math
import math.{Vector2D, multiply}

// Variable declaration
let x = 42
let mut counter: Int = 0

// Function call expression
log("Hello, World!")

// Spawn a routine (passes arguments by deep copy)
spawn ProcessItem(x)

// Bounded loop (only iteration construct allowed)
for item in collection {
    spawn HandleItem(item)
}

// Pattern matching
match list {
    Nil => 0,
    Cons(head, tail) => head + sum(tail)
}
```

### Copy / Reference Assignment Rules

The same rules apply to variable assignments **and** function arguments:

| Source → Target | Behaviour |
|---|---|
| `type → type` | Copies the value |
| `type → Ref[T]` | Creates a reference to the value |
| `Ref[T] → type` | Copies the value the reference points to |
| `Ref[T] → Ref[T]` | Copies the reference (same arena slot) |

> `Ref[T]` is confined to the current routine's arena and **cannot cross `spawn` boundaries**.

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
./proglang programs/00_hello_world.proglang

# Run any test suite program
./proglang programs/06_full_pipeline.proglang

# Run with full compiler & runtime telemetry
./proglang --verbose programs/06_full_pipeline.proglang
```

### Run Test Suite
```bash
cargo test
```

---

## 📄 License
MIT License - Feel free to contribute!
