# Lia - Finite Tasks, Persistent Loops, and Isolated Actors

This is the reference implementation in **Rust** of the compiler and runtime for **Lia**. Functions and routines terminate, every isolated loop iteration terminates, and actors communicate through explicit asynchronous messages.

---

## 🚀 Project Architecture

The project is structured into modular layers:

```text
lia/
├── grammars/
│   └── Lia.g4         # ANTLR4 Grammar Specification (100% English)
├── programs/               # Test Suite Programs Covering Language Features
│   ├── 00_hello_world.lia
│   ├── 01_structs_and_functions.lia
│   ├── 02_bounded_loops.lia
│   ├── 03_structural_recursion.lia
│   ├── 04_actors_concurrency_spawn.lia
│   ├── 05_arenas_and_local_refs.lia
│   ├── 06_full_pipeline.lia
│   ├── 08_persistent_loop.lia
│   ├── 09_loop_signals.lia
│   ├── 10_exceptions.lia
│   ├── 11_search_and_sort.lia
│   ├── 12_main_entry.lia
│   ├── 13_library_consumer.lia
│   ├── std.lia               # Explicit runtime-backed standard library
│   ├── invalid_01_dag_cycle.lia
│   ├── invalid_02_infinite_recursion.lia
│   ├── invalid_03_spawn_with_ref.lia
│   ├── invalid_04_while_loop_prohibited.lia
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
3. **Structural Decrement Proof:** Direct recursive functions must receive a structurally smaller first argument (for example, `tail` of a `List[T]`). Additional arguments, such as a search target, are immutable carried values.
4. **Absolute Memory Isolation (Confined `Ref[T]`):** Mutable references are not copyable. The `spawn` statement accepts only value-copyable data. `Ref[T]` is forbidden in routine parameters (enforced at parse time).
5. **Instant $O(1)$ Deallocation:** Every scheduled routine or loop iteration executes in an isolated `Arena` (`bumpalo`). Its arena is discarded as a unit on completion.
6. **Finite Loop Iterations:** A persistent `loop` may be re-enqueued indefinitely, but the compiler proves every isolated iteration terminates under the same bounded-loop and recursion rules used for routines. Complexity is reported per iteration.

---

## ✍️ Syntax Overview

Lia does **not** use semicolons. Statements are newline-separated:

### Program entry point

A runnable program starts only from `routine main()`. Other routines run only
when `main` (or work it starts) uses `spawn`. A source file without this entry
routine is a library: it can be imported but does not execute on its own. The
process exits after `main`, all spawned routines, and all active loops finish.

```lia
routine main() {
    spawn Worker()
}
```

Pressing `Ctrl+C` explicitly stops all active routines and loops, discarding
queued work and preventing persistent loops from being scheduled again.

```lia
// Package import
import math
import math.{Vector2D, multiply}
import std

// Variable declaration
let x = 42
let mut counter: Int = 0

// Function call expression
std.log("Hello, World!")

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

The expression `type variable` returns the variable's type name as a string,
for example `type count` returns `"Int"` when `count` is an integer. An
unknown type is reported as `"Unknown"`.

`loop` declares a dormant persistent actor. `spawn LoopName()` starts its sole
instance; each iteration runs on the same shared work-stealing queue as
routines and, when its body ends, is queued again. A running loop never
reserves a CPU core. The reserved `terminate` statement ends the current loop
instance:

```lia
loop Poller() {
    terminate
}
```

### Loop signals

Any function, routine, or loop can send a non-blocking message to a named loop:

```lia
signal Poller("new work")
```

Loops do not declare parameters. `spawn Poller(value)` starts a dormant
`Poller` and delivers `value`; while `Poller` is already active, the same form
only signals it, so it continues its current execution rather than restarting.
The loop reads the value through `message.value`. `spawn Poller()` starts it
without a signal, so its first `message.value` is `None` and
`message.pending` is `0`.

At the end of every non-terminated iteration, the runtime schedules the next
iteration. It receives the next queued signal in FIFO order; if the mailbox is
empty, that execution receives `None`, just like a parameterless `spawn`.

### Exceptions

Functions can emit exceptions with `throw` and all declarations can handle
them with `try/catch`. Routines and loops may catch exceptions but cannot
throw them directly:

```lia
import std

func parse(value: String) {
    try {
        throw "invalid input"
    } catch error {
        std.log(error)
    }
}

routine Worker(value: String) {
    try { parse(value) } catch error { std.log(error) }
}
```

### Search and sorting examples

[11_search_and_sort.lia](programs/11_search_and_sort.lia) is a
runnable fixture over literal integer lists. It demonstrates linear search and
maximum (`O(N)` worst case), `insert_sorted` (`O(N)`), and insertion sort
(`O(N²)` worst case). Its `import std` is explicit: `std.equals`, `std.max`,
`std.empty`, and `std.prepend` are package declarations backed by the runtime,
while the algorithms themselves are ordinary Lia functions. The integration
test executes their results; the static-analysis test checks those bounds against
the usual literature classes.
The `std` package is closed: it exposes only certified terminating primitives
over finite values, including `std.log` for a finite terminal emission; it
cannot contain routines, loops, imports, or user-defined bodies, and must be
imported explicitly.

### Programas executáveis e bibliotecas

Arquivos com `routine main()` são executáveis: o runtime inicia somente essa
rotina. Os demais arquivos em `programs/` são bibliotecas/fixtures de
compilação e não disparam trabalho ao serem chamados diretamente. Todo exemplo
que emite texto importa `std` e usa `std.log(...)`; não há mais logging implícito
do runtime. A suíte executa todos os arquivos válidos e verifica as saídas dos
exemplos com `main`.

Uma biblioteca cujo arquivo começa com número pode ser importada pelo nome
entre aspas, por exemplo `import "03_structural_recursion"`. O programa
[`13_library_consumer.lia`](programs/13_library_consumer.lia) executa
funções dessa biblioteca importada.
Functions are the only declarations that return values; routines and loops do
not have return values.

Loop names are unique within a program: only one active instance may use a
given name, and a `signal` target must name one of those declared loops.

Messages are appended to that loop's mailbox and one is delivered at the beginning
of its next iteration. `message.value` is the one message delivered to that
iteration and `message.pending` is the number still queued; the next iteration
receives the next value. Sending does not wait for the receiving loop.

Loops can preserve values across iterations with `memory name = expression`
and read them with `memory name`. Each loop keeps a private memory map; an
unset name reads as `None`, and `terminate` discards that loop's memory.

```lia
loop Counter() {
    memory checkpoint = "started"
    std.log(memory checkpoint)
    terminate
}
```

See [`programs/09_loop_signals.lia`](programs/09_loop_signals.lia)
for a routine that emits a signal and a persistent loop that consumes it.

### Higher-order functions

Named functions without captured state can be passed, returned, stored in
variables or lists, and copied into routine arguments through `spawn`:

```lia
func double(x: Int) -> Int { x * 2 }
func apply(f: (Int) -> Int, x: Int) -> Int { f(x) }
func choose() -> (Int) -> Int { double }
routine Worker(f: (Int) -> Int) { f(5) }
routine main() {
    let copy: (Int) -> Int = double
    let result = apply(copy, choose()(3))
    spawn Worker(copy)
}
```

The compiler propagates possible function targets to a fixed point and uses
them for cycle, termination, and complexity checks. Unresolved indirect calls
and indirect cycles are rejected. Capturing closures and generic functions are described in
[CLOSURES.md](CLOSURES.md) and [GENERICOS.md](GENERICOS.md). See the [mathematical model](ORDEM_SUPERIOR.md) and
[executable example](programs/15_higher_order.lia).

### Closures with value capture

Closures use explicit parameter and return types and capture the free local
variables they read by value:

```lia
func adder(base: Int) -> (Int) -> Int {
    |x: Int| -> Int { base + x }
}
```

Returned closures retain their copied environments. They can be passed to
functions and routines, stored in lists or enum variants, and copied through
`spawn`. Capturing references or data containing references is rejected.
See [CLOSURES.md](CLOSURES.md) and [18_closures.lia](programs/18_closures.lia).

### Tail calls

Calls that provide the final result of a function or closure execute through an
iterative trampoline. The optional `tailcall` keyword asserts that the call is
in a valid tail position:

```lia
func count(items: List[Int], acc: Int) -> Int {
    match items {
        Nil => acc,
        Cons(_, tail) => tailcall count(tail, acc + 1)
    }
}
```

Tail calls preserve pending exception handlers and argument evaluation order.
Structural termination checks and indirect-cycle rejection still apply.
See [TAILCALL.md](TAILCALL.md) and [20_tailcalls.lia](programs/20_tailcalls.lia).

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
# Run a program with an entry routine
cargo run -- programs/12_main_entry.lia

# Run with scheduler telemetry
cargo run -- --verbose programs/12_main_entry.lia

# Compile an artifact
cargo run -- compile programs/12_main_entry.lia -o main.liac
```

Files without `routine main()` are libraries: they validate and compile, but
do not dispatch runtime work. Press `Ctrl+C` to force-cancel active work.

### Run Test Suite
```bash
cargo test
```

### Query one declaration's complexity

```bash
# Explicit kind: func, routine, or loop
cargo run -- complexity programs/03_structural_recursion.lia func sum_list
# func sum_list: O(N)

cargo run -- complexity programs/04_actors_concurrency_spawn.lia routine QueueManager
# routine QueueManager: O(N)

cargo run -- complexity programs/09_loop_signals.lia loop Inbox
# loop Inbox: O(1)
```

When a declaration name is unique, the kind can be omitted:

```bash
cargo run -- complexity programs/09_loop_signals.lia Inbox
```

---

## 📄 License
MIT License - Feel free to contribute!

### Generic types and functions

Parametric declarations such as `func identity<T>(x: T) -> T { x }` support
inferred calls (`identity(3)`) and explicit specialization (`identity<Int>(3)`).
See [GENERICOS.md](GENERICOS.md) and [16_generics.lia](programs/16_generics.lia)
for generic callbacks, lists, records, and routine arguments.

### Algebraic data types

Declare discriminated values with `enum Result { Ok(Int), Err(String) }` and
inspect them with exhaustive `match` patterns. Generic and recursive enums,
qualified constructors, and copied actor arguments are supported. See
[ENUMS.md](ENUMS.md) and [17_enums.lia](programs/17_enums.lia).

### Optional values and results

Use `import option` for `Option[T]` (`Some` / `None`) and `import result` for
`Result[T, E]` (`Ok` / `Err`). Helpers such as `option.map`, `option.unwrap_or`,
and `result.and_then` are ordinary typed Lia functions bundled with the compiler.
Custom sum definitions use `type Maybe<T> = Some(T) | None`.
See [OPCIONAIS.md](OPCIONAIS.md) and [19_option_result.lia](programs/19_option_result.lia).

### Dictionaries and JSON

Use `Dict[T]` for String keys and typed values: `let d: Dict[Int] = {count: 3}`.
With `import dict`, `d["count"]` returns `Option[Int]`, and
`dict.set(d, "count", 8)` returns an independent updated value.
Use `Dict[Json]` for heterogeneous JSON data and `import json` for
`json.parse_dict(text)` / `json.stringify(data)`, returning `Result`.
See [DICIONARIOS.md](DICIONARIOS.md) and [21_dictionary_json.lia](programs/21_dictionary_json.lia).

### Compile-time constants

Declare `const MAX_ITER: Int = 250 * 4` globally or inside a block.
Initializers use constant data, names and checked arithmetic; cycles,
overflow and division by zero are compilation errors.
`for i in MAX_ITER { ... }` iterates from zero to the exclusive bound
without allocating a list. Constants support imports and are stored
as precomputed typed data in compiled artifacts.
See [CONSTANTES.md](CONSTANTES.md) and [22_constants.lia](programs/22_constants.lia).

### Doc-strings and API documentation

Use `///` before functions, routines, loops, constants, types, record fields
and enum variants. Comment blocks preserve Markdown paragraphs and code examples.
`lia doc file.lia` prints the API as Markdown; `lia doc file.lia -o api.md`
writes a file after semantic validation, without running the program.
See [DOCSTRINGS.md](DOCSTRINGS.md) and the [generated example](docs/docstrings-api.md).
