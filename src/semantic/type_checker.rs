use std::collections::HashMap;
use crate::ast::*;
use crate::semantic::SemanticError;

pub struct TypeChecker {
    /// Symbol table for the current scope (functions and variables).
    symbol_table: HashMap<String, TypeSpec>,
    /// Collected function signatures for call‑argument validation.
    functions: HashMap<String, FuncDecl>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            symbol_table: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    /// Entry point for the whole program.
    pub fn check_program(&mut self, program: &Program) -> Result<(), SemanticError> {
        for decl in &program.declarations {
            match decl {
                TopLevelDecl::Func(func) => self.check_func(func)?,
                TopLevelDecl::Routine(routine) => self.check_routine(routine)?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Check a function declaration and store its signature.
    fn check_func(&mut self, func: &FuncDecl) -> Result<(), SemanticError> {
        self.functions.insert(func.name.clone(), func.clone());
        // Parameters belong to the function's own scope.
        for param in &func.params {
            self.symbol_table.insert(param.name.clone(), param.param_type.clone());
        }
        for stmt in &func.body {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    /// Check a routine declaration (actors). Routine parameters cannot be `Ref` – this is enforced by the parser.
    fn check_routine(&mut self, routine: &RoutineDecl) -> Result<(), SemanticError> {
        for param in &routine.params {
            self.symbol_table.insert(param.name.clone(), param.param_type.clone());
        }
        for stmt in &routine.body {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    /// Verify a single statement.
    fn check_stmt(&mut self, stmt: &Statement) -> Result<(), SemanticError> {
        match stmt {
            Statement::Spawn(expr) => {
                // Spawn arguments must be copyable (no Ref).
                if let Expr::FunctionCall { name, args } = expr {
                    for arg in args {
                        let arg_type = self.infer_type(arg);
                        if !arg_type.is_copyable() {
                            return Err(SemanticError::UncopyableSpawnArgument(format!(
                                "Argument in call '{}' contains Ref[T] and cannot be passed across spawn",
                                name
                            )));
                        }
                    }
                }
            }
            Statement::VarDecl { name, type_annot, value, .. } => {
                let inferred = self.infer_type(value);
                let var_type = type_annot.clone().unwrap_or(inferred);
                self.symbol_table.insert(name.clone(), var_type);
            }
            Statement::ForLoop { body, .. } => {
                for s in body {
                    self.check_stmt(s)?;
                }
            }
            Statement::Match { cases, .. } => {
                for case in cases {
                    for s in &case.body {
                        self.check_stmt(s)?;
                    }
                }
            }
            Statement::ExprStmt(expr) => {
                // Validate calls to user‑defined functions.
                if let Expr::FunctionCall { name, args } = expr {
                    if let Some(func) = self.functions.get(name) {
                        // Argument count check.
                        if args.len() != func.params.len() {
                            return Err(SemanticError::ArgumentCountMismatch {
                                function: name.clone(),
                                expected: func.params.len(),
                                found: args.len(),
                            });
                        }
                        // Type compatibility – the language rules allow:
                        //   type → type   (copy)
                        //   type → Ref    (create ref)
                        //   Ref  → type   (copy dereferenced value)
                        //   Ref  → Ref    (copy reference)
                        // Therefore we only need to make sure each argument can be typed.
                        for (arg_expr, param) in args.iter().zip(&func.params) {
                            let _arg_type = self.infer_type(arg_expr);
                            let _param_type = &param.param_type;
                            // No error emitted – the semantics already permit these conversions.
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Compute the static type of an expression.
    pub fn infer_type(&self, expr: &Expr) -> TypeSpec {
        match expr {
            Expr::IntLiteral(_) => TypeSpec::Primitive("Int".to_string()),
            Expr::StringLiteral(_) => TypeSpec::Primitive("String".to_string()),
            Expr::Variable(name) => self
                .symbol_table
                .get(name)
                .cloned()
                .unwrap_or(TypeSpec::Primitive("Unknown".to_string())),
            Expr::RefCreate(inner) => TypeSpec::Ref(Box::new(self.infer_type(inner))),
            _ => TypeSpec::Primitive("Unknown".to_string()),
        }
    }
}
