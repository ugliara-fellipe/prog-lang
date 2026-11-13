//! Rank-one type substitution and first-order unification for parametric declarations.
use std::collections::{HashMap, HashSet};
use crate::ast::TypeSpec;
use crate::semantic::SemanticError;

pub type Substitution = HashMap<String, TypeSpec>;

pub fn substitute(ty: &TypeSpec, substitution: &Substitution) -> TypeSpec {
    match ty {
        TypeSpec::Primitive(name) => substitution.get(name).cloned().unwrap_or_else(|| ty.clone()),
        TypeSpec::Generic(name, args) => TypeSpec::Generic(name.clone(), args.iter().map(|arg| substitute(arg, substitution)).collect()),
        TypeSpec::Ref(inner) => TypeSpec::Ref(Box::new(substitute(inner, substitution))),
        TypeSpec::Function(args, result) => TypeSpec::Function(args.iter().map(|arg| substitute(arg, substitution)).collect(), Box::new(substitute(result, substitution))),
    }
}

/// Only names explicitly listed in `variables` are flexible. Caller variables
/// remain rigid, even if they share a spelling with the callee's binders.
pub fn unify(pattern: &TypeSpec, actual: &TypeSpec, variables: &HashSet<String>, bindings: &mut Substitution) -> bool {
    match (pattern, actual) {
        (TypeSpec::Primitive(name), _) if variables.contains(name) => {
            if matches!(actual, TypeSpec::Primitive(name) if name == "Unknown") { return true; }
            match bindings.get(name) { Some(previous) => previous == actual, None => { bindings.insert(name.clone(), actual.clone()); true } }
        }
        (_, TypeSpec::Primitive(name)) if name == "Unknown" => true,
        (TypeSpec::Generic(name, args), TypeSpec::Generic(other, actual)) if name == other && args.len() == actual.len() => args.iter().zip(actual).all(|(a,b)| unify(a,b,variables,bindings)),
        (TypeSpec::Ref(a), TypeSpec::Ref(b)) => unify(a,b,variables,bindings),
        (TypeSpec::Function(args, result), TypeSpec::Function(actual, actual_result)) if args.len() == actual.len() => args.iter().zip(actual).all(|(a,b)| unify(a,b,variables,bindings)) && unify(result,actual_result,variables,bindings),
        _ => pattern == actual,
    }
}

pub fn freshen(signature: &TypeSpec, parameters: &[String]) -> (TypeSpec, HashSet<String>) {
    let rename: Substitution = parameters.iter().enumerate().map(|(index,name)| (name.clone(),TypeSpec::Primitive(format!("\0generic:{}",index)))).collect();
    let variables = rename.values().filter_map(|ty| if let TypeSpec::Primitive(name)=ty {Some(name.clone())} else {None}).collect();
    (substitute(signature,&rename), variables)
}

pub fn instantiate(signature: &TypeSpec, parameters: &[String], explicit: Option<&[TypeSpec]>, arguments: &[TypeSpec], expected: Option<&TypeSpec>) -> Result<TypeSpec, SemanticError> {
    let (signature, variables) = freshen(signature,parameters);
    let TypeSpec::Function(inputs,result) = &signature else { return Err(SemanticError::NotAFunction(signature)); };
    if inputs.len() != arguments.len() { return Err(SemanticError::ArgumentCountMismatch { function:"generic function".into(),expected:inputs.len(),found:arguments.len() }); }
    let mut bindings=Substitution::new();
    if let Some(explicit)=explicit {
        if explicit.len()!=parameters.len() { return Err(SemanticError::GenericTypeError(format!("expected {} type arguments, found {}",parameters.len(),explicit.len()))); }
        for (index,ty) in explicit.iter().enumerate() { bindings.insert(format!("\0generic:{}",index),ty.clone()); }
    }
    for (index,(parameter,argument)) in inputs.iter().zip(arguments).enumerate() {
        if !unify(parameter,argument,&variables,&mut bindings) { return Err(SemanticError::FunctionArgumentTypeMismatch {index:index+1,expected:substitute(parameter,&bindings),actual:argument.clone()}); }
    }
    if let Some(expected)=expected {
        if !unify(result,expected,&variables,&mut bindings) { return Err(SemanticError::FunctionReturnTypeMismatch {expected:expected.clone(),actual:substitute(result,&bindings)}); }
    }
    if variables.iter().any(|name| !bindings.contains_key(name)) { return Err(SemanticError::GenericTypeError("cannot infer all type parameters; provide explicit type arguments or a type annotation".into())); }
    Ok(substitute(&signature,&bindings))
}

pub fn specialize(signature: &TypeSpec, parameters: &[String], arguments: &[TypeSpec]) -> Result<TypeSpec, SemanticError> {
    if parameters.len()!=arguments.len() { return Err(SemanticError::GenericTypeError(format!("expected {} type arguments, found {}",parameters.len(),arguments.len()))); }
    Ok(substitute(signature,&parameters.iter().cloned().zip(arguments.iter().cloned()).collect()))
}

pub fn instantiate_value(signature: &TypeSpec, parameters: &[String], expected: &TypeSpec) -> Result<TypeSpec, SemanticError> {
    let (signature,variables)=freshen(signature,parameters);
    let mut bindings=Substitution::new();
    if !unify(&signature,expected,&variables,&mut bindings) || variables.iter().any(|name| !bindings.contains_key(name)) { return Err(SemanticError::GenericTypeError(format!("generic function cannot be specialized to {:?}",expected))); }
    Ok(substitute(&signature,&bindings))
}
