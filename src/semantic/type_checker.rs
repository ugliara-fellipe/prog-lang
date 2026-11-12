use std::collections::{HashMap, HashSet};
use crate::ast::*;
use crate::semantic::SemanticError;

pub struct TypeChecker {
    /// Symbol table for the current scope (functions and variables).
    symbol_table: HashMap<String, TypeSpec>,
    /// Collected function signatures for call‑argument validation.
    functions: HashMap<String, FuncDecl>,
    /// Loop declarations known to this program; signals are addressed only to
    /// names in this registry.
    loops: HashSet<String>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            symbol_table: HashMap::new(),
            functions: HashMap::new(),
            loops: HashSet::new(),
        }
    }

    /// Entry point for the whole program.
    pub fn check_program(&mut self, program: &Program) -> Result<(), SemanticError> {
        // Collect first so a function may signal a loop declared later in the
        // file, while rejecting duplicate names before runtime startup.
        for decl in &program.declarations {
            if let TopLevelDecl::Loop(loop_decl) = decl {
                if !self.loops.insert(loop_decl.name.clone()) {
                    return Err(SemanticError::DuplicateLoopName(loop_decl.name.clone()));
                }
            }
        }
        for decl in &program.declarations {
            match decl {
                TopLevelDecl::Func(func) => self.check_func(func)?,
                TopLevelDecl::Routine(routine) => self.check_routine(routine)?,
                TopLevelDecl::Loop(loop_decl) => self.check_loop(loop_decl)?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Check a function declaration and store its signature.
    fn check_func(&mut self, func: &FuncDecl) -> Result<(), SemanticError> {
        Self::validate_non_loop_body(&func.body)?;
        self.functions.insert(func.name.clone(), func.clone());
        // Parameters belong to the function's own scope.
        for param in &func.params {
            self.symbol_table.insert(param.name.clone(), param.param_type.clone());
        }
        for stmt in &func.body {
            self.check_stmt(stmt, true)?;
        }
        Ok(())
    }

    /// Check a routine declaration (actors). Routine parameters cannot be `Ref` – this is enforced by the parser.
    fn check_routine(&mut self, routine: &RoutineDecl) -> Result<(), SemanticError> {
        Self::validate_non_loop_body(&routine.body)?;
        for param in &routine.params {
            self.symbol_table.insert(param.name.clone(), param.param_type.clone());
        }
        for stmt in &routine.body {
            self.check_stmt(stmt, false)?;
        }
        Ok(())
    }

    fn check_loop(&mut self, loop_decl: &LoopDecl) -> Result<(), SemanticError> {
        for stmt in &loop_decl.body {
            self.check_stmt(stmt, false)?;
        }
        Ok(())
    }

    /// Functions and routines do not have a loop mailbox/context. This pass
    /// rejects loop-only control constructs before normal statement checks.
    fn validate_non_loop_body(statements: &[Statement]) -> Result<(), SemanticError> {
        for statement in statements {
            match statement {
                Statement::Terminate => return Err(SemanticError::TerminateOutsideLoop),
                Statement::ExprStmt(expr) | Statement::Spawn(expr) => Self::reject_receive(expr)?,
                Statement::Signal { payload, .. }
                | Statement::VarDecl { value: payload, .. }
                | Statement::Throw(payload) => Self::reject_receive(payload)?,
                Statement::ForLoop { collection, body, .. } => {
                    Self::reject_receive(collection)?;
                    Self::validate_non_loop_body(body)?;
                }
                Statement::Match { target, cases } => {
                    Self::reject_receive(target)?;
                    for case in cases { Self::validate_non_loop_body(&case.body)?; }
                }
                Statement::TryCatch { try_body, catch_body, .. } => {
                    Self::validate_non_loop_body(try_body)?;
                    Self::validate_non_loop_body(catch_body)?;
                }
            }
        }
        Ok(())
    }

    fn reject_receive(expr: &Expr) -> Result<(), SemanticError> {
        match expr {
            Expr::ReceiveSignal => Err(SemanticError::ReceiveOutsideLoop),
            Expr::FunctionCall { args, .. } => {
                for arg in args { Self::reject_receive(arg)?; }
                Ok(())
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::reject_receive(left)?;
                Self::reject_receive(right)
            }
            Expr::MemberAccess { target, .. } | Expr::RefCreate(target) => Self::reject_receive(target),
            _ => Ok(()),
        }
    }

    /// Validates a statement in its declaration context. `allows_throw` is
    /// true exclusively for function bodies; `try/catch` itself is valid in
    /// every context, but a direct `throw` is not valid in routines or loops.
    fn check_stmt(&mut self, stmt: &Statement, allows_throw: bool) -> Result<(), SemanticError> {
        match stmt {
            Statement::Spawn(expr) => {
                // Spawn arguments must be copyable (no Ref).
                if let Expr::FunctionCall { name, args } = expr {
                    if self.loops.contains(name) && args.len() > 1 {
                        return Err(SemanticError::LoopSignalArgumentCount {
                            loop_name: name.clone(),
                            found: args.len(),
                        });
                    }
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
            Statement::Signal { loop_name, payload } => {
                if !self.loops.contains(loop_name) {
                    return Err(SemanticError::UnknownLoopSignal(loop_name.clone()));
                }
                let _payload_type = self.infer_type(payload);
            }
            Statement::VarDecl { name, type_annot, value, .. } => {
                let inferred = self.infer_type(value);
                let var_type = type_annot.clone().unwrap_or(inferred);
                self.symbol_table.insert(name.clone(), var_type);
            }
            Statement::ForLoop { body, .. } => {
                for s in body {
                    self.check_stmt(s, allows_throw)?;
                }
            }
            Statement::Match { cases, .. } => {
                for case in cases {
                    for s in &case.body {
                        self.check_stmt(s, allows_throw)?;
                    }
                }
            }
            Statement::Throw(value) => {
                if !allows_throw {
                    return Err(SemanticError::ThrowOutsideFunction);
                }
                let _exception_type = self.infer_type(value);
            }
            Statement::TryCatch { try_body, error_name, catch_body } => {
                for statement in try_body {
                    self.check_stmt(statement, allows_throw)?;
                }
                self.symbol_table.insert(error_name.clone(), TypeSpec::Primitive("Unknown".to_string()));
                for statement in catch_body {
                    self.check_stmt(statement, allows_throw)?;
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
            Statement::Terminate => {}
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
