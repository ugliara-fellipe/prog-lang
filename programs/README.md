# Demonstration and Test Suite - ProgLang

This directory contains a suite of programs written in **ProgLang** (`.proglang` / `.prolang`), designed for compilation, semantic validation, and testing of language guarantees.

---

## 📂 Programs Overview

### 🟢 Valid Programs (Language Features)

| File | Features Covered |
|---|---|
| [`00_hello_world.proglang`](00_hello_world.proglang) | Classic introductory program: pure function, routine with no arguments, $O(1)$ isolated Arena memory allocation (`Ref[String]`), and terminal printing via `log(...)`. |
| [`01_estruturas_e_funcoes.proglang`](01_estruturas_e_funcoes.proglang) | Declaration of immutable data structs (`type`), pure functions (`func`), type annotations, immutable and mutable variables (`let`, `let mut`), arithmetic (`+`, `-`, `*`), relational (`==`) operators, and field access (`.`). |
| [`02_lacos_delimitados.proglang`](02_lacos_delimitados.proglang) | Strictly bounded iteration loops (`for x in collection`), guaranteed finite termination without `while`. |
| [`03_recursao_estrutural_inducao.proglang`](03_recursao_estrutural_inducao.proglang) | Structural induction on inductive data types (`List[T]`), pattern matching (`match`) with constructors (`Cons`, `Nil`), formal proof of decreasing recursive calls (`tail`). |
| [`04_atores_concorrencia_spawn.proglang`](04_atores_concorrencia_spawn.proglang) | Actor model with pure value-copy semantics, routine declarations (`routine`), asynchronous dispatch via `spawn`, concurrent actor dispatch inside bounded loops. |
| [`05_arenas_e_referencias_locais.proglang`](05_arenas_e_referencias_locais.proglang) | Routine-level $O(1)$ Arena memory management, local mutable references (`Ref[T]` and `ref` operator), instant $O(1)$ deallocation upon routine termination with Zero GC pauses. |
| [`06_pipeline_completo.proglang`](06_pipeline_completo.proglang) | Full end-to-end integration: recursive inductive data structs, pure recursive functions, concurrent actor routines, routine-local Arena, pattern matching, and recursive subtask spawns. |

---

### 🔴 Invalid Programs (Safety Guarantee Enforcement)

| File | Violation Tested | Compiler Error |
|---|---|---|
| [`invalid_01_dag_ciclo.proglang`](invalid_01_dag_ciclo.proglang) | DAG Violation: mutual cross-recursion between functions ($A \to B \to A$) | `SemanticError::CallGraphCycle` |
| [`invalid_02_recursao_infinita.proglang`](invalid_02_recursao_infinita.proglang) | Termination Violation: direct recursive call without structural reduction | `SemanticError::NonDecreasingRecursion` |
| [`invalid_03_spawn_com_ref.proglang`](invalid_03_spawn_com_ref.proglang) | Confinement Violation: passing mutable pointer `Ref[T]` across `spawn` boundary | `SemanticError::UncopyableSpawnArgument` |
| [`invalid_04_laco_while_proibido.proglang`](invalid_04_laco_while_proibido.proglang) | Syntax Violation: use of prohibited unbounded `while` loop | `ParseError::SyntaxError` |

---

## 🚀 How to Compile and Run

### 1. Run a program directly:
```bash
./proglang programs/00_hello_world.proglang
```

### 2. Compile to a binary artifact once and run multiple times:
```bash
./proglang compile programs/06_pipeline_completo.proglang
./proglang run programs/06_pipeline_completo.progc
```

### 3. Validate all programs in the test suite:
```bash
./proglang --test-all
```

### 4. Run automated test suite:
```bash
cargo test
```
