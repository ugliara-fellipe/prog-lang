//! The closed set of runtime-backed standard-library primitives.
//!
//! Each primitive is total with respect to termination: it consumes only
//! already-materialized finite values, never invokes user code, never waits,
//! and never schedules a task. `prepend` copies a finite list once; all other
//! primitives perform constant-time scalar work (apart from finite equality).

use crate::ast::{Program, TopLevelDecl};

/// Qualified names which the runtime is allowed to provide.
pub const TOTAL_STANDARD_FUNCTIONS: &[&str] = &[
    "std.equals",
    "std.less_or_equal",
    "std.max",
    "std.empty",
    "std.prepend",
    "std.log",
];

/// Returns whether a qualified `std.*` function is certified to terminate.
pub fn is_total_standard_function(name: &str) -> bool {
    TOTAL_STANDARD_FUNCTIONS.contains(&name)
}

/// Validates the source declaration of the privileged `std` package.
///
/// Standard declarations are signatures only: implementations live in the
/// runtime whitelist above. Keeping the package closed prevents a source file
/// named `std` from exposing an arbitrary routine, loop, or recursive method
/// under the standard-library namespace.
pub fn validate_standard_library(program: &Program) -> Result<(), String> {
    if !program.imports.is_empty() {
        return Err("the std package cannot import other packages".to_string());
    }

    let mut declared = Vec::new();
    for declaration in &program.declarations {
        let TopLevelDecl::Func(function) = declaration else {
            return Err("the std package may declare functions only".to_string());
        };
        if !function.body.is_empty() {
            return Err(format!(
                "standard function '{}' must be implemented by the certified runtime primitive",
                function.name
            ));
        }
        let qualified_name = format!("std.{}", function.name);
        if !is_total_standard_function(&qualified_name) {
            return Err(format!(
                "'{}' is not a certified terminating standard primitive",
                qualified_name
            ));
        }
        if declared.iter().any(|name| name == &qualified_name) {
            return Err(format!("duplicate standard function '{}'", qualified_name));
        }
        declared.push(qualified_name);
    }

    if declared.len() != TOTAL_STANDARD_FUNCTIONS.len()
        || TOTAL_STANDARD_FUNCTIONS
            .iter()
            .any(|name| !declared.iter().any(|declared_name| declared_name == name))
    {
        return Err("the std package must declare every certified runtime primitive exactly once".to_string());
    }

    Ok(())
}
