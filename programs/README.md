# Demonstration and Test Suite - Lia

This directory contains a suite of programs written in **Lia** (`.lia`), designed for compilation, semantic validation, and testing of language guarantees.

---

## 📂 Programs Overview

### 🟢 Valid Programs (Language Features)

| File | Features Covered |
|---|---|
| [`00_hello_world.lia`](00_hello_world.lia) | Executable introductory program: pure function, routine with no arguments, $O(1)$ isolated Arena memory allocation (`Ref[String]`), and terminal printing via explicit `std.log(...)`. |
| [`01_structs_and_functions.lia`](01_structs_and_functions.lia) | Declaration of immutable data structs (`type`), pure functions (`func`), type annotations, immutable and mutable variables (`let`, `let mut`), arithmetic (`+`, `-`, `*`), relational (`==`) operators, and field access (`.`). |
| [`02_bounded_loops.lia`](02_bounded_loops.lia) | Strictly bounded iteration loops (`for x in collection`), guaranteed finite termination without `while`. |
| [`03_structural_recursion.lia`](03_structural_recursion.lia) | Structural induction on inductive data types (`List[T]`), pattern matching (`match`) with constructors (`Cons`, `Nil`), formal proof of decreasing recursive calls (`tail`). |
| [`04_actors_concurrency_spawn.lia`](04_actors_concurrency_spawn.lia) | Actor model with pure value-copy semantics, routine declarations (`routine`), asynchronous dispatch via `spawn`, concurrent actor dispatch inside bounded loops. |
| [`05_arenas_and_local_refs.lia`](05_arenas_and_local_refs.lia) | Routine-level $O(1)$ Arena memory management, local mutable references (`Ref[T]` and `ref` operator), instant $O(1)$ deallocation upon routine termination with Zero GC pauses. |
| [`06_full_pipeline.lia`](06_full_pipeline.lia) | Full end-to-end integration: recursive inductive data structs, pure recursive functions, concurrent actor routines, routine-local Arena, pattern matching, and recursive subtask spawns. |
| [`07_packages_and_imports.lia`](07_packages_and_imports.lia) | Executable package-import example: whole package import (`import math`), specific item imports (`import math.{Vector2D, multiply}`), explicit `std` import, and cross-module function calls. |
| [`08_persistent_loop.lia`](08_persistent_loop.lia) | Executable one-iteration persistent-loop example. |
| [`09_loop_signals.lia`](09_loop_signals.lia) | Executable signal producer/consumer example with `message.value` and `message.pending`. |
| [`10_exceptions.lia`](10_exceptions.lia) | Executable exception-boundary example with functions, routines, and loops. |
| [`11_search_and_sort.lia`](11_search_and_sort.lia) | Executable recursive algorithms using explicit certified `std` primitives. |
| [`12_main_entry.lia`](12_main_entry.lia) | Executable entry-point example; only `main` is dispatched automatically. |
| [`13_library_consumer.lia`](13_library_consumer.lia) | Executable consumer of `03_structural_recursion.lia` via quoted package import. |
| [`14_type_of.lia`](14_type_of.lia) | Type introspection for local values. |
| [`15_higher_order.lia`](15_higher_order.lia) | Function arguments, returns, local copies, lists, patterns, chained calls, and copied callback arguments in `spawn`. |
| [`16_generics.lia`](16_generics.lia) | Generic functions, recursive map, specialized callbacks, and generic routine arguments. |
| [`17_enums.lia`](17_enums.lia) | Generic and recursive enum values, exhaustive matching, and copied enum arguments in spawn. |
| [`18_closures.lia`](18_closures.lia) | Copied closure environments, snapshots, returned and nested closures, composition, enum payloads, and spawn. |
| [`19_option_result.lia`](19_option_result.lia) | Optional and error values, generic sum definitions, mapping, lazy fallback, and copied actor arguments. |
| [`20_tailcalls.lia`](20_tailcalls.lia) | Tail-recursive accumulators, implicit generic TCO, closure callbacks, and preserved exception handlers. |
| [`21_dictionary_json.lia`](21_dictionary_json.lia) | Typed dictionaries, safe lookup, value copies, callback payloads, and JSON round trips. |
| [`math.lia`](math.lia) and [`std.lia`](std.lia) | Libraries: they have no `main` and become available only through explicit imports. |

---

### 🔴 Invalid Programs (Safety Guarantee Enforcement)

| File | Violation Tested | Compiler Error |
|---|---|---|
| [`invalid_01_dag_cycle.lia`](invalid_01_dag_cycle.lia) | DAG Violation: mutual cross-recursion between functions ($A \to B \to A$) | `SemanticError::CallGraphCycle` |
| [`invalid_02_infinite_recursion.lia`](invalid_02_infinite_recursion.lia) | Termination Violation: direct recursive call without structural reduction | `SemanticError::NonDecreasingRecursion` |
| [`invalid_03_spawn_with_ref.lia`](invalid_03_spawn_with_ref.lia) | Confinement Violation: passing `Ref[T]` across `spawn` boundary (blocked at parse level) | `ParseError::SyntaxError` |
| [`invalid_04_while_loop_prohibited.lia`](invalid_04_while_loop_prohibited.lia) | Syntax Violation: use of prohibited unbounded `while` loop | `ParseError::SyntaxError` |

---

## 🚀 How to Compile and Run

### 1. Run a program directly:
```bash
./lia programs/00_hello_world.lia
```

### 2. Compile to a binary artifact once and run multiple times:
```bash
./lia compile programs/06_full_pipeline.lia
./lia run programs/06_full_pipeline.liac
```

### 3. Validate all programs in the test suite:
```bash
./lia --test-all
```

### 4. Run automated test suite:
```bash
cargo test
```
