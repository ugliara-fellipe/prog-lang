use std::collections::HashSet;
use crate::ast::*;
use crate::semantic::SemanticError;

pub struct TerminationChecker;

impl TerminationChecker {
    pub fn verify_program(program: &Program) -> Result<(), SemanticError> {
        for decl in &program.declarations {
            if let TopLevelDecl::Func(func) = decl {
                Self::verify_func_termination(func)?;
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

    fn check_stmt_termination(
        stmt: &Statement,
        func_name: &str,
        sub_components: &HashSet<String>,
    ) -> Result<(), SemanticError> {
        match stmt {
            Statement::Match { cases, .. } => {
                for case in cases {
                    let mut local_subs = sub_components.clone();
                    Self::extract_pattern_bindings(&case.pattern, &mut local_subs);
                    for s in &case.body {
                        Self::check_stmt_termination(s, func_name, &local_subs)?;
                    }
                }
            }
            Statement::ExprStmt(expr) => {
                Self::check_expr_termination(expr, func_name, sub_components)?;
            }
            Statement::VarDecl { value, .. } => {
                Self::check_expr_termination(value, func_name, sub_components)?;
            }
            Statement::ForLoop { body, .. } => {
                for s in body {
                    Self::check_stmt_termination(s, func_name, sub_components)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

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

    fn check_expr_termination(
        expr: &Expr,
        func_name: &str,
        sub_components: &HashSet<String>,
    ) -> Result<(), SemanticError> {
        match expr {
            Expr::FunctionCall { name, args } => {
                if name == func_name {
                    // If direct recursive call, verify that argument is a subcomponent extracted by match
                    for arg in args {
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
