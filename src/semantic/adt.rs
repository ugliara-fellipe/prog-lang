//! Shared nominal constructor names used by static analysis and the runtime.
use std::collections::{HashMap, HashSet};
use crate::ast::*;

#[derive(Clone, Debug)]
pub struct Constructor {
    pub owner: String,
    pub variant: String,
    pub type_params: Vec<String>,
    pub fields: Vec<TypeSpec>,
}
impl Constructor {
    pub fn signature(&self) -> TypeSpec {
        let result=if self.type_params.is_empty() {TypeSpec::Primitive(self.owner.clone())} else {TypeSpec::Generic(self.owner.clone(),self.type_params.iter().cloned().map(TypeSpec::Primitive).collect())};
        TypeSpec::Function(self.fields.clone(),Box::new(result))
    }
}
#[derive(Clone, Default)]
pub struct Constructors {
    pub entries: HashMap<String, Constructor>,
    pub aliases: HashMap<String,String>,
    pub ambiguous: HashSet<String>,
}
impl Constructors {
    pub fn new(program: &Program) -> Self {
        let enums:Vec<_>=program.declarations.iter().filter_map(|d|match d {TopLevelDecl::Type(t) if !t.variants.is_empty()=>Some(t),_=>None}).collect();
        let mut registry=Self::default();
        for ty in &enums {
            let short=ty.name.rsplit('.').next().unwrap();
            let canonical=if enums.iter().any(|other|other.name==short && other.variants==ty.variants && other.type_params==ty.type_params) {short} else {&ty.name};
            registry.aliases.insert(ty.name.clone(),canonical.to_owned());
        }
        for ty in enums {
            let owner=registry.aliases[&ty.name].clone();
            for variant in &ty.variants {
                let c=Constructor{owner:owner.clone(),variant:variant.name.clone(),type_params:ty.type_params.clone(),fields:variant.fields.iter().map(|t|registry.normalize(t)).collect()};
                let mut names=vec![format!("{}.{}",ty.name,variant.name),variant.name.clone()];
                if let Some((package,_))=ty.name.rsplit_once('.') {names.push(format!("{}.{}",package,variant.name));}
                for name in names {
                    if registry.ambiguous.contains(&name) {continue;}
                    if registry.entries.get(&name).is_some_and(|old|old.owner!=owner || old.variant!=variant.name) {
                        registry.entries.remove(&name);registry.ambiguous.insert(name);
                    } else {registry.entries.insert(name,c.clone());}
                }
            }
        }
        registry
    }
    pub fn normalize(&self, ty:&TypeSpec)->TypeSpec {
        match ty {
            TypeSpec::Primitive(n)=>TypeSpec::Primitive(self.aliases.get(n).unwrap_or(n).clone()),
            TypeSpec::Generic(n,args)=>TypeSpec::Generic(self.aliases.get(n).unwrap_or(n).clone(),args.iter().map(|a|self.normalize(a)).collect()),
            TypeSpec::Function(args,r)=>TypeSpec::Function(args.iter().map(|a|self.normalize(a)).collect(),Box::new(self.normalize(r))),
            TypeSpec::Ref(t)=>TypeSpec::Ref(Box::new(self.normalize(t))),
        }
    }
}
