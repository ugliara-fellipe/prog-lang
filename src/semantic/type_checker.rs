use std::collections::{HashMap, HashSet};
use crate::ast::*;
use crate::semantic::SemanticError;
use crate::semantic::generics::{self, Substitution};

#[derive(Clone)]
pub struct TypeChecker {
    /// Symbol table for the current scope (functions and variables).
    symbol_table: HashMap<String, TypeSpec>,
    /// Collected function signatures for call‑argument validation.
    functions: HashMap<String, FuncDecl>,
    routines: HashMap<String, RoutineDecl>,
    types: HashMap<String, TypeDecl>,
    constructors: crate::semantic::adt::Constructors,
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
            constructors: Default::default(),
            standard_signatures: false,
            type_variables: HashSet::new(),
            loops: HashSet::new(),
        }
    }

    /// Entry point for the whole program.
    pub fn check_program(&mut self, program: &Program) -> Result<(), SemanticError> {
        if let Some(program)=crate::semantic::builtins::augment(program)?{return self.check_program(&program);}
        if crate::semantic::constants::needs_lowering(program){return self.check_program(&crate::semantic::constants::lower_program(program)?);}
        crate::semantic::tail_calls::validate_program(program)?;
        self.symbol_table.clear();
        self.functions.clear();
        self.routines.clear();
        self.types.clear();
        self.constructors=crate::semantic::adt::Constructors::new(program);
        self.loops.clear();
        self.standard_signatures = crate::stdlib::validate_standard_library(program).is_ok();
        // Collect first so a function may signal a loop declared later in the
        // file, while rejecting duplicate names before runtime startup.
        for decl in &program.declarations {
            if let TopLevelDecl::Type(ty) = decl {
                if matches!(ty.name.as_str(),"Int"|"Bool"|"String"|"List"|"Dict"|"Json"|"Ref"|"Unknown"|"Unit") || self.types.insert(ty.name.clone(),ty.clone()).is_some() {
                    return Err(SemanticError::InvalidExpression(format!("duplicate or reserved type name {}",ty.name)));
                }
            }
            if let TopLevelDecl::Func(func) = decl {
                if self.functions.get(&func.name).is_some_and(|old|old.foreign.is_some()||func.foreign.is_some()){return Err(SemanticError::InvalidExpression(format!("duplicate foreign name {}",func.name)));}
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
        for name in self.constructors.entries.keys() {
            if self.functions.contains_key(name) || self.routines.contains_key(name) || self.loops.contains(name) {
                return Err(SemanticError::InvalidExpression(format!("constructor name {} collides with another declaration",name)));
            }
        }
        for decl in &program.declarations {
            self.symbol_table.clear();
            self.type_variables.clear();
            match decl {
                TopLevelDecl::Func(func) => self.check_func(func)?,
                TopLevelDecl::Routine(routine) => self.check_routine(routine)?,
                TopLevelDecl::Loop(loop_decl) => self.check_loop(loop_decl)?,
                TopLevelDecl::Type(ty) => { self.bind_type_parameters(&ty.type_params)?; for field in &ty.fields { self.validate_type(&field.field_type)?; } for variant in &ty.variants {for field in &variant.fields {self.validate_type(field)?;}} },
                _ => {}
            }
        }
        Ok(())
    }

    /// Check a function declaration and store its signature.
    fn check_func(&mut self, func: &FuncDecl) -> Result<(), SemanticError> {
        if func.foreign.is_some(){crate::ffi::validate(func).map_err(SemanticError::InvalidExpression)?;}
        self.bind_type_parameters(&func.type_params)?;
        for param in &func.params { self.validate_type(&param.param_type)?; }
        if let Some(result)=&func.return_type { self.validate_type(result)?; }
        Self::validate_non_loop_body(&func.body)?;
        let saved = self.symbol_table.clone();
        self.symbol_table.clear();
        for param in &func.params { self.symbol_table.insert(param.name.clone(), param.param_type.clone()); }
        let intrinsic = func.body.is_empty() && (func.foreign.is_some() || crate::stdlib::is_bundled_native_function(&func.name) || crate::stdlib::is_total_standard_function(&func.name)
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
        if routine.name == "main" && !routine.type_params.is_empty() { return Err(SemanticError::GenericTypeError("main cannot declare type parameters".into())); }
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

    fn compatible(&self, expected: &TypeSpec, actual: &TypeSpec) -> bool {
        let expected=self.constructors.normalize(expected); let actual=self.constructors.normalize(actual);
        let (expected,actual)=(&expected,&actual);
        if matches!(expected,TypeSpec::Primitive(n) if n=="Json") {return super::json_types::is_json_type(actual);}
        if Self::contains_function_type(expected) || Self::contains_function_type(actual) { return expected == actual; }
        match (expected, actual) {
            (TypeSpec::Ref(inner), other) | (other, TypeSpec::Ref(inner)) => self.compatible(inner, other),
            _ => expected == actual,
        }
    }

    fn variant_fields(&self, ty:&TypeSpec)->Option<Vec<(String,Vec<TypeSpec>)>> {
        let ty=self.constructors.normalize(ty);
        let (name,args)=match &ty {TypeSpec::Primitive(n)=>(n,vec![]),TypeSpec::Generic(n,a)=>(n,a.clone()),_=>return None};
        if name=="Bool" {return Some(vec![("True".into(),vec![]),("False".into(),vec![])]);}
        if name=="List" && args.len()==1 {return Some(vec![("Nil".into(),vec![]),("Cons".into(),vec![args[0].clone(),ty.clone()])]);}
        let decl=self.types.get(name)?;
        if decl.variants.is_empty() {return None;}
        let sub=decl.type_params.iter().cloned().zip(args).collect();
        Some(decl.variants.iter().map(|v|(format!("{}.{}",name,v.name),v.fields.iter().map(|f|self.constructors.normalize(&generics::substitute(f,&sub))).collect())).collect())
    }
    fn pattern_tag(&self, name:&str)->String {
        self.constructors.entries.get(name).map(|c|format!("{}.{}",c.owner,c.variant)).unwrap_or_else(||name.into())
    }
    fn exhaustive_matrix(&self, rows:&[Vec<Pattern>], types:&[TypeSpec])->bool {
        if types.is_empty() {return !rows.is_empty();}
        let wild=|p:&Pattern|matches!(p,Pattern::Wildcard|Pattern::Identifier(_));
        if rows.is_empty() {return false;}
        if rows.iter().all(|r|wild(&r[0])) {return self.exhaustive_matrix(&rows.iter().map(|r|r[1..].to_vec()).collect::<Vec<_>>(),&types[1..]);}
        if let Some(variants)=self.variant_fields(&types[0]) {
            variants.iter().all(|(tag,fields)| {
                let mut specialized=Vec::new();
                for row in rows {
                    let mut prefix=if wild(&row[0]) {vec![Pattern::Wildcard;fields.len()]} else if let Pattern::Constructor(name,parts)=&row[0] {if self.pattern_tag(name)!=*tag {continue;} parts.clone()} else {continue;};
                    prefix.extend_from_slice(&row[1..]); specialized.push(prefix);
                }
                let mut next=fields.clone();next.extend_from_slice(&types[1..]);
                self.exhaustive_matrix(&specialized,&next)
            })
        } else {self.exhaustive_matrix(&rows.iter().filter(|r|wild(&r[0])).map(|r|r[1..].to_vec()).collect::<Vec<_>>(),&types[1..])}
    }
    fn check_guard(&self,guard:Option<&Expr>)->Result<(),SemanticError>{
        if let Some(guard)=guard{
            let boolean=TypeSpec::Primitive("Bool".into());
            let actual=self.check_expr(guard,Some(&boolean))?;
            if actual!=boolean{return Err(SemanticError::InvalidPatternGuard(actual));}
        }
        Ok(())
    }

    fn exhaustive(&self, cases:&[MatchCase], ty:&TypeSpec)->bool {
        self.exhaustive_matrix(&cases.iter().filter(|c|c.guard.is_none()).map(|c|vec![c.pattern.clone()]).collect::<Vec<_>>(),&[ty.clone()])
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
                        let result=decl.fields.iter().map(|f|&f.field_type).chain(decl.variants.iter().flat_map(|v|v.fields.iter())).all(|field| self.copyable(&generics::substitute(field,&substitution),seen));
                        seen.remove(name); result
                    }
                    _=>true
                }
            },
            TypeSpec::Primitive(name) if name == "Unknown" => false,
            TypeSpec::Primitive(name) => match self.types.get(name) {
                Some(decl) if seen.insert(name.clone()) => {
                    let result = decl.fields.iter().map(|f|&f.field_type).chain(decl.variants.iter().flat_map(|v|v.fields.iter())).all(|field| self.copyable(field, seen));
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
            Expr::DictLiteral(items) => items.iter().all(|(_,v)|self.expression_copyable(v)),
            _ => self.copyable(&self.infer_type(expression), &mut HashSet::new()),
        }
    }

    fn expression_compatible(&self, expected: &TypeSpec, expr: &Expr) -> bool {
        if let (TypeSpec::Generic(name, args), Expr::ListLiteral(items)) = (expected, expr) {
            if name == "List" && args.len() == 1 { return items.iter().all(|item| self.expression_compatible(&args[0], item)); }
        }
        let actual = self.check_expr(expr,Some(expected)).unwrap_or(TypeSpec::Primitive("Unknown".into()));
        self.compatible(expected, &actual) && !Self::contains_unknown_type(&actual)
    }

    fn never_completes(&self, body: &[Statement]) -> bool {
        body.iter().any(|statement| match statement {
            Statement::Throw(_) => true,
            Statement::Match { target, cases } => self.exhaustive(cases, &self.infer_type(target))
                && cases.iter().all(|case| self.never_completes(&case.body)),
            Statement::TryCatch { try_body, catch_body, .. } => self.never_completes(try_body) && self.never_completes(catch_body),
            _ => false,
        })
    }

    fn validate_pattern_bindings(pattern:&Pattern,names:&mut HashSet<String>)->Result<(),SemanticError>{
        match pattern {
            Pattern::Identifier(n)=>{if !names.insert(n.clone()){return Err(SemanticError::InvalidPattern(format!("duplicate binding {}",n)));}},
            Pattern::Constructor(_,parts)=>for part in parts{Self::validate_pattern_bindings(part,names)?;},
            _=>{}
        }
        Ok(())
    }

    fn bind_pattern_types(&mut self, pattern: &Pattern, ty: &TypeSpec) -> Result<(), SemanticError> {
        if let Pattern::Constructor(name,parts)=pattern {
            if let Some(c)=self.constructors.entries.get(name).cloned() {
                let normalized=self.constructors.normalize(ty);
                let (owner,args)=match &normalized {TypeSpec::Primitive(n)=>(n,vec![]),TypeSpec::Generic(n,a)=>(n,a.clone()),_=>return Err(SemanticError::InvalidPattern(name.clone()))};
                if *owner!=c.owner || parts.len()!=c.fields.len() {return Err(SemanticError::InvalidPattern(format!("{} does not match {:?}",name,ty)));}
                let sub=c.type_params.iter().cloned().zip(args).collect();
                for (part,field) in parts.iter().zip(&c.fields) {self.bind_pattern_types(part,&generics::substitute(field,&sub))?;}
                return Ok(());
            }
        }
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
            if matches!(parameter.as_str(), "Int" | "Bool" | "String" | "Unknown" | "List" | "Dict" | "Json" | "Ref") || self.types.contains_key(parameter) || !self.type_variables.insert(parameter.clone()) {
                return Err(SemanticError::GenericTypeError(format!("duplicate or reserved type parameter {}",parameter)));
            }
        }
        Ok(())
    }

    fn validate_type(&self, ty: &TypeSpec) -> Result<(), SemanticError> {
        match ty {
            TypeSpec::Primitive(name) if matches!(name.as_str(), "Int" | "String" | "Bool" | "Json" | "Unit" | "Unknown") || self.type_variables.contains(name) => Ok(()),
            TypeSpec::Primitive(name) => match self.types.get(name) {
                Some(decl) if decl.type_params.is_empty() => Ok(()),
                Some(_) => Err(SemanticError::GenericTypeError(format!("type {} requires type arguments",name))),
                None => Err(SemanticError::GenericTypeError(format!("undeclared type {}",name))),
            },
            TypeSpec::Generic(name,args) => {
                let arity=if name=="List" || name=="Dict" {1} else {self.types.get(name).ok_or_else(|| SemanticError::GenericTypeError(format!("undeclared type {}",name)))?.type_params.len()};
                if args.len()!=arity { return Err(SemanticError::GenericTypeError(format!("type {} expects {} arguments, found {}",name,arity,args.len()))); }
                for arg in args {self.validate_type(arg)?;} Ok(())
            }
            TypeSpec::Ref(inner) => self.validate_type(inner),
            TypeSpec::Function(args,result) => {for arg in args {self.validate_type(arg)?;} self.validate_type(result)}
        }
    }

    fn function_scheme(&self, name: &str) -> Option<(Vec<String>,TypeSpec)> {
        if let Some(c)=self.constructors.entries.get(name) {return Some((c.type_params.clone(),c.signature()));}
        let function=self.functions.get(name)?;
        if name=="std.empty" {return Some((vec!["T".into()],TypeSpec::Function(vec![],Box::new(TypeSpec::Generic("List".into(),vec![TypeSpec::Primitive("T".into())])))));}
        if name=="std.prepend" {let t=TypeSpec::Primitive("T".into());return Some((vec!["T".into()],TypeSpec::Function(vec![t.clone(),TypeSpec::Generic("List".into(),vec![t.clone()])],Box::new(TypeSpec::Generic("List".into(),vec![t])))));}
        Some((function.type_params.clone(),self.constructors.normalize(&TypeSpec::Function(function.params.iter().map(|p|p.param_type.clone()).collect(),Box::new(function.return_type.clone().unwrap_or(TypeSpec::Primitive("Unknown".into())))))))
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
        let normalized=explicit.map(|types|types.iter().map(|ty|self.constructors.normalize(ty)).collect::<Vec<_>>());
        let explicit=normalized.as_deref();
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
            if !self.compatible(ty,&actual) || Self::contains_unknown_type(&actual) {return Err(SemanticError::FunctionArgumentTypeMismatch {index:index+1,expected:ty.clone(),actual});}
        }
        Ok(*result)
    }

    fn capture_has_open_type(&self,ty:&TypeSpec)->bool {
        match ty {TypeSpec::Primitive(n)=>self.type_variables.contains(n),TypeSpec::Generic(_,args)=>args.iter().any(|a|self.capture_has_open_type(a)),TypeSpec::Ref(t)=>self.capture_has_open_type(t),TypeSpec::Function(_,_)=>false}
    }
    fn check_closure(&self,params:&[Param],return_type:&TypeSpec,body:&[Statement])->Result<TypeSpec,SemanticError>{
        for p in params {self.validate_type(&p.param_type)?;}self.validate_type(return_type)?;
        Self::validate_non_loop_body(body)?;
        let mut checker=self.clone();checker.symbol_table.clear();
        for name in crate::semantic::closures::free_variables(params,body) {
            if let Some(ty)=self.symbol_table.get(&name) {
                if !self.copyable(ty,&mut HashSet::new()) || self.capture_has_open_type(ty) {
                    return Err(SemanticError::UncopyableCapture(format!("{}: {:?}",name,ty)));
                }
                checker.symbol_table.insert(name,ty.clone());
            }
        }
        for p in params {checker.symbol_table.insert(p.name.clone(),p.param_type.clone());}
        checker.check_function_value_body(body,Some(return_type))?;
        for stmt in body {checker.check_stmt(stmt,true)?;}
        Ok(TypeSpec::Function(params.iter().map(|p|p.param_type.clone()).collect(),Box::new(return_type.clone())))
    }

    fn check_expr(&self, expr: &Expr, expected: Option<&TypeSpec>) -> Result<TypeSpec,SemanticError> {
        let normalized=expected.map(|t|self.constructors.normalize(t));let expected=normalized.as_ref();
        if let Some(name)=Self::qualified_name(expr) {
            if !self.symbol_table.contains_key(&name) && !self.qualified_name_shadowed(&name) {
                if self.constructors.ambiguous.contains(&name) {return Err(SemanticError::InvalidExpression(format!("ambiguous constructor {}; qualify it with the enum name",name)));}
                if let Some(c)=self.constructors.entries.get(&name) {
                    if c.fields.is_empty() {return self.check_call(&c.signature(),&c.type_params,None,&[],expected);}
                    if c.type_params.is_empty() {return Ok(c.signature());}
                }
            }
        }
        match expr {
            Expr::Compose{left,right}=>{
                let a=self.check_expr(left,None)?;let b=self.check_expr(right,None)?;
                let (TypeSpec::Function(inputs,middle),TypeSpec::Function(next,result))=(&a,&b) else{return Err(SemanticError::InvalidExpression("composition requires two unary functions".into()));};
                if inputs.len()!=1 || next.len()!=1 || Self::contains_unknown_type(&a) || Self::contains_unknown_type(&b) || self.constructors.normalize(middle)!=self.constructors.normalize(&next[0]){
                    return Err(SemanticError::InvalidExpression(format!("incompatible composition: {:?} >>> {:?}",a,b)));
                }
                if !self.expression_copyable(left)||!self.expression_copyable(right){return Err(SemanticError::UncopyableCapture("composition".into()));}
                return Ok(TypeSpec::Function(inputs.clone(),result.clone()));
            },
            Expr::ConstValue{value,value_type}=>{crate::semantic::constants::validate_value(value,value_type)?;return Ok(value_type.clone());},
            Expr::Closure {params,return_type,body} => return self.check_closure(params,return_type,body),
            Expr::TailCall(call)=>return self.check_expr(call,expected),
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
                let specialized=generics::specialize(&signature,&parameters,type_args)?;
                if Self::qualified_name(target).and_then(|n|self.constructors.entries.get(&n)).is_some_and(|c|c.fields.is_empty()) {
                    if let TypeSpec::Function(_,result)=specialized {return Ok(*result);}
                }
                return Ok(specialized);
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
                        let receiver=self.infer_type(target);
                        if matches!(&receiver,TypeSpec::Generic(n,_) if n=="Dict") {return Err(SemanticError::InvalidExpression("dictionary lookup uses [String] or dict.get".into()));}
                        let enum_name=match &receiver {TypeSpec::Primitive(n)|TypeSpec::Generic(n,_)=>Some(n),_=>None};
                        if enum_name.and_then(|n|self.types.get(n)).is_some_and(|t|!t.variants.is_empty()) {
                            return Err(SemanticError::InvalidExpression("enum payloads must be accessed through match patterns".into()));
                        }
                    }
                    _=>{}
                }
            }
            Expr::BinaryOp {op,left,right} => {
                let a=self.check_expr(left,None)?; let b=self.check_expr(right,None)?;
                let int=TypeSpec::Primitive("Int".into());
                let valid=if matches!(op.as_str(),"=="|"!=") {self.compatible(&a,&b)} else {self.compatible(&int,&a)&&self.compatible(&int,&b)};
                if !valid {return Err(SemanticError::InvalidExpression(format!("operator {} on {:?} and {:?}",op,a,b)));}
            }
            Expr::RefCreate(inner) => {let inner_expected=match expected {Some(TypeSpec::Ref(ty))=>Some(ty.as_ref()),_=>None};return Ok(TypeSpec::Ref(Box::new(self.check_expr(inner,inner_expected)?)));}
            Expr::DictLiteral(items) => {
                let json=TypeSpec::Primitive("Json".into());
                let value_type=match expected {
                    Some(TypeSpec::Generic(n,a)) if n=="Dict" && a.len()==1=>Some(&a[0]),
                    Some(TypeSpec::Primitive(n)) if n=="Json"=>Some(&json),
                    _=>None
                };
                let mut types=Vec::new();
                for (_,value) in items {
                    let mut actual=self.check_expr(value,value_type)?;
                    if value_type.is_none() && Self::contains_unknown_type(&actual) && matches!(value,Expr::ListLiteral(_)|Expr::DictLiteral(_)){actual=self.check_expr(value,Some(&json))?;}
                    if let Some(ty)=value_type {if !self.compatible(ty,&actual){return Err(SemanticError::FunctionReturnTypeMismatch{expected:ty.clone(),actual});}}
                    types.push(actual);
                }
                let ty=if let Some(t)=value_type {t.clone()} else if types.is_empty(){json}
                    else if types.iter().all(|t|t==&types[0]) {types[0].clone()}
                    else if types.iter().all(super::json_types::is_json_type){json}
                    else {return Err(SemanticError::InvalidExpression("dictionary values require a common type".into()));};
                return Ok(TypeSpec::Generic("Dict".into(),vec![ty]));
            }
            Expr::Index{target,key}=>{
                let ty=self.check_expr(target,None)?;
                let key_ty=self.check_expr(key,Some(&TypeSpec::Primitive("String".into())))?;
                if key_ty!=TypeSpec::Primitive("String".into()){return Err(SemanticError::InvalidExpression("dictionary keys must be String".into()));}
                if let TypeSpec::Generic(n,a)=ty {if n=="Dict" && a.len()==1 {
                    let result=TypeSpec::Generic("option.Option".into(),a);
                    self.validate_type(&result)?;
                    return Ok(result);
                }}
                return Err(SemanticError::InvalidExpression("indexing requires Dict[T]".into()));
            }
            Expr::ListLiteral(items) => {
                if matches!(expected,Some(TypeSpec::Primitive(n)) if n=="Json") {
                    let json=TypeSpec::Primitive("Json".into());
                    for item in items {let ty=self.check_expr(item,Some(&json))?;if !self.compatible(&json,&ty){return Err(SemanticError::InvalidExpression("non-JSON list item".into()));}}
                    return Ok(TypeSpec::Generic("List".into(),vec![json]));
                }
                if let Some(TypeSpec::Generic(name,args))=expected {if name=="List" && args.len()==1 {
                    for item in items {let actual=self.check_expr(item,Some(&args[0]))?;if !self.compatible(&args[0],&actual) {return Err(SemanticError::FunctionReturnTypeMismatch {expected:args[0].clone(),actual});}}
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
                Statement::Const(_)=>return Err(SemanticError::ConstantError("constant declaration must be lowered".into())),
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
                    let (name,args,explicit) = match expr {
                        Expr::FunctionCall {name,args} => (name.clone(),args,None),
                        Expr::IndirectCall {target,args} => {
                            let Expr::Specialize {target,type_args} = target.as_ref() else {return Err(SemanticError::InvalidExpression("spawn requires a named routine".into()));};
                            let name=Self::qualified_name(target).ok_or_else(|| SemanticError::InvalidExpression("spawn requires a named routine".into()))?;
                            (name,args,Some(type_args.as_slice()))
                        }
                        _ => return Err(SemanticError::InvalidExpression("spawn requires a named routine or loop call".into())),
                    };
                    if self.symbol_table.contains_key(&name) || self.qualified_name_shadowed(&name) {return Err(SemanticError::InvalidExpression("spawn target is shadowed".into()));}
                    if let Some(routine)=self.routines.get(&name) {
                        let signature=TypeSpec::Function(routine.params.iter().map(|p|p.param_type.clone()).collect(),Box::new(TypeSpec::Primitive("Unit".into())));
                        self.check_call(&signature,&routine.type_params,explicit,args,None)?;
                    } else if self.loops.contains(&name) {
                        if explicit.is_some() {return Err(SemanticError::GenericTypeError("loops have no type parameters".into()));}
                        if args.len()>1 {return Err(SemanticError::LoopSignalArgumentCount{loop_name:name.clone(),found:args.len()});}
                    } else {return Err(SemanticError::UndeclaredSymbol(name));}
                    for arg in args { if !self.expression_copyable(arg) {return Err(SemanticError::UncopyableSpawnArgument(name.clone()));} }
                }
                Statement::ExprStmt(expr) => { self.check_expr(expr,tail_type)?; }
                Statement::Throw(expr) | Statement::MemorySet { value: expr, .. } | Statement::Signal { payload: expr, .. } => self.check_function_value_expr(expr)?,
                Statement::ForLoop { var_name, collection, body } => {
                    self.check_function_value_expr(collection)?;
                    let scope = self.symbol_table.clone();
                    let collection_type = self.infer_type(collection);
                    if collection_type==TypeSpec::Primitive("Int".into()){
                        self.symbol_table.insert(var_name.clone(),TypeSpec::Primitive("Int".into()));
                        let result=self.check_function_value_body(body,None);self.symbol_table=scope;result?;continue;
                    }
                    let TypeSpec::Generic(name, args) = &collection_type else { return Err(SemanticError::InvalidExpression(format!("for requires Int, List[T] or Dict[T], found {:?}", collection_type))); };
                    if !matches!(name.as_str(),"List"|"Dict") || args.len() != 1 { return Err(SemanticError::InvalidExpression(format!("for requires Int, List[T] or Dict[T], found {:?}", collection_type))); }
                    self.symbol_table.insert(var_name.clone(), if name=="Dict" {TypeSpec::Primitive("String".into())} else {args[0].clone()});
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
                        Self::validate_pattern_bindings(&case.pattern,&mut HashSet::new())?;
                        self.bind_pattern_types(&case.pattern, &target_type)?;
                        let result = self.check_guard(case.guard.as_ref()).and_then(|_|self.check_function_value_body(&case.body, tail_type));
                        self.symbol_table = scope.clone();
                        result?;
                    }
                    if !self.exhaustive(cases,&target_type) {return Err(if tail_type.is_some(){SemanticError::MissingFunctionReturn}else{SemanticError::NonExhaustiveMatch(format!("{:?}",target_type))});}
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
                    if !self.exhaustive(cases, &self.infer_type(target)) {
                        return Err(SemanticError::MissingFunctionReturn);
                    }
                    let scope = self.symbol_table.clone();
                    let target_type = self.infer_type(target);
                    for case in cases {
                        self.symbol_table = scope.clone();
                        Self::validate_pattern_bindings(&case.pattern,&mut HashSet::new())?;
                        self.bind_pattern_types(&case.pattern, &target_type)?;
                        let result = self.check_guard(case.guard.as_ref()).and_then(|_|self.check_function_value_body(&case.body, Some(expected)));
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
                Statement::Const(_)=>return Err(SemanticError::ConstantError("constant declaration must be lowered".into())),
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
                    for case in cases { if let Some(g)=&case.guard{Self::reject_receive(g)?;Self::reject_memory(g)?;} Self::validate_non_loop_body(&case.body)?; }
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
            Expr::DictLiteral(items)=>{for (_,v) in items {Self::reject_receive(v)?;}Ok(())},
            Expr::Index{target,key}=>{Self::reject_receive(target)?;Self::reject_receive(key)},
            Expr::Closure {body,..} => Self::validate_non_loop_body(body),
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
            Expr::Compose{left,right} | Expr::BinaryOp { left, right, .. } => {
                Self::reject_receive(left)?;
                Self::reject_receive(right)
            }
            Expr::MemberAccess { target, .. } | Expr::RefCreate(target) | Expr::TailCall(target) | Expr::Specialize { target, .. } => Self::reject_receive(target),
            _ => Ok(()),
        }
    }

    fn reject_memory(expr: &Expr) -> Result<(), SemanticError> {
        match expr {
            Expr::DictLiteral(items)=>{for (_,v) in items {Self::reject_memory(v)?;}Ok(())},
            Expr::Index{target,key}=>{Self::reject_memory(target)?;Self::reject_memory(key)},
            Expr::Closure {body,..} => Self::validate_non_loop_body(body),
            Expr::MemoryGet(_) => Err(SemanticError::MemoryOutsideLoop),
            Expr::FunctionCall { args, .. } => { for arg in args { Self::reject_memory(arg)?; } Ok(()) }
            Expr::IndirectCall { target, args } => {
                Self::reject_memory(target)?;
                for arg in args { Self::reject_memory(arg)?; }
                Ok(())
            }
            Expr::Compose{left,right} | Expr::BinaryOp { left, right, .. } => { Self::reject_memory(left)?; Self::reject_memory(right) }
            Expr::MemberAccess { target, .. } | Expr::RefCreate(target) | Expr::TailCall(target) | Expr::Specialize { target, .. } => Self::reject_memory(target),
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
            Statement::Const(_)=>return Err(SemanticError::ConstantError("constant declaration must be lowered".into())),
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
                Expr::ConstValue{value,value_type}=>crate::semantic::constants::validate_value(value,value_type)?,
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
                Expr::Compose{left,right} | Expr::BinaryOp { left, right, .. } => {
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
                Expr::RefCreate(target) | Expr::TailCall(target) | Expr::Specialize {target,..} => {
                    expression_is_imported(target, functions)?;
                }
                Expr::ListLiteral(items) => {
                    for item in items {
                        expression_is_imported(item, functions)?;
                    }
                }
                Expr::Closure{body,..} => for stmt in body {TypeChecker::validate_standard_library_calls(stmt,functions)?;},
                Expr::DictLiteral(items)=>for (_,v) in items {expression_is_imported(v,functions)?;},
                Expr::Index{target,key}=>{expression_is_imported(target,functions)?;expression_is_imported(key,functions)?;},
                Expr::IntLiteral(_) | Expr::StringLiteral(_) | Expr::Variable(_) | Expr::TypeOf(_) | Expr::ReceiveSignal | Expr::MemoryGet(_) => {}
            }
            Ok(())
        }

        match statement {
            Statement::Const(_)=>return Err(SemanticError::ConstantError("constant declaration must be lowered".into())),
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
                    if let Some(g)=&case.guard{expression_is_imported(g,functions)?;}
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
        if let Some(name)=Self::qualified_name(expr) {if !self.symbol_table.contains_key(&name) {if let Some(c)=self.constructors.entries.get(&name) {return if c.fields.is_empty(){self.check_expr(expr,None).unwrap_or(TypeSpec::Primitive("Unknown".into()))}else{c.signature()};}}}
        self.constructors.normalize(&self.infer_type_inner(expr))
    }
    fn infer_type_inner(&self, expr: &Expr) -> TypeSpec {
        match expr {
            Expr::Compose{..}=>self.check_expr(expr,None).unwrap_or(TypeSpec::Primitive("Unknown".into())),
            Expr::ConstValue{value_type,..}=>value_type.clone(),
            Expr::DictLiteral(_) | Expr::Index{..} => self.check_expr(expr,None).unwrap_or(TypeSpec::Primitive("Unknown".into())),
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
            Expr::FunctionCall { name, .. } if !self.symbol_table.contains_key(name) && self.function_scheme(name).is_some_and(|(parameters,_)|!parameters.is_empty() || self.constructors.entries.contains_key(name)) => self.check_expr(expr,None).unwrap_or(TypeSpec::Primitive("Unknown".into())),
            Expr::FunctionCall { name, .. } => match self.symbol_table.get(name) {
                Some(TypeSpec::Function(_, result)) => *result.clone(),
                Some(_) => TypeSpec::Primitive("Unknown".into()),
                None => self.functions.get(name).and_then(|func| func.return_type.clone()).unwrap_or_else(|| TypeSpec::Primitive("Unknown".into())),
            },
            Expr::Closure {..} | Expr::Specialize {..} => self.check_expr(expr,None).unwrap_or(TypeSpec::Primitive("Unknown".into())),
            Expr::IndirectCall { target, .. } if self.generic_target(target).is_some() => self.check_expr(expr,None).unwrap_or(TypeSpec::Primitive("Unknown".into())),
            Expr::IndirectCall { target, .. } => match self.infer_type(target) {
                TypeSpec::Function(_, result) => *result,
                _ => TypeSpec::Primitive("Unknown".into()),
            },
            Expr::RefCreate(inner) => TypeSpec::Ref(Box::new(self.infer_type(inner))),
            Expr::TailCall(call)=>self.infer_type(call),
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

// Elaborate composition while lexical types and generic binders are available.
// Generated identifiers contain NUL, which source identifiers cannot contain.
impl TypeChecker {
    pub(crate) fn lower_compositions(&mut self,mut program:Program)->Result<Program,SemanticError>{
        for decl in &mut program.declarations{
            self.symbol_table.clear();self.type_variables.clear();
            match decl{
                TopLevelDecl::Func(f)=>{self.bind_type_parameters(&f.type_params)?;for p in &f.params{self.symbol_table.insert(p.name.clone(),p.param_type.clone());}self.lower_pipe_body(&mut f.body)?;},
                TopLevelDecl::Routine(r)=>{self.bind_type_parameters(&r.type_params)?;for p in &r.params{self.symbol_table.insert(p.name.clone(),p.param_type.clone());}self.lower_pipe_body(&mut r.body)?;},
                TopLevelDecl::Loop(l)=>self.lower_pipe_body(&mut l.body)?,
                _=>{}
            }
        }
        Ok(program)
    }
    fn lower_pipe_expr(&mut self,expr:&mut Expr)->Result<(),SemanticError>{
        match expr{
            Expr::Compose{left,right}=>{
                self.lower_pipe_expr(left)?;self.lower_pipe_expr(right)?;
                let a=self.check_expr(left,None)?;let b=self.check_expr(right,None)?;
                let TypeSpec::Function(inputs,result)=self.check_expr(expr,None)? else{unreachable!()};
                let input=inputs[0].clone();let output=*result;
                let first="\0pipe_first".to_string();let second="\0pipe_second".to_string();let arg="\0pipe_argument".to_string();
                let call=Expr::IndirectCall{target:Box::new(Expr::Variable(second.clone())),args:vec![Expr::IndirectCall{target:Box::new(Expr::Variable(first.clone())),args:vec![Expr::Variable(arg.clone())]}]};
                let closure=Expr::Closure{params:vec![Param{name:arg,param_type:input.clone()}],return_type:output.clone(),body:vec![Statement::ExprStmt(call)]};
                let factory=Expr::Closure{params:vec![Param{name:first,param_type:a},Param{name:second,param_type:b}],return_type:TypeSpec::Function(vec![input],Box::new(output)),body:vec![Statement::ExprStmt(closure)]};
                let Expr::Compose{left,right}=std::mem::replace(expr,Expr::IntLiteral(0)) else{unreachable!()};
                *expr=Expr::IndirectCall{target:Box::new(factory),args:vec![*left,*right]};
            },
            Expr::Closure{params,body,..}=>{
                let saved=self.symbol_table.clone();for p in params{self.symbol_table.insert(p.name.clone(),p.param_type.clone());}
                let result=self.lower_pipe_body(body);self.symbol_table=saved;result?;
            },
            Expr::FunctionCall{args,..}|Expr::ListLiteral(args)=>for e in args{self.lower_pipe_expr(e)?;},
            Expr::DictLiteral(items)=>for (_,e) in items{self.lower_pipe_expr(e)?;},
            Expr::IndirectCall{target,args}=>{self.lower_pipe_expr(target)?;for e in args{self.lower_pipe_expr(e)?;}},
            Expr::BinaryOp{left,right,..}|Expr::Index{target:left,key:right}=>{self.lower_pipe_expr(left)?;self.lower_pipe_expr(right)?;},
            Expr::RefCreate(t)|Expr::TailCall(t)|Expr::MemberAccess{target:t,..}|Expr::Specialize{target:t,..}=>self.lower_pipe_expr(t)?,
            _=>{}
        }
        Ok(())
    }
    fn lower_pipe_body(&mut self,body:&mut [Statement])->Result<(),SemanticError>{
        for stmt in body{
            match stmt{
                Statement::VarDecl{name,type_annot,value,..}=>{self.lower_pipe_expr(value)?;let ty=match type_annot{Some(t)=>t.clone(),None=>self.check_expr(value,None)?};self.symbol_table.insert(name.clone(),ty);},
                Statement::ExprStmt(e)|Statement::Spawn(e)|Statement::Throw(e)|Statement::MemorySet{value:e,..}|Statement::Signal{payload:e,..}=>self.lower_pipe_expr(e)?,
                Statement::ForLoop{var_name,collection,body}=>{
                    self.lower_pipe_expr(collection)?;let saved=self.symbol_table.clone();
                    let ty=match self.infer_type(collection){TypeSpec::Generic(n,a) if n=="List"=>a[0].clone(),TypeSpec::Generic(n,_) if n=="Dict"=>TypeSpec::Primitive("String".into()),_=>TypeSpec::Primitive("Int".into())};
                    self.symbol_table.insert(var_name.clone(),ty);self.lower_pipe_body(body)?;self.symbol_table=saved;
                },
                Statement::Match{target,cases}=>{
                    self.lower_pipe_expr(target)?;let ty=self.infer_type(target);let saved=self.symbol_table.clone();
                    for c in cases{self.symbol_table=saved.clone();self.bind_pattern_types(&c.pattern,&ty)?;if let Some(g)=&mut c.guard{self.lower_pipe_expr(g)?;}self.lower_pipe_body(&mut c.body)?;}
                    self.symbol_table=saved;
                },
                Statement::TryCatch{try_body,error_name,catch_body}=>{
                    let saved=self.symbol_table.clone();self.lower_pipe_body(try_body)?;self.symbol_table=saved.clone();self.symbol_table.insert(error_name.clone(),TypeSpec::Primitive("Unknown".into()));self.lower_pipe_body(catch_body)?;self.symbol_table=saved;
                },
                _=>{}
            }
        }
        Ok(())
    }
}

impl TypeChecker {
    pub fn repl_binding_types(&self,persist:bool)->Result<HashMap<String,TypeSpec>,SemanticError>{
        for (name,ty) in &self.symbol_table{
            if persist && !self.copyable(ty,&mut HashSet::new()){return Err(SemanticError::UncopyableCapture(format!("REPL binding {}: {:?}",name,ty)));}
        }
        Ok(self.symbol_table.clone())
    }
}
