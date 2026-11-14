//! The language prelude is available without source imports. Legacy module
//! imports retain their aliases and qualified functions for compatibility.
use crate::ast::*;
use super::SemanticError;
use std::sync::OnceLock;

pub fn augment(program:&Program)->Result<Option<Program>,SemanticError>{
    if crate::stdlib::validate_standard_library(program).is_ok(){return Ok(None);}
    let present=|name:&str|program.declarations.iter().any(|d|match d{TopLevelDecl::Type(t)=>t.name==name,TopLevelDecl::Func(f)=>f.name==name,_=>false});
    if ["option.Option","result.Result","dict.get"].iter().all(|n|present(n)){return Ok(None);}
    static PRELUDE:OnceLock<Program>=OnceLock::new();
    let prelude=PRELUDE.get_or_init(||super::packages::PackageManager::language_prelude().expect("embedded language prelude must parse"));
    fn name(d:&TopLevelDecl)->Option<&str>{match d{TopLevelDecl::Type(t)=>Some(&t.name),TopLevelDecl::Func(f)=>Some(&f.name),_=>None}}
    let mut result=program.clone();let mut declarations=Vec::new();
    for d in &prelude.declarations{
        let Some(n)=name(d)else{continue};
        if let Some(existing)=program.declarations.iter().find(|d|name(d)==Some(n)){
            // Existing source aliases or resolved canonical members are kept.
            // Unqualified custom types can shadow the prelude type aliases.
            if !n.contains('.') || matches!((existing,d),(TopLevelDecl::Type(_),TopLevelDecl::Type(_))|(TopLevelDecl::Func(_),TopLevelDecl::Func(_))){continue;}
            return Err(SemanticError::InvalidExpression(format!("conflicting built-in declaration {}",n)));
        }
        let mut declaration=d.clone();
        // Old artifacts can use different internal generic binder names.
        // Build aliases from their existing canonical declaration.
        if let TopLevelDecl::Type(alias)=&mut declaration{
            if !alias.name.contains('.'){
                let canonical=match alias.name.as_str(){"Option"=>"option.Option","Result"=>"result.Result",_=>""};
                if let Some(TopLevelDecl::Type(existing))=program.declarations.iter().find(|d|name(d)==Some(canonical)){
                    let short=alias.name.clone();*alias=existing.clone();alias.name=short;
                }
            }
        }
        declarations.push(declaration);
    }
    declarations.extend(result.declarations);result.declarations=declarations;Ok(Some(result))
}
