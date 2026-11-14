### Language Specification: Finite Execution Routines and Isolated Concurrency Model

#### 1. Overview and Core Concept
The goal of this language is to provide a highly concurrent and safe execution environment where **every individual routine has a mathematical guarantee of termination**.
Unlike traditional non-Turing-complete languages that force the entire application to terminate, this language introduces a clear separation between the **Runtime** and **Routines**:
*   **Runtime (System Level):** An infinite Event Loop listening for messages and IO.
*   **Routine (Application Level):** An isolated, short-lived, event-driven execution context whose termination is proven finite at compile-time.

By eliminating the possibility of infinite recursion and unbounded loops within each routine, the language eliminates an entire class of errors (such as thread hangs, stack overflow memory exhaustion, and resource contention), while enabling predictable, high-performance memory management.

--------------------------------------------------------------------------------

#### 2. Theoretical Foundation
##### 2.1 The Halting Problem and Total Languages
By Turing's Theorem, it is impossible to create a generic algorithm that determines whether an arbitrary program in a Turing-complete language will halt. To circumvent this limitation, the language adopts the concept of **Total Programming**: Turing-completeness is consciously removed from the scope of each routine.

##### 2.2 Three Pillars of Termination Guarantee
A routine R is guaranteed to terminate if, and only if:
1.  **Absence of Call Cycles:** The call graph of functions within R forms a Directed Acyclic Graph (DAG).
2.  **Bounded Loops:** All iteration loops have a maximum iteration bound derived from immutable finite data.
3.  **Absence of Blocking Waits:** IO or asynchronous communication operations do not suspend threads inside a while loop. Waiting is translated by the Runtime into spawning a *new* routine.

--------------------------------------------------------------------------------

#### 3. Function Structure and Recursion Prevention
The language prevents infinite recursions such as $A \to B \to C \to A$ or $A \to A$ through two combined strategies:

##### 3.1 Syntax-Level Prevention (Strict Scope)
A function only becomes visible in the symbol table **after** the complete parsing of its code block.
```text
// ❌ SYNTAX ERROR: 'process' does not yet exist in scope
func process(x: Int) -> Int {
    return process(x - 1);
}
```
Functions must be declared in strictly bottom-up dependency order. If function B depends on A, A must be declared grammatically before B.

##### 3.2 Structural Recursion (Induction)
To enable inductive algorithms (such as list or tree processing), recursion is accepted **only if the compiler can prove that the recursive argument is a structurally smaller subcomponent of the input**.
```text
func sum_list(list: List[Int]) -> Int {
    match list {
        Nil => 0,
        // 'tail' is strictly smaller than 'list'
        Cons(head, tail) => head + sum_list(tail) 
    }
}
```
If code attempts to pass an argument of equal or greater size (e.g. `sum_list(list)`), the compiler emits a compilation error.

--------------------------------------------------------------------------------

#### 4. Concurrency Model and Memory Isolation
Concurrency is based on the **Pure Value-Semantics Actor Model**.

##### 4.1 Strict Data Copying (No Shared State)
When a routine spawns another routine via the `spawn` statement, **all arguments are copied by value**. No global pointers or shared data exist between execution instances.
```text
routine ProcessItem(item: Data) {
    // Operates exclusively on its local copy of 'item'
}

routine Pipeline(items: List[Data]) {
    for d in items {
        // 'd' is cloned into ProcessItem's context
        spawn ProcessItem(d); 
    }
}
```

##### 4.2 Reference Confinement (Ref<T>)
Pointers or mutable references (`Ref<T>`) are types strictly confined to the creating routine's scope.
*   **Type Rule:** The type `Ref<T>` **does not implement** the `Copyable` transfer trait.
*   **Compiler Check:** The `spawn` statement only accepts arguments satisfying the `Copyable` constraint. Passing a `Ref<T>` across a `spawn` yields a compile error.

##### 4.3 Safe Asynchronous Recursion (Spawn Graph)
To prevent a routine from triggering an unbounded thread explosion (*fork bomb*):
*  Spawning a routine of a different type ($R_1 \to R_2$) must respect a global DAG of routine types.
*  Spawning the same routine ($R_1 \to R_1$) is allowed **only under structural recursion**, where the payload sent in spawn is strictly smaller than the received payload.

--------------------------------------------------------------------------------

#### 5. Memory Management: Per-Routine Arenas
Because every routine has guaranteed termination and completely isolated memory, the allocation strategy is drastically simplified:
```text
  [ Routine Start ]
          │
          ▼
┌───────────────────────────────────┐
│ Allocate Memory Arena Block K     │
└───────────────────────────────────┘
          │
          ▼
┌───────────────────────────────────┐
│ Execute Finite Instructions       │
│ (Allocations occur in Arena)      │
└───────────────────────────────────┘
          │
          ▼
┌───────────────────────────────────┐
│ Routine Completed                 │
│ Deallocate Entire Arena in O(1)   │
└───────────────────────────────────┘
```

##### Advantages:
1.  **No Garbage Collector:** No GC pauses and no reference counting overhead.
2.  **Allocation Performance:** Memory allocation within a routine consists simply of advancing a bump pointer in the local Arena.
3.  **Instant Deallocation:** When the routine finishes, the entire Arena is freed by resetting the base pointer in $O(1)$ time.

--------------------------------------------------------------------------------

#### 6. Practical Code Example
The following example demonstrates parallel processing of a task tree with guaranteed termination:
```text
// Immutable data structure
type Task {
    id: Int,
    payload: String,
    subtasks: List[Task]
}

// Pure function with guaranteed structural recursion
func count_subtasks(t: Task) -> Int {
    match t.subtasks {
        Nil => 1,
        Cons(head, tail) => count_subtasks(head) + sum_tail(tail)
    }
}

func sum_tail(l: List[Task]) -> Int {
    match l {
        Nil => 0,
        Cons(head, tail) => count_subtasks(head) + sum_tail(tail)
    }
}

// Routine with guaranteed termination and isolation
routine TaskExecutor(task: Task) {
    // Local allocation in Arena
    let mut local_ref: Ref[Int] = ref count_subtasks(task);
    
    log("Processing task ID:", task.id);

    // Work division: recursive spawn with strictly smaller subcomponent
    match task.subtasks {
        Nil => {
            // Leaf work completed
        },
        Cons(head, tail) => {
            // Allowed: 'head' and 'tail' are smaller subcomponents
            spawn TaskExecutor(head);
            
            // Re-package and dispatch tail in new routines
            for sub in tail {
                spawn TaskExecutor(sub);
            }
        }
    }
    
    // Routine completes here. Memory Arena is wiped instantly.
}
```

--------------------------------------------------------------------------------

#### 7. Compiler Rules Matrix
| Code Feature | Permitted? | Compiler Validation Rule |
| ------ | ------ | ------ |
| **Loop while (cond)** | ❌ No | Syntax error: unbounded loops are prohibited. |
| **Loop for item in list** | ✅ Yes | Valid if list is a known finite collection. |
| **Direct Recursion ($A \to A$)** | ⚠️ Conditional | Allowed only if argument is an inductive subcomponent. |
| **Cross Recursion ($A \to B \to A$)** | ❌ No | Rejected via DAG check on Call Graph. |
| **Shared Pointers** | ❌ No | Type `Ref<T>` cannot be exported or sent via `spawn`. |
| **Routine spawn** | ✅ Yes | Allowed if all arguments implement `Copyable`. |
| **Infinite Message Loop** | ✅ Yes | Allowed only in **Runtime** scope, outside user routines. |

--------------------------------------------------------------------------------

#### 8. Conclusion
This design combines the mathematical rigor of total functional languages with the flexibility of actor-based distributed systems. The result is a language ideal for microservices, data processing, and highly concurrent pipelines, delivering peak performance without deadlocks, infinite loops, or memory leaks.
