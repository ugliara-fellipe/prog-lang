/// Complexity Analyzer for ProgLang.
///
/// Functions and routines are guaranteed finite, and each isolated loop
/// iteration is guaranteed finite. We can therefore compute a **static
/// worst-case complexity class** for every declaration straight from the AST.
///
/// ## Rules
/// | Construct                       | Added complexity |
/// |----------------------------------|-----------------|
/// | Simple statement (let, spawn …) | O(1)            |
/// | Function call (non-recursive)   | O(cost of callee) |
/// | `for x in e { body }`          | O(|e|) × O(body) |
/// | `match` statement               | max over branches |
/// | Structural-recursive `func`     | O(N) over inductive argument |
///
/// The result is expressed as a [`Complexity`] value that knows how to
/// render itself as a readable Big-O string such as `O(1)`, `O(N)`,
/// `O(N²)`, or `O(N^k)` for deeper nesting.

use std::collections::HashMap;
use crate::ast::*;

/// Symbolic worst-case complexity.
#[derive(Debug, Clone, PartialEq)]
pub enum Complexity {
    /// O(1) – constant work.
    Constant,
    /// O(N^k) where k ≥ 1 – polynomial in the size of the inductive input.
    Polynomial(u32),
}

impl Complexity {
    /// Multiply two complexities (nested loops / calls).
    pub fn multiply(&self, other: &Self) -> Self {
        match (self, other) {
            (Complexity::Constant, x) | (x, Complexity::Constant) => x.clone(),
            (Complexity::Polynomial(a), Complexity::Polynomial(b)) => {
                Complexity::Polynomial(a + b)
            }
        }
    }

    /// Take the maximum (sequential composition / match branches).
    pub fn max(&self, other: &Self) -> Self {
        match (self, other) {
            (Complexity::Polynomial(a), Complexity::Polynomial(b)) => {
                Complexity::Polynomial(*a.max(b))
            }
            (p @ Complexity::Polynomial(_), Complexity::Constant) => p.clone(),
            (Complexity::Constant, p @ Complexity::Polynomial(_)) => p.clone(),
            (Complexity::Constant, Complexity::Constant) => Complexity::Constant,
        }
    }

    /// Human-readable Big-O notation.
    pub fn to_big_o(&self) -> String {
        match self {
            Complexity::Constant => "O(1)".to_string(),
            Complexity::Polynomial(1) => "O(N)".to_string(),
            Complexity::Polynomial(2) => "O(N²)".to_string(),
            Complexity::Polynomial(k) => format!("O(N^{})", k),
        }
    }
}

/// Result of analyzing one top-level declaration.
#[derive(Debug, Clone)]
pub struct ComplexityEntry {
    /// Name of the function or routine.
    pub name: String,
    /// Whether it is a `func` or a `routine`.
    pub kind: &'static str,
    /// Computed worst-case complexity.
    pub complexity: Complexity,
}

/// Analyses all declarations in a program and returns one entry per decl.
pub struct ComplexityAnalyzer {
    /// Already-computed complexity for each named function (used for call resolution).
    cache: HashMap<String, Complexity>,
}

impl ComplexityAnalyzer {
    pub fn new() -> Self {
        Self { cache: HashMap::new() }
    }

    /// Run the full analysis and return one [`ComplexityEntry`] per declaration.
    pub fn analyze(&mut self, program: &Program) -> Vec<ComplexityEntry> {
        let mut results = Vec::new();

        // First pass: detect which functions are recursive (needed for the
        // O(N) assignment during the second pass).
        let recursive_funcs: std::collections::HashSet<String> = program
            .declarations
            .iter()
            .filter_map(|d| {
                if let TopLevelDecl::Func(f) = d {
                    if Self::is_recursive_func(f) { Some(f.name.clone()) } else { None }
                } else {
                    None
                }
            })
            .collect();

        // Second pass: compute complexity.
        for decl in &program.declarations {
            match decl {
                TopLevelDecl::Func(f) => {
                    // A structurally-recursive function over a List/Tree is
                    // O(N) plus the most expensive already-known helper it
                    // calls on each recursive step.  This captures patterns
                    // such as insertion sort: `insertion_sort` calls the
                    // O(N) helper `insert_sorted` once per input element.
                    let complexity = if recursive_funcs.contains(&f.name) {
                        self.recursive_complexity(f, &recursive_funcs)
                    } else {
                        self.complexity_of_body(&f.body, &recursive_funcs)
                    };
                    self.cache.insert(f.name.clone(), complexity.clone());
                    results.push(ComplexityEntry {
                        name: f.name.clone(),
                        kind: "func",
                        complexity,
                    });
                }
                TopLevelDecl::Routine(r) => {
                    let complexity = self.complexity_of_body(&r.body, &recursive_funcs);
                    results.push(ComplexityEntry {
                        name: r.name.clone(),
                        kind: "routine",
                        complexity,
                    });
                }
                TopLevelDecl::Loop(l) => {
                    let complexity = self.complexity_of_body(&l.body, &recursive_funcs);
                    results.push(ComplexityEntry {
                        name: l.name.clone(),
                        kind: "loop",
                        complexity,
                    });
                }
                TopLevelDecl::Type(_) | TopLevelDecl::Import(_) => {}
            }
        }

        results
    }

    // ------------------------------------------------------------------ //
    // Internal helpers                                                     //
    // ------------------------------------------------------------------ //

    fn is_recursive_func(func: &FuncDecl) -> bool {
        for stmt in &func.body {
            if Self::stmt_calls(&func.name, stmt) {
                return true;
            }
        }
        false
    }

    fn recursive_complexity(
        &self,
        func: &FuncDecl,
        recursive: &std::collections::HashSet<String>,
    ) -> Complexity {
        let helper_cost = Self::called_function_names(&func.body)
            .into_iter()
            .filter(|name| name != &func.name)
            .map(|name| {
                self.cache.get(&name).cloned().unwrap_or_else(|| {
                    if recursive.contains(&name) {
                        Complexity::Polynomial(1)
                    } else {
                        Complexity::Constant
                    }
                })
            })
            .fold(Complexity::Constant, |acc, cost| acc.max(&cost));

        match helper_cost {
            Complexity::Constant => Complexity::Polynomial(1),
            Complexity::Polynomial(power) => Complexity::Polynomial(power + 1),
        }
    }

    fn called_function_names(body: &[Statement]) -> Vec<String> {
        let mut names = Vec::new();
        for statement in body {
            Self::collect_call_names_from_stmt(statement, &mut names);
        }
        names
    }

    fn collect_call_names_from_stmt(statement: &Statement, names: &mut Vec<String>) {
        match statement {
            Statement::ExprStmt(expr) | Statement::Spawn(expr) | Statement::Throw(expr) => {
                Self::collect_call_names_from_expr(expr, names);
            }
            Statement::Signal { payload, .. } | Statement::VarDecl { value: payload, .. } => {
                Self::collect_call_names_from_expr(payload, names);
            }
            Statement::TryCatch { try_body, catch_body, .. } => {
                for statement in try_body.iter().chain(catch_body) {
                    Self::collect_call_names_from_stmt(statement, names);
                }
            }
            Statement::ForLoop { collection, body, .. } => {
                Self::collect_call_names_from_expr(collection, names);
                for statement in body {
                    Self::collect_call_names_from_stmt(statement, names);
                }
            }
            Statement::Match { target, cases } => {
                Self::collect_call_names_from_expr(target, names);
                for case in cases {
                    for statement in &case.body {
                        Self::collect_call_names_from_stmt(statement, names);
                    }
                }
            }
            Statement::Terminate => {}
        }
    }

    fn collect_call_names_from_expr(expression: &Expr, names: &mut Vec<String>) {
        match expression {
            Expr::FunctionCall { name, args } => {
                names.push(name.clone());
                for argument in args {
                    Self::collect_call_names_from_expr(argument, names);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::collect_call_names_from_expr(left, names);
                Self::collect_call_names_from_expr(right, names);
            }
            Expr::MemberAccess { target, .. } | Expr::RefCreate(target) => {
                Self::collect_call_names_from_expr(target, names);
            }
            Expr::ListLiteral(items) => {
                for item in items {
                    Self::collect_call_names_from_expr(item, names);
                }
            }
            Expr::IntLiteral(_) | Expr::StringLiteral(_) | Expr::Variable(_) | Expr::ReceiveSignal => {}
        }
    }

    fn stmt_calls(name: &str, stmt: &Statement) -> bool {
        match stmt {
            Statement::ExprStmt(e) | Statement::Spawn(e) => Self::expr_calls(name, e),
            Statement::Signal { payload, .. } => Self::expr_calls(name, payload),
            Statement::Throw(expr) => Self::expr_calls(name, expr),
            Statement::TryCatch { try_body, catch_body, .. } => try_body.iter().chain(catch_body).any(|s| Self::stmt_calls(name, s)),
            Statement::VarDecl { value, .. } => Self::expr_calls(name, value),
            Statement::ForLoop { body, .. } => body.iter().any(|s| Self::stmt_calls(name, s)),
            Statement::Match { cases, .. } => {
                cases.iter().any(|c| c.body.iter().any(|s| Self::stmt_calls(name, s)))
            }
            Statement::Terminate => false,
        }
    }

    fn expr_calls(name: &str, expr: &Expr) -> bool {
        match expr {
            Expr::FunctionCall { name: n, args } => {
                n == name || args.iter().any(|a| Self::expr_calls(name, a))
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::expr_calls(name, left) || Self::expr_calls(name, right)
            }
            Expr::MemberAccess { target, .. } => Self::expr_calls(name, target),
            Expr::RefCreate(inner) => Self::expr_calls(name, inner),
            _ => false,
        }
    }

    /// Complexity of a sequence of statements (sequential composition = max).
    fn complexity_of_body(
        &self,
        body: &[Statement],
        recursive: &std::collections::HashSet<String>,
    ) -> Complexity {
        body.iter()
            .map(|s| self.complexity_of_stmt(s, recursive))
            .fold(Complexity::Constant, |acc, c| acc.max(&c))
    }

    fn complexity_of_stmt(
        &self,
        stmt: &Statement,
        recursive: &std::collections::HashSet<String>,
    ) -> Complexity {
        match stmt {
            // Simple statements without sub-structure.
            Statement::Spawn(_) | Statement::Terminate => Complexity::Constant,
            Statement::Signal { payload, .. } => self.complexity_of_expr(payload, recursive),
            Statement::Throw(expr) => self.complexity_of_expr(expr, recursive),
            Statement::TryCatch { try_body, catch_body, .. } => {
                self.complexity_of_body(try_body, recursive)
                    .max(&self.complexity_of_body(catch_body, recursive))
            }
            Statement::VarDecl { value, .. } => self.complexity_of_expr(value, recursive),
            Statement::ExprStmt(e) => self.complexity_of_expr(e, recursive),

            // `for x in collection { body }` → O(|collection|) × O(body)
            Statement::ForLoop { body, .. } => {
                let body_cost = self.complexity_of_body(body, recursive);
                // Each iteration of a `for` multiplies by N.
                Complexity::Polynomial(1).multiply(&body_cost)
            }

            // `match` → max over all branches.
            Statement::Match { cases, .. } => cases
                .iter()
                .map(|c| self.complexity_of_body(&c.body, recursive))
                .fold(Complexity::Constant, |acc, c| acc.max(&c)),
        }
    }

    fn complexity_of_expr(
        &self,
        expr: &Expr,
        recursive: &std::collections::HashSet<String>,
    ) -> Complexity {
        match expr {
            Expr::FunctionCall { name, args } => {
                // Look up pre-computed complexity of the callee.
                let callee_cost = self
                    .cache
                    .get(name)
                    .cloned()
                    // Recursive functions were cached as O(N) in analyze().
                    // If we see a recursive name here we treat it as O(N).
                    .unwrap_or_else(|| {
                        if recursive.contains(name) {
                            Complexity::Polynomial(1)
                        } else {
                            Complexity::Constant
                        }
                    });
                // Also account for argument expressions (usually O(1)).
                let args_cost = args
                    .iter()
                    .map(|a| self.complexity_of_expr(a, recursive))
                    .fold(Complexity::Constant, |acc, c| acc.max(&c));
                callee_cost.max(&args_cost)
            }
            Expr::BinaryOp { left, right, .. } => {
                let l = self.complexity_of_expr(left, recursive);
                let r = self.complexity_of_expr(right, recursive);
                l.max(&r)
            }
            Expr::MemberAccess { target, .. } => self.complexity_of_expr(target, recursive),
            Expr::RefCreate(inner) => self.complexity_of_expr(inner, recursive),
            // Literals and plain variable reads are O(1).
            _ => Complexity::Constant,
        }
    }
}

