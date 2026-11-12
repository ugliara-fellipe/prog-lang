pub mod dag;
pub mod type_checker;
pub mod termination;
pub mod complexity;
pub mod packages;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum SemanticError {
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

    #[error("'receive' may only be used inside a loop")]
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
