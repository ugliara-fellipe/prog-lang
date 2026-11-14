pub mod pipes;
pub mod builtins;
pub mod constants;
pub mod json_types;
pub mod tail_calls;
pub mod closures;
pub mod adt;
pub mod dag;
pub mod flow;
pub mod generics;
pub mod type_checker;
pub mod termination;
pub mod complexity;
pub mod packages;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum SemanticError {
    #[error("Pattern guard must have type Bool, found {0:?}")]
    InvalidPatternGuard(crate::ast::TypeSpec),
    #[error("Compile-time constant error: {0}")]
    ConstantError(String),
    #[error("Invalid tailcall: {0}")]
    InvalidTailCall(String),
    #[error("Closure capture cannot be copied by value: {0}")]
    UncopyableCapture(String),
    #[error("Non-exhaustive match for {0}")]
    NonExhaustiveMatch(String),
    #[error("Generic type error: {0}")]
    GenericTypeError(String),
    #[error("Invalid expression: {0}")]
    InvalidExpression(String),
    #[error("Pure functions cannot spawn actors")]
    SpawnInsideFunction,
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),
    #[error("Function argument {index} has type {actual:?}; expected {expected:?}")]
    FunctionArgumentTypeMismatch { index: usize, expected: crate::ast::TypeSpec, actual: crate::ast::TypeSpec },
    #[error("Expression is not a function: {0:?}")]
    NotAFunction(crate::ast::TypeSpec),
    #[error("A value of the declared return type must be returned on every normal path")]
    MissingFunctionReturn,
    #[error("Returned or assigned value has type {actual:?}; expected {expected:?}")]
    FunctionReturnTypeMismatch { expected: crate::ast::TypeSpec, actual: crate::ast::TypeSpec },
    #[error("Call graph cycle detected in function calls: {0}")]
    CallGraphCycle(String),

    #[error("Argument count mismatch in call to {function}: expected {expected}, found {found}")]
    ArgumentCountMismatch { function: String, expected: usize, found: usize },

    #[error("Type Error: Ref[T] cannot be passed across spawn to actor ({0})")]
    UncopyableSpawnArgument(String),

    #[error("Termination Violation: Recursion argument is not strictly smaller on inductive parameter ({0})")]
    NonDecreasingRecursion(String),

    #[error("Package error: {0}")]
    PackageError(String),

    #[error("Undeclared symbol: {0}")]
    UndeclaredSymbol(String),

    #[error("Standard-library function '{0}' requires an explicit import")]
    MissingStandardLibraryImport(String),

    #[error("'terminate' may only be used inside a loop")]
    TerminateOutsideLoop,
    #[error("'memory' may only be used inside a loop")]
    MemoryOutsideLoop,

    #[error("'message' may only be used inside a loop")]
    ReceiveOutsideLoop,

    #[error("Duplicate loop declaration: '{0}'")]
    DuplicateLoopName(String),

    #[error("Signal target is not a declared loop: '{0}'")]
    UnknownLoopSignal(String),

    #[error("Spawn to loop '{loop_name}' accepts at most one signal value, found {found}")]
    LoopSignalArgumentCount { loop_name: String, found: usize },

    #[error("Only functions may throw exceptions")]
    ThrowOutsideFunction,
}
