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

/// Bundled algebraic data types and helpers, implemented as ordinary Lia source.
/// They are available only after an explicit import, without external files.
pub fn bundled_package(name:&str)->Option<&'static str>{
    match name {"option"=>Some(include_str!("library/option.lia")),"result"=>Some(include_str!("library/result.lia")),"dict"=>Some(include_str!("library/dict.lia")),"json"=>Some(include_str!("library/json.lia")),_=>None}
}

/// These finite natives never invoke callbacks; dictionary operations copy values.
pub fn is_bundled_native_function(name:&str)->bool{
    matches!(name,"dict.empty"|"dict.get"|"dict.set"|"dict.remove"|"dict.has"|"dict.keys"|"dict.values"|"dict.len"|"json.parse"|"json.parse_dict"|"json.stringify"|"json.as_dict"|"json.as_array"|"json.as_string"|"json.as_int"|"json.as_bool"|"json.is_null"|"json.null")
}
