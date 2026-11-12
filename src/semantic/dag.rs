use std::collections::{HashMap, HashSet};
use crate::ast::*;
use crate::semantic::SemanticError;

pub struct CallGraphValidator {
    adjacency_list: HashMap<String, HashSet<String>>,
}

impl CallGraphValidator {
    pub fn new() -> Self {
        Self {
            adjacency_list: HashMap::new(),
        }
    }

    pub fn build_and_validate(&mut self, program: &Program) -> Result<(), SemanticError> {
        // Collect the set of routine names so we can distinguish function
        // calls (which must form a DAG) from spawn targets (routines are
        // allowed to spawn each other freely because each spawn creates an
        // independent actor that terminates on its own).
        let routine_names: HashSet<String> = program
            .declarations
            .iter()
            .filter_map(|d| {
                match d {
                    TopLevelDecl::Routine(r) => Some(r.name.clone()),
                    TopLevelDecl::Loop(l) => Some(l.name.clone()),
                    _ => None,
                }
            })
            .collect();

        for decl in &program.declarations {
            match decl {
                TopLevelDecl::Func(func) => {
                    let mut callees = HashSet::new();
                    for stmt in &func.body {
                        self.collect_calls_from_stmt(stmt, &mut callees);
                    }
                    // Allow self-recursion only if handled by structural induction module
                    callees.remove(&func.name);
                    self.adjacency_list.insert(func.name.clone(), callees);
                }
                TopLevelDecl::Routine(routine) => {
                    // Collect *synchronous* function calls made inside the
                    // routine body.  Spawn targets are excluded because
                    // they don't affect the current routine's termination.
                    let mut callees = HashSet::new();
                    for stmt in &routine.body {
                        self.collect_func_calls_from_routine_stmt(
                            stmt,
                            &routine_names,
                            &mut callees,
                        );
                    }
                    callees.remove(&routine.name);
                    self.adjacency_list.insert(routine.name.clone(), callees);
                }
                TopLevelDecl::Loop(loop_decl) => {
                    let mut callees = HashSet::new();
                    for stmt in &loop_decl.body {
                        self.collect_func_calls_from_routine_stmt(stmt, &routine_names, &mut callees);
                    }
                    self.adjacency_list.insert(loop_decl.name.clone(), callees);
                }
                _ => {}
            }
        }

        self.detect_cycles()
    }

    // ------------------------------------------------------------------
    // Helpers for *function* bodies (original logic, unchanged)
    // ------------------------------------------------------------------

    fn collect_calls_from_stmt(&self, stmt: &Statement, callees: &mut HashSet<String>) {
        match stmt {
            Statement::ExprStmt(expr) | Statement::Spawn(expr) => {
                self.collect_calls_from_expr(expr, callees);
            }
            Statement::Signal { payload, .. } => self.collect_calls_from_expr(payload, callees),
            Statement::Throw(expr) => self.collect_calls_from_expr(expr, callees),
            Statement::TryCatch { try_body, catch_body, .. } => {
                for statement in try_body.iter().chain(catch_body) {
                    self.collect_calls_from_stmt(statement, callees);
                }
            }
            Statement::VarDecl { value, .. } => {
                self.collect_calls_from_expr(value, callees);
            }
            Statement::ForLoop { collection, body, .. } => {
                self.collect_calls_from_expr(collection, callees);
                for s in body {
                    self.collect_calls_from_stmt(s, callees);
                }
            }
            Statement::Match { target, cases } => {
                self.collect_calls_from_expr(target, callees);
                for case in cases {
                    for s in &case.body {
                        self.collect_calls_from_stmt(s, callees);
                    }
                }
            }
            Statement::Terminate => {}
        }
    }

    fn collect_calls_from_expr(&self, expr: &Expr, callees: &mut HashSet<String>) {
        match expr {
            Expr::FunctionCall { name, args } => {
                callees.insert(name.clone());
                for arg in args {
                    self.collect_calls_from_expr(arg, callees);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.collect_calls_from_expr(left, callees);
                self.collect_calls_from_expr(right, callees);
            }
            Expr::RefCreate(inner) | Expr::MemberAccess { target: inner, .. } => {
                self.collect_calls_from_expr(inner, callees);
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Helpers for *routine* bodies
    // ------------------------------------------------------------------
    // Only collect synchronous function calls.  `spawn` targets are
    // intentionally omitted — they run as independent actors and do not
    // contribute to the current routine's call graph.

    fn collect_func_calls_from_routine_stmt(
        &self,
        stmt: &Statement,
        routine_names: &HashSet<String>,
        callees: &mut HashSet<String>,
    ) {
        match stmt {
            Statement::Spawn(expr) => {
                // Only recurse into the *arguments* of the spawn call to
                // catch nested function calls.  The top-level spawn target
                // is a routine and is not added to the function call graph.
                if let Expr::FunctionCall { args, .. } = expr {
                    for arg in args {
                        self.collect_func_calls_from_routine_expr(arg, routine_names, callees);
                    }
                }
            }
            Statement::Signal { payload, .. } => {
                self.collect_func_calls_from_routine_expr(payload, routine_names, callees);
            }
            Statement::Throw(expr) => {
                self.collect_func_calls_from_routine_expr(expr, routine_names, callees);
            }
            Statement::TryCatch { try_body, catch_body, .. } => {
                for statement in try_body.iter().chain(catch_body) {
                    self.collect_func_calls_from_routine_stmt(statement, routine_names, callees);
                }
            }
            Statement::ExprStmt(expr) => {
                self.collect_func_calls_from_routine_expr(expr, routine_names, callees);
            }
            Statement::VarDecl { value, .. } => {
                self.collect_func_calls_from_routine_expr(value, routine_names, callees);
            }
            Statement::ForLoop { collection, body, .. } => {
                self.collect_func_calls_from_routine_expr(collection, routine_names, callees);
                for s in body {
                    self.collect_func_calls_from_routine_stmt(s, routine_names, callees);
                }
            }
            Statement::Match { target, cases } => {
                self.collect_func_calls_from_routine_expr(target, routine_names, callees);
                for case in cases {
                    for s in &case.body {
                        self.collect_func_calls_from_routine_stmt(s, routine_names, callees);
                    }
                }
            }
            Statement::Terminate => {}
        }
    }

    fn collect_func_calls_from_routine_expr(
        &self,
        expr: &Expr,
        routine_names: &HashSet<String>,
        callees: &mut HashSet<String>,
    ) {
        match expr {
            Expr::FunctionCall { name, args } => {
                // Only add *function* calls — skip routine names since
                // calling a routine synchronously is not permitted in the
                // language and would be caught elsewhere.
                if !routine_names.contains(name) {
                    callees.insert(name.clone());
                }
                for arg in args {
                    self.collect_func_calls_from_routine_expr(arg, routine_names, callees);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.collect_func_calls_from_routine_expr(left, routine_names, callees);
                self.collect_func_calls_from_routine_expr(right, routine_names, callees);
            }
            Expr::RefCreate(inner) | Expr::MemberAccess { target: inner, .. } => {
                self.collect_func_calls_from_routine_expr(inner, routine_names, callees);
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Cycle detection (unchanged)
    // ------------------------------------------------------------------

    fn detect_cycles(&self) -> Result<(), SemanticError> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for node in self.adjacency_list.keys() {
            if self.dfs_cycle(node, &mut visited, &mut rec_stack) {
                return Err(SemanticError::CallGraphCycle(node.clone()));
            }
        }
        Ok(())
    }

    fn dfs_cycle(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> bool {
        if rec_stack.contains(node) {
            return true;
        }
        if visited.contains(node) {
            return false;
        }

        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());

        if let Some(neighbors) = self.adjacency_list.get(node) {
            for neighbor in neighbors {
                if self.dfs_cycle(neighbor, visited, rec_stack) {
                    return true;
                }
            }
        }

        rec_stack.remove(node);
        false
    }
}
