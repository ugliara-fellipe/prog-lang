use std::collections::{HashMap, HashSet};
use crate::ast::*;
use crate::semantic::SemanticError;
use crate::semantic::generics::{self, Substitution};

pub struct TypeChecker {
    /// Symbol table for the current scope (functions and variables).
    symbol_table: HashMap<String, TypeSpec>,
    /// Collected function signatures for call‑argument validation.
    functions: HashMap<String, FuncDecl>,
    routines: HashMap<String, RoutineDecl>,
    types: HashMap<String, TypeDecl>,
    standard_signatures: bool,
    type_variables: HashSet<String>,
    /// Loop declarations known to this program; signals are addressed only to
    /// names in this registry.
    loops: HashSet<String>,
}

impl TypeChecker {
    /// Validate a call once the target and argument types are known.
    /// Unknown types are rejected rather than treated as compatible.
    pub fn check_function_signature(target: &TypeSpec, arguments: &[TypeSpec]) -> Result<TypeSpec, SemanticError> {
        let TypeSpec::Function(parameters, result) = target else {
            return Err(SemanticError::NotAFunction(target.clone()));
        };
        if parameters.len() != arguments.len() {
            return Err(SemanticError::ArgumentCountMismatch {
                function: "function value".into(), expected: parameters.len(), found: arguments.len(),
            });
        }
        for (index, (expected, actual)) in parameters.iter().zip(arguments).enumerate() {
            if expected != actual || Self::contains_unknown_type(expected) || Self::contains_unknown_type(actual) {
                return Err(SemanticError::FunctionArgumentTypeMismatch { index: index + 1, expected: expected.clone(), actual: actual.clone() });
            }
        }
        Ok((**result).clone())
    }

    fn contains_unknown_type(spec: &TypeSpec) -> bool {
        match spec {
            TypeSpec::Primitive(name) => name == "Unknown",
            TypeSpec::Ref(inner) => Self::contains_unknown_type(inner),
            TypeSpec::Generic(_, args) => args.iter().any(Self::contains_unknown_type),
            TypeSpec::Function(args, result) => args.iter().any(Self::contains_unknown_type) || Self::contains_unknown_type(result),
        }
    }
    pub fn new() -> Self {
        Self {
            symbol_table: HashMap::new(),
            functions: HashMap::new(),
            routines: HashMap::new(),
            types: HashMap::new(),
            standard_signatures: false,
            type_variables: HashSet::new(),
            loops: HashSet::new(),
        }
    }

    /// Entry point for the whole program.
    pub fn check_program(&mut self, program: &Program) -> Result<(), SemanticError> {
        self.symbol_table.clear();
        self.functions.clear();
        self.routines.clear();
        self.types.clear();
        self.loops.clear();
        self.standard_signatures = crate::stdlib::validate_standard_library(program).is_ok();
        // Collect first so a function may signal a loop declared later in the
        // file, while rejecting duplicate names before runtime startup.
        for decl in &program.declarations {
            if let TopLevelDecl::Type(ty) = decl { self.types.insert(ty.name.clone(), ty.clone()); }
            if let TopLevelDecl::Func(func) = decl {
                self.functions.insert(func.name.clone(), func.clone());
            }
            if let TopLevelDecl::Routine(routine) = decl {
                self.routines.insert(routine.name.clone(), routine.clone());
            }
            if let TopLevelDecl::Loop(loop_decl) = decl {
                if !self.loops.insert(loop_decl.name.clone()) {
                    return Err(SemanticError::DuplicateLoopName(loop_decl.name.clone()));
                }
            }
        }
        for decl in &program.declarations {
            self.symbol_table.clear();
            self.type_variables.clear();
            match decl {
                TopLevelDecl::Func(func) => self.check_func(func)?,
                TopLevelDecl::Routine(routine) => self.check_routine(routine)?,
                TopLevelDecl::Loop(loop_decl) => self.check_loop(loop_decl)?,
                TopLevelDecl::Type(ty) => { self.bind_type_parameters(&ty.type_params)?; for field in &ty.fields { self.validate_type(&field.field_type)?; } },
                _ => {}
            }
        }
        Ok(())
    }

    /// Check a function declaration and store its signature.
    fn check_func(&mut self, func: &FuncDecl) -> Result<(), SemanticError> {
        self.bind_type_parameters(&func.type_params)?;
        for param in &func.params { self.validate_type(&param.param_type)?; }
        if let Some(result)=&func.return_type { self.validate_type(result)?; }
        Self::validate_non_loop_body(&func.body)?;
        let saved = self.symbol_table.clone();
        self.symbol_table.clear();
        for param in &func.params { self.symbol_table.insert(param.name.clone(), param.param_type.clone()); }
        let intrinsic = func.body.is_empty() && (crate::stdlib::is_total_standard_function(&func.name)
            || (self.standard_signatures && crate::stdlib::is_total_standard_function(&format!("std.{}", func.name))));
        let check = self.check_function_value_body(&func.body, if intrinsic { None } else { func.return_type.as_ref() });
        self.symbol_table = saved;
        check?;
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
        self.bind_type_parameters(&routine.type_params)?;
        for param in &routine.params { self.validate_type(&param.param_type)?; }
        Self::validate_non_loop_body(&routine.body)?;
        let saved = self.symbol_table.clone();
        self.symbol_table.clear();
        for param in &routine.params { self.symbol_table.insert(param.name.clone(), param.param_type.clone()); }
        let check = self.check_function_value_body(&routine.body, None);
        self.symbol_table = saved;
        check?;
        for param in &routine.params {
            self.symbol_table.insert(param.name.clone(), param.param_type.clone());
        }
        for stmt in &routine.body {
            self.check_stmt(stmt, false)?;
        }
        Ok(())
    }

    fn check_loop(&mut self, loop_decl: &LoopDecl) -> Result<(), SemanticError> {
        let saved = self.symbol_table.clone();
        self.symbol_table.clear();
        let check = self.check_function_value_body(&loop_decl.body, None);
        self.symbol_table = saved;
        check?;
        for stmt in &loop_decl.body {
            self.check_stmt(stmt, false)?;
        }
        Ok(())
    }

    fn contains_function_type(spec: &TypeSpec) -> bool {
        match spec {
            TypeSpec::Function(_, _) => true,
            TypeSpec::Ref(inner) => Self::contains_function_type(inner),
            TypeSpec::Generic(_, args) => args.iter().any(Self::contains_function_type),
            TypeSpec::Primitive(_) => false,
        }
    }

    fn compatible(expected: &TypeSpec, actual: &TypeSpec) -> bool {
        if Self::contains_function_type(expected) || Self::contains_function_type(actual) { return expected == actual; }
        match (expected, actual) {
            (TypeSpec::Ref(inner), other) | (other, TypeSpec::Ref(inner)) => Self::compatible(inner, other),
            _ => expected == actual,
        }
    }

    fn exhaustive(cases: &[MatchCase], ty: &TypeSpec) -> bool {
        let any = |p: &Pattern| matches!(p, Pattern::Wildcard | Pattern::Identifier(_));
        if cases.iter().any(|c| any(&c.pattern)) { return true; }
        let ctor = |name: &str, arity| cases.iter().any(|c| matches!(&c.pattern, Pattern::Constructor(n, args) if n == name && args.len() == arity && args.iter().all(&any)));
        match ty {
            TypeSpec::Primitive(name) if name == "Bool" => ctor("True", 0) && ctor("False", 0),
            TypeSpec::Generic(name, _) if name == "List" => ctor("Nil", 0) && ctor("Cons", 2),
            _ => false,
        }
    }

    fn copyable(&self, ty: &TypeSpec, seen: &mut HashSet<String>) -> bool {
        match ty {
            TypeSpec::Ref(_) => false,
            TypeSpec::Function(_, _) => true,
            TypeSpec::Generic(name, args) => {
                if name=="Ref" || !args.iter().all(|arg| self.copyable(arg,seen)) {return false;}
                match self.types.get(name) {
                    Some(decl) if seen.insert(name.clone()) => {
                        let substitution:Substitution=decl.type_params.iter().cloned().zip(args.iter().cloned()).collect();
                        let result=decl.fields.iter().all(|field| self.copyable(&generics::substitute(&field.field_type,&substitution),seen));
                        seen.remove(name); result
                    }
                    _=>true
                }
            },
            TypeSpec::Primitive(name) if name == "Unknown" => false,
            TypeSpec::Primitive(name) => match self.types.get(name) {
                Some(decl) if seen.insert(name.clone()) => {
                    let result = decl.fields.iter().all(|field| self.copyable(&field.field_type, seen));
                    seen.remove(name);
                    result
                }
                _ => true,
            },
        }
    }

    fn expression_copyable(&self, expression: &Expr) -> bool {
        match expression {
            Expr::ListLiteral(items) => items.iter().all(|item| self.expression_copyable(item)),
            _ => self.copyable(&self.infer_type(expression), &mut HashSet::new()),
        }
    }

    fn check_argument_types(&self, name: &str, parameters: &[TypeSpec], args: &[Expr]) -> Result<(), SemanticError> {
        if parameters.len() != args.len() {
            return Err(SemanticError::ArgumentCountMismatch { function: name.into(), expected: parameters.len(), found: args.len() });
        }
        for (index, (expected, arg)) in parameters.iter().zip(args).enumerate() {
            let actual = self.infer_type(arg);
            if !self.expression_compatible(expected, arg) {
                return Err(SemanticError::FunctionArgumentTypeMismatch { index: index + 1, expected: expected.clone(), actual });
            }
        }
        Ok(())
    }

    fn expression_compatible(&self, expected: &TypeSpec, expr: &Expr) -> bool {
        if let (TypeSpec::Generic(name, args), Expr::ListLiteral(items)) = (expected, expr) {
            if name == "List" && args.len() == 1 { return items.iter().all(|item| self.expression_compatible(&args[0], item)); }
        }
        let actual = self.check_expr(expr,Some(expected)).unwrap_or(TypeSpec::Primitive("Unknown".into()));
        Self::compatible(expected, &actual) && !Self::contains_unknown_type(&actual)
    }

    fn never_completes(&self, body: &[Statement]) -> bool {
        body.iter().any(|statement| match statement {
            Statement::Throw(_) => true,
            Statement::Match { target, cases } => Self::exhaustive(cases, &self.infer_type(target))
                && cases.iter().all(|case| self.never_completes(&case.body)),
            Statement::TryCatch { try_body, catch_body, .. } => self.never_completes(try_body) && self.never_completes(catch_body),
            _ => false,
        })
    }

    fn bind_pattern_types(&mut self, pattern: &Pattern, ty: &TypeSpec) -> Result<(), SemanticError> {
        match (pattern, ty) {
            (Pattern::Identifier(name), _) => { self.symbol_table.insert(name.clone(), ty.clone()); }
            (Pattern::Constructor(name, parts), TypeSpec::Generic(list, args))
                if name == "Cons" && list == "List" && args.len() == 1 && parts.len() == 2 => {
                self.bind_pattern_types(&parts[0], &args[0])?;
                self.bind_pattern_types(&parts[1], ty)?;
            }
            (Pattern::Wildcard, _) => {}
            (Pattern::Constructor(name, parts), TypeSpec::Generic(list, _)) if name == "Nil" && list == "List" && parts.is_empty() => {}
            (Pattern::Constructor(name, parts), TypeSpec::Primitive(kind)) if matches!(name.as_str(), "True" | "False") && kind == "Bool" && parts.is_empty() => {}
            (Pattern::Constructor(name, _), _) => return Err(SemanticError::InvalidPattern(format!("{} does not match {:?}", name, ty))),
        }
        Ok(())
    }

    fn bind_type_parameters(&mut self, parameters: &[String]) -> Result<(), SemanticError> {
        self.type_variables.clear();
        for parameter in parameters {
            if matches!(parameter.as_str(), "Int" | "Bool" | "String" | "Unknown" | "List" | "Ref") || self.types.contains_key(parameter) || !self.type_variables.insert(parameter.clone()) {
                return Err(SemanticError::GenericTypeError(format!("duplicate or reserved type parameter {}",parameter)));
            }
        }
        Ok(())
    }

    fn validate_type(&self, ty: &TypeSpec) -> Result<(), SemanticError> {
        match ty {
            TypeSpec::Primitive(name) if matches!(name.as_str(), "Int" | "String" | "Bool" | "Unit" | "Unknown") || self.type_variables.contains(name) => Ok(()),
            TypeSpec::Primitive(name) => match self.types.get(name) {
                Some(decl) if decl.type_params.is_empty() => Ok(()),
                Some(_) => Err(SemanticError::GenericTypeError(format!("type {} requires type arguments",name))),
                None => Err(SemanticError::GenericTypeError(format!("undeclared type {}",name))),
            },
            TypeSpec::Generic(name,args) => {
                let arity=if name=="List" {1} else {self.types.get(name).ok_or_else(|| SemanticError::GenericTypeError(format!("undeclared type {}",name)))?.type_params.len()};
                if args.len()!=arity { return Err(SemanticError::GenericTypeError(format!("type {} expects {} arguments, found {}",name,arity,args.len()))); }
                for arg in args {self.validate_type(arg)?;} Ok(())
            }
            TypeSpec::Ref(inner) => self.validate_type(inner),
            TypeSpec::Function(args,result) => {for arg in args {self.validate_type(arg)?;} self.validate_type(result)}
        }
    }

    fn function_scheme(&self, name: &str) -> Option<(Vec<String>,TypeSpec)> {
        let function=self.functions.get(name)?;
        if name=="std.empty" {return Some((vec!["T".into()],TypeSpec::Function(vec![],Box::new(TypeSpec::Generic("List".into(),vec![TypeSpec::Primitive("T".into())])))));}
        if name=="std.prepend" {let t=TypeSpec::Primitive("T".into());return Some((vec!["T".into()],TypeSpec::Function(vec![t.clone(),TypeSpec::Generic("List".into(),vec![t.clone()])],Box::new(TypeSpec::Generic("List".into(),vec![t])))));}
        Some((function.type_params.clone(),TypeSpec::Function(function.params.iter().map(|p|p.param_type.clone()).collect(),Box::new(function.return_type.clone().unwrap_or(TypeSpec::Primitive("Unknown".into()))))))
    }

    fn generic_target<'a>(&self, target: &'a Expr) -> Option<(Vec<String>,TypeSpec,Option<&'a [TypeSpec]>)> {
        if let Expr::Specialize {target,type_args}=target {
            let (parameters,signature,_) = self.generic_target(target)?;
            return Some((parameters,signature,Some(type_args.as_slice())));
        }
        let name=Self::qualified_name(target)?;
        if self.symbol_table.contains_key(&name) || self.qualified_name_shadowed(&name) { return None; }
        let (parameters,signature)=self.function_scheme(&name)?;
        Some((parameters,signature,None))
    }

    fn check_call(&self, signature: &TypeSpec, parameters: &[String], explicit: Option<&[TypeSpec]>, args: &[Expr], expected: Option<&TypeSpec>) -> Result<TypeSpec,SemanticError> {
        if let Some(types)=explicit {for ty in types {self.validate_type(ty)?;}}
        let signature=if parameters.is_empty() {
            if explicit.is_some() { return Err(SemanticError::GenericTypeError("function has no type parameters".into())); }
            signature.clone()
        } else {
            let actual:Vec<_>=args.iter().map(|arg| self.infer_type(arg)).collect();
            generics::instantiate(signature,parameters,explicit,&actual,expected)?
        };
        let TypeSpec::Function(inputs,result)=signature else {return Err(SemanticError::NotAFunction(signature));};
        if inputs.len()!=args.len() {return Err(SemanticError::ArgumentCountMismatch {function:"function".into(),expected:inputs.len(),found:args.len()});}
        for (index,(ty,arg)) in inputs.iter().zip(args).enumerate() {
            let actual=self.check_expr(arg,Some(ty))?;
            if !Self::compatible(ty,&actual) || Self::contains_unknown_type(&actual) {return Err(SemanticError::FunctionArgumentTypeMismatch {index:index+1,expected:ty.clone(),actual});}
        }
        Ok(*result)
    }

    fn check_expr(&self, expr: &Expr, expected: Option<&TypeSpec>) -> Result<TypeSpec,SemanticError> {
        match expr {
            Expr::FunctionCall {name,args} => {
                if self.qualified_name_shadowed(name) {return Err(SemanticError::UndeclaredSymbol(name.clone()));}
                if name.starts_with("std.") && !self.functions.contains_key(name) {return Err(SemanticError::MissingStandardLibraryImport(name.clone()));}
                if let Some(signature)=self.symbol_table.get(name) {return self.check_call(signature,&[],None,args,expected);}
                if name=="std.log" && self.functions.contains_key(name) || name=="log" && !self.functions.contains_key(name) {
                    for arg in args {self.check_expr(arg,None)?;} return Ok(TypeSpec::Primitive("Unknown".into()));
                }
                if let Some((parameters,signature))=self.function_scheme(name) {return self.check_call(&signature,&parameters,None,args,expected);}
                if self.routines.contains_key(name) || self.loops.contains(name) {return Err(SemanticError::InvalidExpression(format!("{} is an actor; use spawn",name)));}
                return Err(SemanticError::UndeclaredSymbol(name.clone()));
            }
            Expr::IndirectCall {target,args} => {
                if let Some((parameters,signature,explicit))=self.generic_target(target) {return self.check_call(&signature,&parameters,explicit,args,expected);}
                let signature=self.check_expr(target,None)?;
                return self.check_call(&signature,&[],None,args,expected);
            }
            Expr::Specialize {target,type_args} => {
                for ty in type_args {self.validate_type(ty)?;}
                let Some((parameters,signature,_))=self.generic_target(target) else {return Err(SemanticError::GenericTypeError("only named generic functions can be specialized".into()));};
                if parameters.is_empty() {return Err(SemanticError::GenericTypeError("function has no type parameters".into()));}
                return generics::specialize(&signature,&parameters,type_args);
            }
            Expr::Variable(_) | Expr::MemberAccess {..} => {
                if let Some((parameters,signature,_))=self.generic_target(expr) {
                    if !parameters.is_empty() {
                        return match expected {Some(ty)=>generics::instantiate_value(&signature,&parameters,ty),None=>Err(SemanticError::GenericTypeError("generic function values require explicit specialization or a function type annotation".into()))};
                    }
                }
                match expr {
                    Expr::Variable(name) if !matches!(name.as_str(),"True"|"False") && !self.symbol_table.contains_key(name) && !self.functions.contains_key(name) => return Err(SemanticError::UndeclaredSymbol(name.clone())),
                    Expr::MemberAccess {target,..} => {
                        if let Some(name)=Self::qualified_name(expr) {
                            if !self.qualified_name_shadowed(&name) && self.functions.contains_key(&name) {return Ok(self.infer_type(expr));}
                            if name.starts_with("std.") && !self.qualified_name_shadowed(&name) {return Err(SemanticError::MissingStandardLibraryImport(name));}
                        }
                        self.check_expr(target,None)?;
                    }
                    _=>{}
                }
            }
            Expr::BinaryOp {op,left,right} => {
                let a=self.check_expr(left,None)?; let b=self.check_expr(right,None)?;
                let int=TypeSpec::Primitive("Int".into());
                let valid=if matches!(op.as_str(),"=="|"!=") {Self::compatible(&a,&b)} else {Self::compatible(&int,&a)&&Self::compatible(&int,&b)};
                if !valid {return Err(SemanticError::InvalidExpression(format!("operator {} on {:?} and {:?}",op,a,b)));}
            }
            Expr::RefCreate(inner) => {let inner_expected=match expected {Some(TypeSpec::Ref(ty))=>Some(ty.as_ref()),_=>None};return Ok(TypeSpec::Ref(Box::new(self.check_expr(inner,inner_expected)?)));}
            Expr::ListLiteral(items) => {
                if let Some(TypeSpec::Generic(name,args))=expected {if name=="List" && args.len()==1 {
                    for (index,item) in items.iter().enumerate() {let actual=self.check_expr(item,Some(&args[0]))?;if !Self::compatible(&args[0],&actual) {return Err(SemanticError::FunctionArgumentTypeMismatch {index:index+1,expected:args[0].clone(),actual});}}
                    return Ok(expected.unwrap().clone());
                }}
                for item in items {self.check_expr(item,None)?;}
            }
            _=>{}
        }
        Ok(self.infer_type(expr))
    }

    fn check_function_value_expr(&self, expr: &Expr) -> Result<(), SemanticError> {
        self.check_expr(expr,None).map(|_| ())
    }

    fn check_function_value_body(&mut self, body: &[Statement], return_type: Option<&TypeSpec>) -> Result<(), SemanticError> {
        for (position, statement) in body.iter().enumerate() {
            let tail_type = if position + 1 == body.len() { return_type } else { None };
            match statement {
                Statement::VarDecl { name, type_annot, value, .. } => {
                    if let Some(ty)=type_annot { self.validate_type(ty)?; }
                    let actual = self.check_expr(value,type_annot.as_ref())?;
                    if let Some(expected) = type_annot {
                        if !self.expression_compatible(expected, value) {
                            return Err(SemanticError::FunctionReturnTypeMismatch { expected: expected.clone(), actual });
                        }
                    }
                    let ty = type_annot.clone().unwrap_or(actual);
                    self.symbol_table.insert(name.clone(), ty);
                }
                Statement::Spawn(expr) => {
                    if let Expr::FunctionCall { name, args } = expr {
                        if self.symbol_table.contains_key(name) || self.qualified_name_shadowed(name) {
                            return Err(SemanticError::InvalidExpression("spawn requires an unshadowed named routine or loop".into()));
                        }
                        for arg in args {
                            self.check_function_value_expr(arg)?;
                            if !self.expression_copyable(arg) {
                                return Err(SemanticError::UncopyableSpawnArgument(name.clone()));
                            }
                        }
                        if let Some(routine) = self.routines.get(name) {
                            let parameters: Vec<_> = routine.params.iter().map(|p| p.param_type.clone()).collect();
                            self.check_argument_types(name, &parameters, args)?;
                        } else if self.loops.contains(name) && args.len() > 1 {
                            return Err(SemanticError::LoopSignalArgumentCount { loop_name: name.clone(), found: args.len() });
                        } else if !self.loops.contains(name) {
                            return Err(SemanticError::UndeclaredSymbol(format!("spawn target {} is not a routine or loop", name)));
                        }
                    } else {
                        return Err(SemanticError::InvalidExpression("spawn requires a named routine or loop call".into()));
                    }
                }
                Statement::ExprStmt(expr) => { self.check_expr(expr,tail_type)?; }
                Statement::Throw(expr) | Statement::MemorySet { value: expr, .. } | Statement::Signal { payload: expr, .. } => self.check_function_value_expr(expr)?,
                Statement::ForLoop { var_name, collection, body } => {
                    self.check_function_value_expr(collection)?;
                    let scope = self.symbol_table.clone();
                    let collection_type = self.infer_type(collection);
                    let TypeSpec::Generic(name, args) = &collection_type else { return Err(SemanticError::InvalidExpression(format!("for requires a list, found {:?}", collection_type))); };
                    if name != "List" || args.len() != 1 { return Err(SemanticError::InvalidExpression(format!("for requires a list, found {:?}", collection_type))); }
                    self.symbol_table.insert(var_name.clone(), args[0].clone());
                    let result = self.check_function_value_body(body, None);
                    self.symbol_table = scope;
                    result?;
                }
                Statement::Match { target, cases } => {
                    self.check_function_value_expr(target)?;
                    let scope = self.symbol_table.clone();
                    let target_type = self.infer_type(target);
                    for case in cases {
                        self.symbol_table = scope.clone();
                        self.bind_pattern_types(&case.pattern, &target_type)?;
                        let result = self.check_function_value_body(&case.body, tail_type);
                        self.symbol_table = scope.clone();
                        result?;
                    }
                }
                Statement::TryCatch { try_body, error_name, catch_body } => {
                    let scope = self.symbol_table.clone();
                    let result = self.check_function_value_body(try_body, tail_type);
                    self.symbol_table = scope.clone();
                    result?;
                    self.symbol_table.insert(error_name.clone(), TypeSpec::Primitive("Unknown".into()));
                    let result = self.check_function_value_body(catch_body, tail_type);
                    self.symbol_table = scope;
                    result?;
                }
                Statement::Terminate => {}
            }
        }
        if let Some(expected) = return_type {
            if self.never_completes(body) { return Ok(()); }
            match body.last() {
                Some(Statement::ExprStmt(expr)) => {
                    let actual = self.infer_type(expr);
                    if !self.expression_compatible(expected, expr) {
                        return Err(SemanticError::FunctionReturnTypeMismatch { expected: expected.clone(), actual });
                    }
                }
                Some(Statement::Match { target, cases }) if !cases.is_empty() => {
                    if !Self::exhaustive(cases, &self.infer_type(target)) {
                        return Err(SemanticError::MissingFunctionReturn);
                    }
                    let scope = self.symbol_table.clone();
                    let target_type = self.infer_type(target);
                    for case in cases {
                        self.symbol_table = scope.clone();
                        self.bind_pattern_types(&case.pattern, &target_type)?;
                        let result = self.check_function_value_body(&case.body, Some(expected));
                        if let Err(error) = result {
                            self.symbol_table = scope;
                            return Err(error);
                        }
                    }
                    self.symbol_table = scope;
                }
                Some(Statement::TryCatch { try_body, error_name, catch_body }) => {
                    let scope = self.symbol_table.clone();
                    let result = self.check_function_value_body(try_body, Some(expected));
                    self.symbol_table = scope.clone();
                    result?;
                    self.symbol_table.insert(error_name.clone(), TypeSpec::Primitive("Unknown".into()));
                    let result = self.check_function_value_body(catch_body, Some(expected));
                    self.symbol_table = scope;
                    result?;
                }
                Some(Statement::Throw(_)) => {}
                _ => return Err(SemanticError::MissingFunctionReturn),
            }
        }
        Ok(())
    }

    /// Functions and routines do not have a loop mailbox/context. This pass
    /// rejects loop-only control constructs before normal statement checks.
    fn validate_non_loop_body(statements: &[Statement]) -> Result<(), SemanticError> {
        for statement in statements {
            match statement {
                Statement::MemorySet { .. } => return Err(SemanticError::MemoryOutsideLoop),
                Statement::Terminate => return Err(SemanticError::TerminateOutsideLoop),
                Statement::ExprStmt(expr) | Statement::Spawn(expr) => { Self::reject_receive(expr)?; Self::reject_memory(expr)?; },
                Statement::Signal { payload, .. }
                | Statement::VarDecl { value: payload, .. }
                | Statement::Throw(payload) => { Self::reject_receive(payload)?; Self::reject_memory(payload)?; },
                Statement::ForLoop { collection, body, .. } => {
                    Self::reject_receive(collection)?;
                    Self::reject_memory(collection)?;
                    Self::validate_non_loop_body(body)?;
                }
                Statement::Match { target, cases } => {
                    Self::reject_receive(target)?;
                    Self::reject_memory(target)?;
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
            Expr::IndirectCall { target, args } => {
                Self::reject_receive(target)?;
                for arg in args { Self::reject_receive(arg)?; }
                Ok(())
            }
            Expr::ListLiteral(items) => {
                for item in items { Self::reject_receive(item)?; }
                Ok(())
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::reject_receive(left)?;
                Self::reject_receive(right)
            }
            Expr::MemberAccess { target, .. } | Expr::RefCreate(target) | Expr::Specialize { target, .. } => Self::reject_receive(target),
            _ => Ok(()),
        }
    }

    fn reject_memory(expr: &Expr) -> Result<(), SemanticError> {
        match expr {
            Expr::MemoryGet(_) => Err(SemanticError::MemoryOutsideLoop),
            Expr::FunctionCall { args, .. } => { for arg in args { Self::reject_memory(arg)?; } Ok(()) }
            Expr::IndirectCall { target, args } => {
                Self::reject_memory(target)?;
                for arg in args { Self::reject_memory(arg)?; }
                Ok(())
            }
            Expr::BinaryOp { left, right, .. } => { Self::reject_memory(left)?; Self::reject_memory(right) }
            Expr::MemberAccess { target, .. } | Expr::RefCreate(target) | Expr::Specialize { target, .. } => Self::reject_memory(target),
            Expr::ListLiteral(items) => { for item in items { Self::reject_memory(item)?; } Ok(()) }
            _ => Ok(()),
        }
    }

    /// Validates a statement in its declaration context. `allows_throw` is
    /// true exclusively for function bodies; `try/catch` itself is valid in
    /// every context, but a direct `throw` is not valid in routines or loops.
    fn check_stmt(&mut self, stmt: &Statement, allows_throw: bool) -> Result<(), SemanticError> {
        Self::validate_standard_library_calls(stmt, &self.functions)?;
        match stmt {
            Statement::MemorySet { value, .. } => { Self::reject_receive(value)?; },
            Statement::Spawn(_) => {
                if allows_throw { return Err(SemanticError::SpawnInsideFunction); }
                // Targets, argument types and copyability were checked in the
                // scoped expression pass above.
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
                let saved = self.symbol_table.clone();
                for s in body {
                    if let Err(error) = self.check_stmt(s, allows_throw) {
                        self.symbol_table = saved;
                        return Err(error);
                    }
                }
                self.symbol_table = saved;
            }
            Statement::Match { cases, .. } => {
                let saved = self.symbol_table.clone();
                for case in cases {
                    self.symbol_table = saved.clone();
                    for s in &case.body {
                        if let Err(error) = self.check_stmt(s, allows_throw) {
                            self.symbol_table = saved;
                            return Err(error);
                        }
                    }
                }
                self.symbol_table = saved;
            }
            Statement::Throw(value) => {
                if !allows_throw {
                    return Err(SemanticError::ThrowOutsideFunction);
                }
                let _exception_type = self.infer_type(value);
            }
            Statement::TryCatch { try_body, error_name, catch_body } => {
                let saved = self.symbol_table.clone();
                for statement in try_body {
                    if let Err(error) = self.check_stmt(statement, allows_throw) {
                        self.symbol_table = saved;
                        return Err(error);
                    }
                }
                self.symbol_table = saved.clone();
                self.symbol_table.insert(error_name.clone(), TypeSpec::Primitive("Unknown".to_string()));
                for statement in catch_body {
                    if let Err(error) = self.check_stmt(statement, allows_throw) {
                        self.symbol_table = saved;
                        return Err(error);
                    }
                }
                self.symbol_table = saved;
            }
            Statement::ExprStmt(_) => {}
            Statement::Terminate => {}
        }
        Ok(())
    }

    /// Runtime-backed operations live in ordinary packages.  A qualified
    /// `std.*` call is legal only when package resolution has added its
    /// declaration to the program, which requires `import std` in source.
    fn validate_standard_library_calls(
        statement: &Statement,
        functions: &HashMap<String, FuncDecl>,
    ) -> Result<(), SemanticError> {
        fn expression_is_imported(
            expression: &Expr,
            functions: &HashMap<String, FuncDecl>,
        ) -> Result<(), SemanticError> {
            match expression {
                Expr::IndirectCall { target, args } => {
                    expression_is_imported(target, functions)?;
                    for arg in args { expression_is_imported(arg, functions)?; }
                }
                Expr::Variable(name) if name.starts_with("std.") && !functions.contains_key(name) => return Err(SemanticError::MissingStandardLibraryImport(name.clone())),
                Expr::FunctionCall { name, args } => {
                    if name.starts_with("std.")
                        && (!crate::stdlib::is_total_standard_function(name)
                            || !functions.contains_key(name))
                    {
                        return Err(SemanticError::MissingStandardLibraryImport(name.clone()));
                    }
                    for argument in args {
                        expression_is_imported(argument, functions)?;
                    }
                }
                Expr::BinaryOp { left, right, .. } => {
                    expression_is_imported(left, functions)?;
                    expression_is_imported(right, functions)?;
                }
                Expr::MemberAccess { target, .. } => {
                    if let Some(name) = TypeChecker::qualified_name(expression) {
                        if name.starts_with("std.") && !functions.contains_key(&name) {
                            return Err(SemanticError::MissingStandardLibraryImport(name));
                        }
                    }
                    expression_is_imported(target, functions)?;
                }
                Expr::RefCreate(target) | Expr::Specialize {target,..} => {
                    expression_is_imported(target, functions)?;
                }
                Expr::ListLiteral(items) => {
                    for item in items {
                        expression_is_imported(item, functions)?;
                    }
                }
                Expr::IntLiteral(_) | Expr::StringLiteral(_) | Expr::Variable(_) | Expr::TypeOf(_) | Expr::ReceiveSignal | Expr::MemoryGet(_) => {}
            }
            Ok(())
        }

        match statement {
            Statement::MemorySet { value, .. } => expression_is_imported(value, functions),
            Statement::ExprStmt(expression) | Statement::Spawn(expression) | Statement::Throw(expression) => {
                expression_is_imported(expression, functions)
            }
            Statement::Signal { payload, .. } | Statement::VarDecl { value: payload, .. } => {
                expression_is_imported(payload, functions)
            }
            Statement::ForLoop { collection, body, .. } => {
                expression_is_imported(collection, functions)?;
                for statement in body {
                    Self::validate_standard_library_calls(statement, functions)?;
                }
                Ok(())
            }
            Statement::Match { target, cases } => {
                expression_is_imported(target, functions)?;
                for case in cases {
                    for statement in &case.body {
                        Self::validate_standard_library_calls(statement, functions)?;
                    }
                }
                Ok(())
            }
            Statement::TryCatch { try_body, catch_body, .. } => {
                for statement in try_body.iter().chain(catch_body) {
                    Self::validate_standard_library_calls(statement, functions)?;
                }
                Ok(())
            }
            Statement::Terminate => Ok(()),
        }
    }

    /// Compute the static type of an expression.
    pub fn infer_type(&self, expr: &Expr) -> TypeSpec {
        match expr {
            Expr::IntLiteral(_) => TypeSpec::Primitive("Int".to_string()),
            Expr::StringLiteral(_) => TypeSpec::Primitive("String".to_string()),
            Expr::Variable(name) if name == "True" || name == "False" => TypeSpec::Primitive("Bool".into()),
            Expr::BinaryOp { op, left, right } => {
                let left_type = self.infer_type(left);
                let right_type = self.infer_type(right);
                if left_type != right_type { TypeSpec::Primitive("Unknown".into()) }
                else if matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=") { TypeSpec::Primitive("Bool".into()) }
                else { left_type }
            }
            Expr::ListLiteral(items) if !items.is_empty() => {
                let element = self.infer_type(&items[0]);
                if items.iter().all(|item| self.infer_type(item) == element) {
                    TypeSpec::Generic("List".into(), vec![element])
                } else { TypeSpec::Primitive("Unknown".into()) }
            }
            Expr::ListLiteral(_) => TypeSpec::Generic("List".into(), vec![TypeSpec::Primitive("Unknown".into())]),
            Expr::TypeOf(_) => TypeSpec::Primitive("String".to_string()),
            Expr::Variable(name) => self
                .symbol_table
                .get(name)
                .cloned()
                .or_else(|| self.functions.get(name).filter(|_| !self.qualified_name_shadowed(name)).map(|func| TypeSpec::Function(
                    func.params.iter().map(|param| param.param_type.clone()).collect(),
                    Box::new(func.return_type.clone().unwrap_or_else(|| TypeSpec::Primitive("Unknown".into()))),
                )))
                .unwrap_or(TypeSpec::Primitive("Unknown".to_string())),
            Expr::FunctionCall { name, .. } if self.qualified_name_shadowed(name) => TypeSpec::Primitive("Unknown".into()),
            Expr::FunctionCall { name, .. } if !self.symbol_table.contains_key(name) && self.function_scheme(name).is_some_and(|(parameters,_)|!parameters.is_empty()) => self.check_expr(expr,None).unwrap_or(TypeSpec::Primitive("Unknown".into())),
            Expr::FunctionCall { name, .. } => match self.symbol_table.get(name) {
                Some(TypeSpec::Function(_, result)) => *result.clone(),
                Some(_) => TypeSpec::Primitive("Unknown".into()),
                None => self.functions.get(name).and_then(|func| func.return_type.clone()).unwrap_or_else(|| TypeSpec::Primitive("Unknown".into())),
            },
            Expr::Specialize {..} => self.check_expr(expr,None).unwrap_or(TypeSpec::Primitive("Unknown".into())),
            Expr::IndirectCall { target, .. } if self.generic_target(target).is_some() => self.check_expr(expr,None).unwrap_or(TypeSpec::Primitive("Unknown".into())),
            Expr::IndirectCall { target, .. } => match self.infer_type(target) {
                TypeSpec::Function(_, result) => *result,
                _ => TypeSpec::Primitive("Unknown".into()),
            },
            Expr::RefCreate(inner) => TypeSpec::Ref(Box::new(self.infer_type(inner))),
            Expr::MemberAccess { target, field } => {
                if let Some(name) = Self::qualified_name(expr) {
                    if let Some(func) = self.functions.get(&name).filter(|_| !self.qualified_name_shadowed(&name)) {
                        return TypeSpec::Function(func.params.iter().map(|p| p.param_type.clone()).collect(), Box::new(func.return_type.clone().unwrap_or(TypeSpec::Primitive("Unknown".into()))));
                    }
                }
                if matches!(target.as_ref(), Expr::ReceiveSignal) {
                    return TypeSpec::Primitive(if field == "pending" { "Int" } else { "String" }.into());
                }
                let mut ty = self.infer_type(target);
                if let TypeSpec::Ref(inner) = ty { ty = *inner; }
                if let TypeSpec::Primitive(name) = &ty {
                    if let Some(decl) = self.types.get(name) {
                        if let Some(member) = decl.fields.iter().find(|member| member.name == *field) { return member.field_type.clone(); }
                    }
                }
                if let TypeSpec::Generic(name,args)=ty {
                    if let Some(decl)=self.types.get(&name) {
                        if let Some(member)=decl.fields.iter().find(|member|member.name==*field) {
                            let substitutions=decl.type_params.iter().cloned().zip(args.iter().cloned()).collect();
                            return generics::substitute(&member.field_type,&substitutions);
                        }
                    }
                }
                TypeSpec::Primitive("Unknown".into())
            }
            _ => TypeSpec::Primitive("Unknown".to_string()),
        }
    }

    fn qualified_name(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Variable(name) => Some(name.clone()),
            Expr::MemberAccess { target, field } => Some(format!("{}.{}", Self::qualified_name(target)?, field)),
            _ => None,
        }
    }

    fn qualified_name_shadowed(&self, name: &str) -> bool {
        name.split_once('.').is_some_and(|(root, _)| self.symbol_table.contains_key(root))
    }
}
