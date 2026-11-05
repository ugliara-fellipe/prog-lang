use std::collections::HashMap;
use crate::ast::*;
use crate::semantic::SemanticError;

pub struct TypeChecker {
    symbol_table: HashMap<String, TypeSpec>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            symbol_table: HashMap::new(),
        }
    }

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

    fn check_func(&mut self, func: &FuncDecl) -> Result<(), SemanticError> {
        for param in &func.params {
            self.symbol_table.insert(param.name.clone(), param.param_type.clone());
        }
        for stmt in &func.body {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    fn check_routine(&mut self, routine: &RoutineDecl) -> Result<(), SemanticError> {
        for param in &routine.params {
            self.symbol_table.insert(param.name.clone(), param.param_type.clone());
        }
        for stmt in &routine.body {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    fn check_stmt(&mut self, stmt: &Statement) -> Result<(), SemanticError> {
        match stmt {
            Statement::Spawn(expr) => {
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
            _ => {}
        }
        Ok(())
    }

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
