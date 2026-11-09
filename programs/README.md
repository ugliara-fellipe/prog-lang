# Demonstration and Test Suite - ProgLang

This directory contains a suite of programs written in **ProgLang** (`.proglang`), designed for compilation, semantic validation, and testing of language guarantees.

---

## 📂 Programs Overview

### 🟢 Valid Programs (Language Features)

| File | Features Covered |
|---|---|
| [`00_hello_world.proglang`](00_hello_world.proglang) | Classic introductory program: pure function, routine with no arguments, $O(1)$ isolated Arena memory allocation (`Ref[String]`), and terminal printing via `log(...)`. |
| [`01_structs_and_functions.proglang`](01_structs_and_functions.proglang) | Declaration of immutable data structs (`type`), pure functions (`func`), type annotations, immutable and mutable variables (`let`, `let mut`), arithmetic (`+`, `-`, `*`), relational (`==`) operators, and field access (`.`). |
| [`02_bounded_loops.proglang`](02_bounded_loops.proglang) | Strictly bounded iteration loops (`for x in collection`), guaranteed finite termination without `while`. |
| [`03_structural_recursion.proglang`](03_structural_recursion.proglang) | Structural induction on inductive data types (`List[T]`), pattern matching (`match`) with constructors (`Cons`, `Nil`), formal proof of decreasing recursive calls (`tail`). |
| [`04_actors_concurrency_spawn.proglang`](04_actors_concurrency_spawn.proglang) | Actor model with pure value-copy semantics, routine declarations (`routine`), asynchronous dispatch via `spawn`, concurrent actor dispatch inside bounded loops. |
| [`05_arenas_and_local_refs.proglang`](05_arenas_and_local_refs.proglang) | Routine-level $O(1)$ Arena memory management, local mutable references (`Ref[T]` and `ref` operator), instant $O(1)$ deallocation upon routine termination with Zero GC pauses. |
| [`06_full_pipeline.proglang`](06_full_pipeline.proglang) | Full end-to-end integration: recursive inductive data structs, pure recursive functions, concurrent actor routines, routine-local Arena, pattern matching, and recursive subtask spawns. |
| [`07_packages_and_imports.proglang`](07_packages_and_imports.proglang) | Package system and module imports: whole package import (`import math`), specific item imports (`import math.{Vector2D, multiply}`), cross-module function calls and routine spawns. |

---

### 🔴 Invalid Programs (Safety Guarantee Enforcement)

| File | Violation Tested | Compiler Error |
|---|---|---|
| [`invalid_01_dag_cycle.proglang`](invalid_01_dag_cycle.proglang) | DAG Violation: mutual cross-recursion between functions ($A \to B \to A$) | `SemanticError::CallGraphCycle` |
| [`invalid_02_infinite_recursion.proglang`](invalid_02_infinite_recursion.proglang) | Termination Violation: direct recursive call without structural reduction | `SemanticError::NonDecreasingRecursion` |
| [`invalid_03_spawn_with_ref.proglang`](invalid_03_spawn_with_ref.proglang) | Confinement Violation: passing `Ref[T]` across `spawn` boundary (blocked at parse level) | `ParseError::SyntaxError` |
| [`invalid_04_while_loop_prohibited.proglang`](invalid_04_while_loop_prohibited.proglang) | Syntax Violation: use of prohibited unbounded `while` loop | `ParseError::SyntaxError` |

---

## 🚀 How to Compile and Run

### 1. Run a program directly:
```bash
./proglang programs/00_hello_world.proglang
```

### 2. Compile to a binary artifact once and run multiple times:
```bash
./proglang compile programs/06_full_pipeline.proglang
./proglang run programs/06_full_pipeline.progc
```

### 3. Validate all programs in the test suite:
```bash
./proglang --test-all
```

### 4. Run automated test suite:
```bash
cargo test
```
