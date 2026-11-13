use std::collections::HashSet;
use crate::ast::*;
use crate::semantic::SemanticError;

pub struct TerminationChecker;

impl TerminationChecker {
    pub fn verify_program(program: &Program) -> Result<(), SemanticError> {
        for decl in &program.declarations {
            match decl {
                TopLevelDecl::Func(func) => {
                    Self::verify_func_termination(func)?;
                }
                TopLevelDecl::Routine(routine) => {
                    Self::verify_routine_termination(routine)?;
                }
                TopLevelDecl::Loop(loop_decl) => {
                    Self::verify_loop_iteration(loop_decl)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn verify_func_termination(func: &FuncDecl) -> Result<(), SemanticError> {
        let func_name = &func.name;
        let sub_components = HashSet::new();

        for stmt in &func.body {
            Self::check_stmt_termination(stmt, func_name, &sub_components)?;
        }
        Ok(())
    }

    /// Verify that a routine body terminates just like a function body.
    /// All the same rules apply: no non-decreasing recursion via function
    /// calls, bounded loops, etc.  Spawn statements are excluded from the
    /// self-recursion check because spawning creates a *new* isolated actor
    /// – the current routine still finishes.  However the *functions* called
    /// inside a routine body must still obey structural induction.
    fn verify_routine_termination(routine: &RoutineDecl) -> Result<(), SemanticError> {
        let routine_name = &routine.name;
        let sub_components = HashSet::new();

        for stmt in &routine.body {
            Self::check_routine_stmt_termination(stmt, routine_name, &sub_components)?;
        }
        Ok(())
    }

    /// A loop is persistent, but its re-enqueue operation is deliberately
    /// outside the program body. We therefore prove termination of one
    /// isolated iteration with exactly the same rules as a routine; the
    /// runtime may safely schedule the next proven-finite iteration.
    fn verify_loop_iteration(loop_decl: &LoopDecl) -> Result<(), SemanticError> {
        let sub_components = HashSet::new();
        for stmt in &loop_decl.body {
            Self::check_routine_stmt_termination(stmt, &loop_decl.name, &sub_components)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Shared statement / expression walkers for *functions*
    // ------------------------------------------------------------------

    fn check_stmt_termination(
        stmt: &Statement,
        func_name: &str,
        sub_components: &HashSet<String>,
    ) -> Result<(), SemanticError> {
        match stmt {
            Statement::MemorySet { value, .. } => Self::check_expr_termination(value, func_name, sub_components)?,
            Statement::Match { cases, .. } => {
                for case in cases {
                    let mut local_subs = sub_components.clone();
                    Self::extract_pattern_bindings(&case.pattern, &mut local_subs);
                    for s in &case.body {
                        Self::check_stmt_termination(s, func_name, &local_subs)?;
                    }
                }
            }
            Statement::ExprStmt(expr) | Statement::Spawn(expr) => {
                Self::check_expr_termination(expr, func_name, sub_components)?;
            }
            Statement::Signal { payload, .. } => {
                Self::check_expr_termination(payload, func_name, sub_components)?;
            }
            Statement::Throw(expr) => {
                Self::check_expr_termination(expr, func_name, sub_components)?;
            }
            Statement::TryCatch { try_body, catch_body, .. } => {
                for statement in try_body.iter().chain(catch_body) {
                    Self::check_stmt_termination(statement, func_name, sub_components)?;
                }
            }
            Statement::VarDecl { value, .. } => {
                Self::check_expr_termination(value, func_name, sub_components)?;
            }
            Statement::ForLoop { var_name, collection, body } => {
                Self::check_expr_termination(collection, func_name, sub_components)?;
                let mut loop_subs = sub_components.clone();
                // If iterating over a subcomponent (e.g. `tail`), elements drawn from it (`sub`)
                // are also structurally smaller subcomponents.
                if let Expr::Variable(coll_name) = collection {
                    if sub_components.contains(coll_name) {
                        loop_subs.insert(var_name.clone());
                    }
                }
                for s in body {
                    Self::check_stmt_termination(s, func_name, &loop_subs)?;
                }
            }
            Statement::Terminate => {}
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Statement / expression walkers for *routines*
    // ------------------------------------------------------------------

    /// Like `check_stmt_termination` but for routine bodies.
    /// `spawn` expressions are *not* checked for self-recursion on the
    /// routine name because spawning is asynchronous and does not prevent
    /// the current routine from terminating.  Function calls inside the
    /// routine body *are* still checked normally.
    fn check_routine_stmt_termination(
        stmt: &Statement,
        routine_name: &str,
        sub_components: &HashSet<String>,
    ) -> Result<(), SemanticError> {
        match stmt {
            Statement::MemorySet { value, .. } => Self::check_expr_termination(value, routine_name, sub_components)?,
            Statement::Match { cases, .. } => {
                for case in cases {
                    let mut local_subs = sub_components.clone();
                    Self::extract_pattern_bindings(&case.pattern, &mut local_subs);
                    for s in &case.body {
                        Self::check_routine_stmt_termination(s, routine_name, &local_subs)?;
                    }
                }
            }
            Statement::Spawn(expr) => {
                // Spawn creates a new actor – the current routine still
                // terminates, so we only recurse into the spawn arguments
                // to verify any *function* calls inside them.
                Self::check_routine_spawn_expr(expr, routine_name, sub_components)?;
            }
            Statement::Signal { payload, .. } => {
                Self::check_routine_expr_termination(payload, routine_name, sub_components)?;
            }
            Statement::Throw(expr) => {
                Self::check_routine_expr_termination(expr, routine_name, sub_components)?;
            }
            Statement::TryCatch { try_body, catch_body, .. } => {
                for statement in try_body.iter().chain(catch_body) {
                    Self::check_routine_stmt_termination(statement, routine_name, sub_components)?;
                }
            }
            Statement::ExprStmt(expr) => {
                Self::check_routine_expr_termination(expr, routine_name, sub_components)?;
            }
            Statement::VarDecl { value, .. } => {
                Self::check_routine_expr_termination(value, routine_name, sub_components)?;
            }
            Statement::ForLoop { var_name, collection, body } => {
                Self::check_routine_expr_termination(collection, routine_name, sub_components)?;
                let mut loop_subs = sub_components.clone();
                if let Expr::Variable(coll_name) = collection {
                    if sub_components.contains(coll_name) {
                        loop_subs.insert(var_name.clone());
                    }
                }
                for s in body {
                    Self::check_routine_stmt_termination(s, routine_name, &loop_subs)?;
                }
            }
            Statement::Terminate => {}
        }
        Ok(())
    }

    /// Walk an expression inside a routine body.  If we encounter a
    /// *synchronous* function call to a known function that recurses, we
    /// need structural induction just like inside a `func`.
    /// Direct calls to the routine's own name (e.g. `RoutineX(...)`)
    /// outside a `spawn` would be a syntax-level error (routines are not
    /// callable as functions), but we defensively check and reject it.
    fn check_routine_expr_termination(
        expr: &Expr,
        routine_name: &str,
        sub_components: &HashSet<String>,
    ) -> Result<(), SemanticError> {
        match expr {
            Expr::FunctionCall { name, args } => {
                // A routine calling itself synchronously (not via spawn) is
                // an error – routines are not functions, but if the name
                // collides we must reject it to guarantee termination.
                if name == routine_name {
                    return Err(SemanticError::NonDecreasingRecursion(format!(
                        "Actor '{}' calls itself synchronously without spawn; \
                         routines and loops must use 'spawn' for self-invocation",
                        routine_name
                    )));
                }
                for arg in args {
                    Self::check_routine_expr_termination(arg, routine_name, sub_components)?;
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::check_routine_expr_termination(left, routine_name, sub_components)?;
                Self::check_routine_expr_termination(right, routine_name, sub_components)?;
            }
            Expr::RefCreate(inner) => {
                Self::check_routine_expr_termination(inner, routine_name, sub_components)?;
            }
            Expr::MemberAccess { target, .. } => {
                Self::check_routine_expr_termination(target, routine_name, sub_components)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Walk into the arguments of a `spawn` expression.  The top-level
    /// call target is the spawned routine – we do *not* flag it as self-
    /// recursion because spawning is async.  However any nested function
    /// calls inside the spawn arguments must still be checked.
    fn check_routine_spawn_expr(
        expr: &Expr,
        routine_name: &str,
        sub_components: &HashSet<String>,
    ) -> Result<(), SemanticError> {
        if let Expr::FunctionCall { args, .. } = expr {
            // Skip the top-level name (that's the spawned routine).
            // Only recurse into the argument expressions.
            for arg in args {
                Self::check_routine_expr_termination(arg, routine_name, sub_components)?;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Pattern helpers
    // ------------------------------------------------------------------

    fn extract_pattern_bindings(pattern: &Pattern, sub_components: &mut HashSet<String>) {
        match pattern {
            Pattern::Constructor(_, sub_pats) => {
                for pat in sub_pats {
                    if let Pattern::Identifier(name) = pat {
                        sub_components.insert(name.clone());
                    }
                    Self::extract_pattern_bindings(pat, sub_components);
                }
            }
            Pattern::Identifier(name) => {
                sub_components.insert(name.clone());
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Original function expression walker (unchanged)
    // ------------------------------------------------------------------

    fn check_expr_termination(
        expr: &Expr,
        func_name: &str,
        sub_components: &HashSet<String>,
    ) -> Result<(), SemanticError> {
        match expr {
            Expr::FunctionCall { name, args } => {
                if name == func_name {
                    // If direct recursive call, verify that argument is a subcomponent extracted by match
                    // The first argument is the inductive input that proves
                    // recursive descent. Remaining arguments (for example a
                    // search target or accumulator) are carried unchanged.
                    for (index, arg) in args.iter().enumerate() {
                        if index > 0 {
                            continue;
                        }
                        match arg {
                            Expr::Variable(var_name) => {
                                if !sub_components.contains(var_name) {
                                    return Err(SemanticError::NonDecreasingRecursion(format!(
                                        "Argument '{}' in recursive function '{}' is not a structurally smaller subcomponent",
                                        var_name, func_name
                                    )));
                                }
                            }
                            _ => {
                                return Err(SemanticError::NonDecreasingRecursion(format!(
                                    "Expression argument in recursive function '{}' is not a structurally smaller subcomponent",
                                    func_name
                                )));
                            }
                        }
                    }
                }
                for arg in args {
                    Self::check_expr_termination(arg, func_name, sub_components)?;
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::check_expr_termination(left, func_name, sub_components)?;
                Self::check_expr_termination(right, func_name, sub_components)?;
            }
            Expr::RefCreate(inner) => {
                Self::check_expr_termination(inner, func_name, sub_components)?;
            }
            Expr::MemberAccess { target, .. } => {
                Self::check_expr_termination(target, func_name, sub_components)?;
            }
            _ => {}
        }
        Ok(())
    }
}
