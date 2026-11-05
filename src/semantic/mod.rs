pub mod dag;
pub mod type_checker;
pub mod termination;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum SemanticError {
    #[error("Call graph cycle detected in function calls: {0}")]
    CallGraphCycle(String),

    #[error("Type Error: Ref[T] cannot be passed across spawn to actor ({0})")]
    UncopyableSpawnArgument(String),

    #[error("Termination Violation: Recursion argument is not strictly smaller on inductive parameter ({0})")]
    NonDecreasingRecursion(String),

    #[error("Undeclared symbol: {0}")]
    UndeclaredSymbol(String),
}
