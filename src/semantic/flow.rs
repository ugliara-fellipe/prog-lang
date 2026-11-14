//! Finite, monotone analysis of named function values. Results refer to nodes
//! in the supplied Program, so consumers must query the same AST.
use std::collections::{HashMap, HashSet};
use crate::ast::*;
use crate::semantic::SemanticError;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Value {
    targets: HashSet<String>,
    // Container contents are deliberately merged. This loses positional
    // precision but never omits a function stored in a nested list.
    contents: HashSet<String>,
    unknown: bool,
    contents_unknown: bool,
}
impl Value {
    fn named(name: &str) -> Self { Self { targets: [name.to_owned()].into(), ..Self::default() } }
    fn unknown() -> Self { Self { unknown: true, ..Self::default() } }
    fn merge(&mut self, other: &Self) -> bool {
        let before = self.clone();
        self.targets.extend(other.targets.iter().cloned());
        self.contents.extend(other.contents.iter().cloned());
        self.unknown |= other.unknown;
        self.contents_unknown |= other.contents_unknown;
        *self != before
    }
    fn element(&self) -> Self {
        Self { targets: self.contents.clone(), contents: self.contents.clone(), unknown: self.contents_unknown || self.unknown, contents_unknown: self.contents_unknown || self.unknown }
    }
}

#[derive(Default)]
pub struct FunctionFlow {
    targets: HashMap<usize, HashSet<String>>,
    indirect: HashSet<usize>,
    calls: HashMap<String, HashSet<String>>,
    spawns: HashMap<String, HashSet<String>>,
    indirect_calls: HashMap<String, HashSet<String>>,
}
impl FunctionFlow {
    pub fn analyze(program: &Program) -> Result<Self, SemanticError> {
        for decl in &program.declarations{if let TopLevelDecl::Func(f)=decl{if f.foreign.is_some(){crate::ffi::validate(f).map_err(SemanticError::InvalidExpression)?;}}}
        if crate::semantic::pipes::needs_lowering(program){return Err(SemanticError::InvalidExpression("lower composition before function flow analysis".into()));}
        if crate::semantic::constants::needs_lowering(program){return Err(SemanticError::ConstantError("lower constants before function flow analysis".into()));}
        crate::semantic::tail_calls::validate_program(program)?;
        let mut solver = Solver::new(program);
        solver.solve();
        // A declaration with no incoming function values is an open entry
        // point. Conservatively admit every named value matching its type.
        // Actual call sites otherwise supply the more precise finite sets.
        solver.seed_open_parameters();
        solver.solve();
        if let Some(reason) = solver.unresolved.iter().min() {
            return Err(SemanticError::UndeclaredSymbol(format!("cannot safely resolve indirect call: {}", reason)));
        }
        Ok(solver.flow)
    }
    pub fn targets_for(&self, expr: &Expr) -> HashSet<String> { self.targets.get(&(expr as *const Expr as usize)).cloned().unwrap_or_default() }
    pub fn is_indirect(&self, expr: &Expr) -> bool { self.indirect.contains(&(expr as *const Expr as usize)) }
    pub fn spawns_by_owner(&self)->&HashMap<String,HashSet<String>>{&self.spawns}
    pub fn calls_by_owner(&self) -> &HashMap<String, HashSet<String>> { &self.calls }
    pub fn indirect_calls_by_owner(&self) -> &HashMap<String, HashSet<String>> { &self.indirect_calls }
}

struct Solver<'a> {
    program: &'a Program,
    constructors: crate::semantic::adt::Constructors,
    functions: HashMap<String, &'a FuncDecl>,
    closures: HashMap<String, &'a Expr>,
    captures: HashMap<String, HashMap<String,Value>>,
    closure_type_variables: Vec<String>,
    params: HashMap<String, Vec<Value>>,
    returns: HashMap<String, Value>,
    raised: HashMap<String, Value>,
    flow: FunctionFlow,
    changed: bool,
    unresolved: HashSet<String>,
}
impl<'a> Solver<'a> {
    fn new(program: &'a Program) -> Self {
        let mut params = HashMap::new();
        let mut functions = HashMap::new();
        for decl in &program.declarations {
            match decl {
                TopLevelDecl::Func(f) => { functions.insert(f.name.clone(), f); params.insert(f.name.clone(), vec![Value::default(); f.params.len()]); }
                TopLevelDecl::Routine(r) => { params.insert(r.name.clone(), vec![Value::default(); r.params.len()]); }
                _ => {}
            }
        }
        let closures:HashMap<_,_>=crate::semantic::closures::collect(program).into_iter().map(|e|(crate::semantic::closures::identity(e),e)).collect();
        for (name,expr) in &closures {if let Expr::Closure{params:args,..}=expr {params.insert(name.clone(),vec![Value::default();args.len()]);}}
        let closure_type_variables=program.declarations.iter().flat_map(|d|match d{TopLevelDecl::Func(f)=>f.type_params.clone(),TopLevelDecl::Routine(r)=>r.type_params.clone(),_=>vec![]}).collect();
        Self { program, closures, captures:HashMap::new(), closure_type_variables, constructors: crate::semantic::adt::Constructors::new(program), functions, params, returns: HashMap::new(), raised: HashMap::new(), flow: FunctionFlow::default(), changed: false, unresolved: HashSet::new() }
    }
    fn solve(&mut self) {
        loop {
            self.changed = false;
            self.unresolved.clear();
            for decl in &self.program.declarations {
                let (name, params, body) = match decl {
                    TopLevelDecl::Func(f) => (&f.name, f.params.as_slice(), f.body.as_slice()),
                    TopLevelDecl::Routine(r) => (&r.name, r.params.as_slice(), r.body.as_slice()),
                    TopLevelDecl::Loop(l) => (&l.name, &[][..], l.body.as_slice()),
                    _ => continue,
                };
                let values = self.params.get(name).cloned().unwrap_or_default();
                let mut scope = params.iter().zip(values).map(|(p,v)| (p.name.clone(),v)).collect();
                let result = self.body(name, body, &mut scope);
                self.changed |= self.returns.entry(name.clone()).or_default().merge(&result);
            }
            for (name,expr) in self.closures.clone() {
                if let Some(mut scope)=self.captures.get(&name).cloned() {
                    if let Expr::Closure{params,body,..}=expr {
                        for (p,v) in params.iter().zip(self.params[&name].clone()){scope.insert(p.name.clone(),v);}
                        let result=self.body(&name,body,&mut scope);
                        self.changed |= self.returns.entry(name).or_default().merge(&result);
                    }
                }
            }
            if !self.changed { break; }
        }
    }
    fn signature(f: &FuncDecl) -> TypeSpec {
        TypeSpec::Function(f.params.iter().map(|p| p.param_type.clone()).collect(), Box::new(f.return_type.clone().unwrap_or(TypeSpec::Primitive("Unknown".into()))))
    }
    // Open generic entries must admit every instantiation of their type
    // parameters. Shape matching deliberately overapproximates equalities
    // between repeated variables; instantiated calls are checked separately.
    fn signature_shape_matches(a: &TypeSpec, avars: &[String], b: &TypeSpec, bvars: &[String]) -> bool {
        if matches!(a, TypeSpec::Primitive(n) if avars.contains(n)) || matches!(b, TypeSpec::Primitive(n) if bvars.contains(n)) { return true; }
        match (a,b) {
            (TypeSpec::Primitive(a),TypeSpec::Primitive(b)) => a == b,
            (TypeSpec::Ref(a),TypeSpec::Ref(b)) => Self::signature_shape_matches(a,avars,b,bvars),
            (TypeSpec::Generic(an,a),TypeSpec::Generic(bn,b)) => an == bn && a.len() == b.len() && a.iter().zip(b).all(|(a,b)| Self::signature_shape_matches(a,avars,b,bvars)),
            (TypeSpec::Function(a,ar),TypeSpec::Function(b,br)) => a.len() == b.len() && a.iter().zip(b).all(|(a,b)| Self::signature_shape_matches(a,avars,b,bvars)) && Self::signature_shape_matches(ar,avars,br,bvars),
            _ => false,
        }
    }
    fn candidates(&self, ty: &TypeSpec, type_params: &[String]) -> Value {
        self.candidates_inner(ty,type_params,&mut HashSet::new())
    }
    fn candidates_inner(&self, ty:&TypeSpec,type_params:&[String],seen:&mut HashSet<String>)->Value {
        let normalized=self.constructors.normalize(ty);
        let (name,args)=match &normalized {TypeSpec::Primitive(n)=>(n.as_str(),vec![]),TypeSpec::Generic(n,args)=>(n.as_str(),args.clone()),_=>("",vec![])};
        let constructors:Vec<_>=self.constructors.entries.values().filter(|c|c.owner==name).collect();
        if !constructors.is_empty() && seen.insert(name.into()) {
            let mut value=Value::default();
            for c in constructors {let sub=c.type_params.iter().cloned().zip(args.iter().cloned()).collect();for field in &c.fields {
                let field=crate::semantic::generics::substitute(field,&sub);let nested=self.candidates_inner(&field,type_params,seen);
                value.contents.extend(nested.targets);value.contents.extend(nested.contents);value.contents_unknown |= nested.unknown || nested.contents_unknown;
            }}
            seen.remove(name);return value;
        }
        match ty {
            TypeSpec::Function(_, _) => Value { targets: self.functions.iter().filter(|(_, f)| Self::signature_shape_matches(&normalized,type_params,&self.constructors.normalize(&Self::signature(f)),&f.type_params)).map(|(name,_)| name.clone()).chain(self.closures.iter().filter(|(_,e)|Self::signature_shape_matches(&normalized,type_params,&self.constructors.normalize(&crate::semantic::closures::signature(e)),&self.closure_type_variables)).map(|(name,_)|name.clone())).chain(self.constructors.entries.iter().filter(|(_,c)|Self::signature_shape_matches(&normalized,type_params,&c.signature(),&c.type_params)).map(|(name,_)|name.clone())).collect(), ..Value::default() },
            TypeSpec::Generic(_, args) => {
                let mut result = Value::default();
                for arg in args { let nested = self.candidates_inner(arg,type_params,seen); result.contents.extend(nested.targets); result.contents.extend(nested.contents); }
                result
            }
            TypeSpec::Ref(inner) => self.candidates_inner(inner,type_params,seen),
            // A type variable may itself be instantiated with a function or
            // a container of functions. Do not silently treat it as scalar.
            TypeSpec::Primitive(name) if type_params.contains(name) => Value { targets: self.functions.keys().cloned().collect(), contents: self.functions.keys().cloned().collect(), ..Value::default() },
            _ => Value::default(),
        }
    }
    fn seed_open_parameters(&mut self) {
        for decl in &self.program.declarations {
            let (name, params, type_params) = match decl { TopLevelDecl::Func(f) => (&f.name, &f.params, &f.type_params), TopLevelDecl::Routine(r) => (&r.name, &r.params, &r.type_params), _ => continue };
            if crate::stdlib::is_prelude_function(name){continue;}
            for (index,param) in params.iter().enumerate() {
                let existing = &self.params[name][index];
                if existing.targets.is_empty() && existing.contents.is_empty() && !existing.unknown {
                    let fallback = self.candidates(&param.param_type,type_params);
                    self.params.get_mut(name).unwrap()[index].merge(&fallback);
                }
            }
        }
        for (name,expr) in &self.closures {
            if let Expr::Closure{params,..}=expr {
                for (i,param) in params.iter().enumerate(){let value=&self.params[name][i];if value.targets.is_empty() && value.contents.is_empty() && !value.unknown {
                    let fallback=self.candidates(&param.param_type,&self.closure_type_variables);self.params.get_mut(name).unwrap()[i].merge(&fallback);
                }}
            }
        }
    }
    fn expr(&mut self, owner: &str, expr: &Expr, scope: &HashMap<String,Value>, spawn: bool) -> Value {
        match expr {
            Expr::Closure{params,body,..}=>{
                let name=crate::semantic::closures::identity(expr);
                self.changed |= !self.captures.contains_key(&name);
                let captured=self.captures.entry(name.clone()).or_default();
                for variable in crate::semantic::closures::free_variables(params,body){if let Some(value)=scope.get(&variable){self.changed |= captured.entry(variable).or_default().merge(value);}}
                Value::named(&name)
            },
            Expr::Variable(name) => scope.get(name).cloned().unwrap_or_else(|| if self.functions.contains_key(name) || self.constructors.entries.get(name).is_some_and(|c|!c.fields.is_empty()) { Value::named(name) } else if self.constructors.entries.contains_key(name) {Value::default()} else if name == "True" || name == "False" { Value::default() } else { Value::unknown() }),
            Expr::FunctionCall { name, args } => {
                let (target, indirect) = match scope.get(name) {
                    Some(value) => (value.clone(),true),
                    None if name.contains('.') && scope.contains_key(name.split('.').next().unwrap_or(name)) => (Value::unknown(),true),
                    None => (Value::named(name),false),
                };
                self.call(owner, expr, target, args, scope, spawn, indirect)
            }
            Expr::IndirectCall { target, args } => {
                if let Expr::Specialize { target: named, .. } = target.as_ref() {
                    if let Some(name) = Self::qualified(named) {
                        if !scope.contains_key(name.split('.').next().unwrap_or(&name)) && (self.functions.contains_key(&name) || self.constructors.entries.contains_key(&name) || (spawn && self.params.contains_key(&name))) {
                            return self.call(owner,expr,Value::named(&name),args,scope,spawn,false);
                        }
                    }
                }
                let value = self.expr(owner,target,scope,false);
                self.call(owner, expr,value,args,scope,spawn,true)
            }
            Expr::DictLiteral(items)=>{let mut result=Value::default();for (_,item) in items{let value=self.expr(owner,item,scope,false);result.contents.extend(value.targets);result.contents.extend(value.contents);result.contents_unknown |= value.unknown || value.contents_unknown;}result},
            Expr::Index{target,key}=>{let v=self.expr(owner,target,scope,false);self.expr(owner,key,scope,false);Value{contents:v.contents,contents_unknown:v.contents_unknown || v.unknown,..Value::default()}},
            Expr::ListLiteral(items) => {
                let mut result = Value::default();
                for item in items { let value = self.expr(owner,item,scope,false); result.contents.extend(value.targets); result.contents.extend(value.contents); result.contents_unknown |= value.unknown || value.contents_unknown; }
                result
            }
            Expr::RefCreate(inner) | Expr::TailCall(inner) | Expr::Specialize { target: inner, .. } => self.expr(owner,inner,scope,false),
            Expr::MemberAccess { target, field } => {
                if let Some(name) = Self::qualified(target) {
                    // A local receiver shadows a package qualifier. Resolving
                    // its member as a global function would omit a possible
                    // dynamic value (or accept a non-function member).
                    if scope.contains_key(name.split('.').next().unwrap_or(&name)) {
                        self.expr(owner,target,scope,false);
                        return Value::unknown();
                    }
                    let name = format!("{}.{}",name,field);
                    if self.functions.contains_key(&name) || self.constructors.entries.get(&name).is_some_and(|c|!c.fields.is_empty()) { return Value::named(&name); }
                    if self.constructors.entries.contains_key(&name) {return Value::default();}
                }
                self.expr(owner,target,scope,false);
                Value::unknown()
            }
            Expr::BinaryOp { left, right, .. } => { self.expr(owner,left,scope,false); self.expr(owner,right,scope,false); Value::default() }
            Expr::MemoryGet(_) | Expr::ReceiveSignal => Value::unknown(),
            _ => Value::default(),
        }
    }
    fn qualified(expr: &Expr) -> Option<String> { match expr { Expr::Variable(name) => Some(name.clone()), Expr::MemberAccess {target,field} => Some(format!("{}.{}",Self::qualified(target)?,field)), _ => None } }
    fn call(&mut self, owner: &str, expr: &Expr, target: Value, args: &[Expr], scope: &HashMap<String,Value>, spawn: bool, indirect: bool) -> Value {
        let values: Vec<_> = args.iter().map(|arg| self.expr(owner,arg,scope,false)).collect();
        let id = expr as *const Expr as usize;
        let before = self.flow.targets.entry(id).or_default().len();
        self.flow.targets.entry(id).or_default().extend(target.targets.iter().cloned());
        self.changed |= before != self.flow.targets[&id].len();
        if indirect {
            self.flow.indirect.insert(id);
            // Bottom and unknown are distinct: after the fixed point and
            // open-parameter seeding, bottom has no inhabiting function value
            // in this closed program. It occurs in unused generic helpers.
            // Unknown may denote an unmodelled destination and is rejected.
            if target.unknown { self.unresolved.insert(format!("{}: {:?}",crate::semantic::closures::display_name(owner),expr)); }
        }
        let mut result = Value::default();
        for name in &target.targets {
            if self.constructors.entries.contains_key(name) {
                for value in &values {result.contents.extend(value.targets.iter().cloned());result.contents.extend(value.contents.iter().cloned());result.contents_unknown |= value.unknown || value.contents_unknown;}
                continue;
            }
            if spawn{self.changed |= self.flow.spawns.entry(owner.to_owned()).or_default().insert(name.clone());}
            if !spawn {
                self.changed |= self.flow.calls.entry(owner.to_owned()).or_default().insert(name.clone());
                if indirect { self.changed |= self.flow.indirect_calls.entry(owner.to_owned()).or_default().insert(name.clone()); }
            }
            if let Some(params) = self.params.get_mut(name) {
                for (param,value) in params.iter_mut().zip(&values) { self.changed |= param.merge(value); }
            } else if indirect { self.unresolved.insert(format!("{}: {} is not a registered function",crate::semantic::closures::display_name(owner),crate::semantic::closures::display_name(name))); }
            if let Some(raised) = self.raised.get(name).cloned() {
                self.changed |= self.raised.entry(owner.to_owned()).or_default().merge(&raised);
            }
            // Only the certified qualified primitive has this transfer
            // function. A user function named `prepend` must use its body.
            if crate::stdlib::is_bundled_native_function(name) {
                if matches!(name.as_str(),"dict.get"|"dict.remove"|"dict.values"|"dict.set") {if let Some(v)=values.first(){result.merge(v);}}
                if name=="dict.set" {if let Some(v)=values.get(2){result.contents.extend(v.targets.iter().cloned());result.contents.extend(v.contents.iter().cloned());result.contents_unknown |= v.unknown || v.contents_unknown;}}
            } else if name == "std.prepend" {
                if let Some(head) = values.first() { result.contents.extend(head.targets.iter().cloned()); result.contents.extend(head.contents.iter().cloned()); result.contents_unknown |= head.unknown || head.contents_unknown; }
                if let Some(tail) = values.get(1) { result.merge(tail); }
            } else if let Some(value) = self.returns.get(name) { result.merge(value); }
            else if !self.params.contains_key(name) { result.unknown = true; }
        }
        result
    }
    fn bind(pattern: &Pattern, value: &Value, scope: &mut HashMap<String,Value>) {
        match pattern {
            Pattern::Identifier(name) => { scope.insert(name.clone(),value.clone()); }
            Pattern::Constructor(name,parts) if name == "Cons" && parts.len() == 2 => { Self::bind(&parts[0],&value.element(),scope); Self::bind(&parts[1],value,scope); }
            Pattern::Constructor(_,parts) => for part in parts {Self::bind(part,&value.element(),scope);},
            _ => {}
        }
    }
    fn body(&mut self, owner: &str, body: &[Statement], scope: &mut HashMap<String,Value>) -> Value {
        let mut result = Value::default();
        for stmt in body {
            result = match stmt {
                Statement::Const(_)=>Value::default(),
                Statement::VarDecl { name,value,.. } => { let value = self.expr(owner,value,scope,false); scope.insert(name.clone(),value); Value::default() }
                Statement::ExprStmt(expr) => self.expr(owner,expr,scope,false),
                Statement::Spawn(expr) => { self.expr(owner,expr,scope,true); Value::default() }
                Statement::Throw(expr) => { let value=self.expr(owner,expr,scope,false); self.changed |= self.raised.entry(owner.to_owned()).or_default().merge(&value); Value::default() }
                Statement::Signal{loop_name,payload}=>{self.changed |= self.flow.spawns.entry(owner.to_owned()).or_default().insert(loop_name.clone());self.expr(owner,payload,scope,false);Value::default()}
                Statement::MemorySet {value:expr,..} => { self.expr(owner,expr,scope,false); Value::default() }
                Statement::Match { target,cases } => {
                    let target = self.expr(owner,target,scope,false); let mut value = Value::default();
                    for case in cases { let mut local=scope.clone(); Self::bind(&case.pattern,&target,&mut local); if let Some(g)=&case.guard{self.expr(owner,g,&local,false);} value.merge(&self.body(owner,&case.body,&mut local)); }
                    value
                }
                Statement::ForLoop {var_name,collection,body} => { let collection=self.expr(owner,collection,scope,false); let mut local=scope.clone(); local.insert(var_name.clone(),collection.element()); self.body(owner,body,&mut local); Value::default() }
                Statement::TryCatch {try_body,error_name,catch_body} => { let mut value=self.body(owner,try_body,&mut scope.clone()); let mut local=scope.clone(); local.insert(error_name.clone(),self.raised.get(owner).cloned().unwrap_or_default()); value.merge(&self.body(owner,catch_body,&mut local)); value }
                Statement::Terminate => Value::default(),
            };
        }
        result
    }
}
