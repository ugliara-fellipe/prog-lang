/// Complexity Analyzer for Lia.
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
use crate::semantic::{flow::FunctionFlow, SemanticError};

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
    flow: Option<FunctionFlow>,
    ignored_function: Option<String>,
}

impl ComplexityAnalyzer {
    pub fn new() -> Self {
        Self { cache: HashMap::new(), flow: None, ignored_function: None }
    }

    /// Run the full analysis and return one [`ComplexityEntry`] per declaration.
    pub fn analyze(&mut self, program: &Program) -> Vec<ComplexityEntry> {
        self.try_analyze(program).expect("complexity requires analyzable call destinations")
    }

    pub fn try_analyze(&mut self, program: &Program) -> Result<Vec<ComplexityEntry>, SemanticError> {
        crate::semantic::dag::CallGraphValidator::new().build_and_validate(program)?;
        self.flow = Some(FunctionFlow::analyze(program)?);
        let mut results = Vec::new();

        // First pass: detect which functions are recursive (needed for the
        // O(N) assignment during the second pass).
        let recursive_funcs: std::collections::HashSet<String> = program
            .declarations
            .iter()
            .filter_map(|d| {
                if let TopLevelDecl::Func(f) = d {
                    if self.flow.as_ref().unwrap().calls_by_owner().get(&f.name).is_some_and(|targets| targets.contains(&f.name)) { Some(f.name.clone()) } else { None }
                } else {
                    None
                }
            })
            .collect();

        // Populate the cache independently of declaration order. The call
        // graph has already been validated as acyclic apart from structural
        // self recursion, so at most one pass per function propagates costs.
        self.cache.clear();
        for _ in 0..program.declarations.len() {
            for decl in &program.declarations {
                if let TopLevelDecl::Func(f) = decl {
                    self.ignored_function = Some(f.name.clone());
                    let cost = if recursive_funcs.contains(&f.name) {
                        self.recursive_complexity(f, &recursive_funcs)
                    } else {
                        self.complexity_of_body(&f.body, &recursive_funcs)
                    };
                    self.cache.insert(f.name.clone(), cost);
                }
            }
        }

        // Second pass: compute complexity.
        for decl in &program.declarations {
            match decl {
                TopLevelDecl::Func(f) => {
                    // A structurally-recursive function over a List/Tree is
                    // O(N) plus the most expensive already-known helper it
                    // calls on each recursive step.  This captures patterns
                    // such as insertion sort: `insertion_sort` calls the
                    // O(N) helper `insert_sorted` once per input element.
                    self.ignored_function = Some(f.name.clone());
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
                    self.ignored_function = None;
                    let complexity = self.complexity_of_body(&r.body, &recursive_funcs);
                    results.push(ComplexityEntry {
                        name: r.name.clone(),
                        kind: "routine",
                        complexity,
                    });
                }
                TopLevelDecl::Loop(l) => {
                    self.ignored_function = None;
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

        Ok(results)
    }

    // ------------------------------------------------------------------ //
    // Internal helpers                                                     //
    // ------------------------------------------------------------------ //

    fn recursive_complexity(
        &self,
        func: &FuncDecl,
        recursive: &std::collections::HashSet<String>,
    ) -> Complexity {
        Complexity::Polynomial(1).multiply(&self.complexity_of_body(&func.body, recursive))
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
            Statement::Spawn(Expr::FunctionCall { args, .. } | Expr::IndirectCall { args, .. }) => args.iter()
                .map(|arg| self.complexity_of_expr(arg, recursive))
                .fold(Complexity::Constant, |cost, arg| cost.max(&arg)),
            Statement::Spawn(expr) => self.complexity_of_expr(expr, recursive),
            Statement::Terminate => Complexity::Constant,
            Statement::Signal { payload, .. } => self.complexity_of_expr(payload, recursive),
            Statement::Throw(expr) => self.complexity_of_expr(expr, recursive),
            Statement::TryCatch { try_body, catch_body, .. } => {
                self.complexity_of_body(try_body, recursive)
                    .max(&self.complexity_of_body(catch_body, recursive))
            }
            Statement::VarDecl { value, .. } | Statement::MemorySet { value, .. } => self.complexity_of_expr(value, recursive),
            Statement::ExprStmt(e) => self.complexity_of_expr(e, recursive),

            // `for x in collection { body }` → O(|collection|) × O(body)
            Statement::ForLoop { collection, body, .. } => {
                let body_cost = self.complexity_of_body(body, recursive);
                // Each iteration of a `for` multiplies by N.
                Complexity::Polynomial(1).multiply(&body_cost).max(&self.complexity_of_expr(collection, recursive))
            }

            // `match` → max over all branches.
            Statement::Match { target, cases } => cases
                .iter()
                .map(|c| self.complexity_of_body(&c.body, recursive))
                .fold(self.complexity_of_expr(target, recursive), |acc, c| acc.max(&c)),
        }
    }

    fn destination_cost(&self, targets: &std::collections::HashSet<String>) -> Complexity {
        targets.iter().filter(|name| self.ignored_function.as_ref() != Some(*name))
            .map(|name| self.cache.get(name).cloned().unwrap_or(Complexity::Constant))
            .fold(Complexity::Constant, |cost, target| cost.max(&target))
    }

    fn complexity_of_expr(
        &self,
        expr: &Expr,
        recursive: &std::collections::HashSet<String>,
    ) -> Complexity {
        match expr {
            Expr::IndirectCall { target, args } => {
                let targets = self.flow.as_ref().expect("flow computed").targets_for(expr);
                args.iter().map(|arg| self.complexity_of_expr(arg, recursive))
                    .fold(self.destination_cost(&targets).max(&self.complexity_of_expr(target, recursive)), |cost, arg| cost.max(&arg))
            }
            Expr::FunctionCall { args, .. } => {
                let targets = self.flow.as_ref().expect("flow computed").targets_for(expr);
                args.iter().map(|arg| self.complexity_of_expr(arg, recursive))
                    .fold(self.destination_cost(&targets), |cost, arg| cost.max(&arg))
            }
            Expr::BinaryOp { left, right, .. } => {
                let l = self.complexity_of_expr(left, recursive);
                let r = self.complexity_of_expr(right, recursive);
                l.max(&r)
            }
            Expr::MemberAccess { target, .. } | Expr::Specialize { target, .. } => self.complexity_of_expr(target, recursive),
            Expr::RefCreate(inner) => self.complexity_of_expr(inner, recursive),
            Expr::ListLiteral(items) => items.iter().map(|item| self.complexity_of_expr(item, recursive)).fold(Complexity::Constant, |cost, item| cost.max(&item)),
            // Literals and plain variable reads are O(1).
            _ => Complexity::Constant,
        }
    }
}

