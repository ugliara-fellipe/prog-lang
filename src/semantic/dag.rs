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
        for decl in &program.declarations {
            if let TopLevelDecl::Func(func) = decl {
                let mut callees = HashSet::new();
                for stmt in &func.body {
                    self.collect_calls_from_stmt(stmt, &mut callees);
                }
                // Allow self-recursion only if handled by structural induction module
                callees.remove(&func.name);
                self.adjacency_list.insert(func.name.clone(), callees);
            }
        }

        self.detect_cycles()
    }

    fn collect_calls_from_stmt(&self, stmt: &Statement, callees: &mut HashSet<String>) {
        match stmt {
            Statement::ExprStmt(expr) | Statement::Spawn(expr) => {
                self.collect_calls_from_expr(expr, callees);
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
