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
6. **Finite Loop Iterations:** A persistent `loop` may be re-enqueued indefinitely, but the compiler proves every isolated iteration terminates under the same bounded-loop and recursion rules used for routines. Complexity is reported per iteration.

---

## ✍️ Syntax Overview

ProgLang does **not** use semicolons. Statements are newline-separated:

### Program entry point

A runnable program starts only from `routine main()`. Other routines run only
when `main` (or work it starts) uses `spawn`. A source file without this entry
routine is a library: it can be imported but does not execute on its own. The
process exits after `main`, all spawned routines, and all active loops finish.

```proglang
routine main() {
    spawn Worker()
}
```

Pressing `Ctrl+C` explicitly stops all active routines and loops, discarding
queued work and preventing persistent loops from being scheduled again.

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

### Persistent loops

`loop` declares a dormant persistent actor. `spawn LoopName()` starts its sole
instance; each iteration runs on the same shared work-stealing queue as
routines and, when its body ends, is queued again. A running loop never
reserves a CPU core. The reserved `terminate` statement ends the current loop
instance:

```proglang
loop Poller() {
    terminate
}
```

### Loop signals

Any function, routine, or loop can send a non-blocking message to a named loop:

```proglang
signal Poller("new work")
```

Loops do not declare parameters. `spawn Poller(value)` starts a dormant
`Poller` and delivers `value`; while `Poller` is already active, the same form
only signals it, so it continues its current execution rather than restarting.
The loop reads the value through `receive.value`. `spawn Poller()` starts it
without a signal, so its first `receive.value` is `None` and
`receive.pending` is `0`.

At the end of every non-terminated iteration, the runtime schedules the next
iteration. It receives the next queued signal in FIFO order; if the mailbox is
empty, that execution receives `None`, just like a parameterless `spawn`.

### Exceptions

Functions can emit exceptions with `throw` and all declarations can handle
them with `try/catch`. Routines and loops may catch exceptions but cannot
throw them directly:

```proglang
func parse(value: String) {
    try {
        throw "invalid input"
    } catch error {
        log(error)
    }
}

routine Worker(value: String) {
    try { parse(value) } catch error { log(error) }
}
```

### Search and sorting examples

[11_search_and_sort.proglang](programs/11_search_and_sort.proglang) contains
a structural linear search (`O(N)`) and bubble sort (`O(N²)`). These examples
are part of the test suite for the static complexity analyzer.
Functions are the only declarations that return values; routines and loops do
not have return values.

Loop names are unique within a program: only one active instance may use a
given name, and a `signal` target must name one of those declared loops.

Messages are appended to that loop's mailbox and one is delivered at the beginning
of its next iteration. `receive.value` is the one message delivered to that
iteration and `receive.pending` is the number still queued; the next iteration
receives the next value. Sending does not wait for the receiving loop.

See [`programs/09_loop_signals.proglang`](programs/09_loop_signals.proglang)
for a routine that emits a signal and a persistent loop that consumes it.

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

### Query one declaration's complexity

```bash
# Explicit kind: func, routine, or loop
cargo run -- complexity programs/03_structural_recursion.proglang func sum_list
# func sum_list: O(N)

cargo run -- complexity programs/04_actors_concurrency_spawn.proglang routine QueueManager
# routine QueueManager: O(N)

cargo run -- complexity programs/09_loop_signals.proglang loop Inbox
# loop Inbox: O(1)
```

When a declaration name is unique, the kind can be omitted:

```bash
cargo run -- complexity programs/09_loop_signals.proglang Inbox
```

---

## 📄 License
MIT License - Feel free to contribute!
