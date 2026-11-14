//! Compile-time data evaluation and lexical substitution. No runtime evaluator
//! or user function is called here: the accepted expression language is finite.
use std::collections::{HashMap, HashSet};
use crate::ast::*;
use super::{json_types, SemanticError};

#[derive(Clone)]
struct Value { expr: Expr, ty: TypeSpec }
impl Value {
    fn reference(&self)->Expr {
        Expr::ConstValue{value:Box::new(self.expr.clone()),value_type:self.ty.clone()}
    }
}
type Scope = HashMap<String, Option<Value>>;
fn error(message:impl Into<String>)->SemanticError {SemanticError::ConstantError(message.into())}
fn primitive(name:&str)->TypeSpec {TypeSpec::Primitive(name.into())}
fn qualified(expr:&Expr)->Option<String> {
    match expr {
        Expr::Variable(n)=>Some(n.clone()),
        Expr::MemberAccess{target,field}=>Some(format!("{}.{}",qualified(target)?,field)),
        _=>None
    }
}
fn type_name(ty:&TypeSpec)->String {
    match ty {
        TypeSpec::Primitive(n)=>n.clone(),
        TypeSpec::Generic(n,args)=>format!("{}[{}]",n,args.iter().map(type_name).collect::<Vec<_>>().join(", ")),
        _=>unreachable!("constant data types are validated"),
    }
}
fn compatible(expected:&TypeSpec,actual:&TypeSpec)->bool {
    expected==actual || matches!(expected,TypeSpec::Primitive(n) if n=="Json") && json_types::is_json_type(actual)
}
fn validate_type(ty:&TypeSpec)->Result<(),SemanticError>{
    match ty {
        TypeSpec::Primitive(n) if matches!(n.as_str(),"Int"|"String"|"Bool"|"Json")=>Ok(()),
        TypeSpec::Generic(n,args) if matches!(n.as_str(),"List"|"Dict") && args.len()==1=>validate_type(&args[0]),
        _=>Err(error(format!("constant type must be Int, String, Bool, Json, List[T] or Dict[T], found {:?}",ty)))
    }
}

/// Checks the serialized constant payload as well as its type. Artifacts cannot
/// hide calls, references or runtime bindings inside a precomputed value.
pub fn validate_value(expr:&Expr,ty:&TypeSpec)->Result<(),SemanticError>{
    fn literal(e:&Expr)->bool{match e{
        Expr::IntLiteral(_)|Expr::StringLiteral(_)=>true,
        Expr::Variable(n)=>matches!(n.as_str(),"True"|"False"),
        Expr::ListLiteral(v)=>v.iter().all(literal),
        Expr::DictLiteral(v)=>v.iter().all(|(_,e)|literal(e)),
        _=>false,
    }}
    if !literal(expr){return Err(error("precomputed payload is not literal data"));}
    evaluate(expr,Some(ty),&mut |_|Err(error("non-literal reference"))).map(|_|())
}

fn evaluate(expr:&Expr,expected:Option<&TypeSpec>,lookup:&mut impl FnMut(&str)->Result<Value,SemanticError>)->Result<Value,SemanticError>{
    if let Some(ty)=expected{validate_type(ty)?;}
    let mut value=match expr {
        Expr::ConstValue{value,value_type}=>{validate_value(value,value_type)?;Value{expr:(**value).clone(),ty:value_type.clone()}},
        Expr::IntLiteral(_)=>Value{expr:expr.clone(),ty:primitive("Int")},
        Expr::StringLiteral(_)=>Value{expr:expr.clone(),ty:primitive("String")},
        Expr::Variable(n) if matches!(n.as_str(),"True"|"False")=>Value{expr:expr.clone(),ty:primitive("Bool")},
        Expr::Variable(n)=>lookup(n)?,
        Expr::MemberAccess{..}=>lookup(&qualified(expr).ok_or_else(||error("invalid constant name"))?)?,
        Expr::TypeOf(n)=>{let value=lookup(n)?;Value{expr:Expr::StringLiteral(type_name(&value.ty)),ty:primitive("String")}},
        Expr::ListLiteral(items)=>{
            let item_type=match expected{Some(TypeSpec::Generic(n,a)) if n=="List"=>Some(&a[0]),Some(TypeSpec::Primitive(n)) if n=="Json"=>Some(expected.unwrap()),_=>None};
            let values=items.iter().map(|e|evaluate(e,item_type,lookup)).collect::<Result<Vec<_>,_>>()?;
            let element=common_type(&values,item_type)?;
            Value{expr:Expr::ListLiteral(values.into_iter().map(|v|v.expr).collect()),ty:TypeSpec::Generic("List".into(),vec![element])}
        },
        Expr::DictLiteral(items)=>{
            let item_type=match expected{Some(TypeSpec::Generic(n,a)) if n=="Dict"=>Some(&a[0]),Some(TypeSpec::Primitive(n)) if n=="Json"=>Some(expected.unwrap()),_=>None};
            let mut names=HashSet::new();let mut values=Vec::new();let mut keys=Vec::new();
            for (k,e) in items{
                if !names.insert(k){return Err(error(format!("duplicate dictionary key {}",k)));}
                keys.push(k.clone());values.push(evaluate(e,item_type,lookup)?);
            }
            let element=common_type(&values,item_type)?;
            Value{expr:Expr::DictLiteral(keys.into_iter().zip(values.into_iter().map(|v|v.expr)).collect()),ty:TypeSpec::Generic("Dict".into(),vec![element])}
        },
        Expr::BinaryOp{op,left,right}=>{
            let a=evaluate(left,None,lookup)?;let b=evaluate(right,None,lookup)?;
            let result=match (op.as_str(),&a.expr,&b.expr) {
                ("+"|"-"|"*"|"/",Expr::IntLiteral(x),Expr::IntLiteral(y)) if a.ty==primitive("Int") && b.ty==primitive("Int")=>{
                    if op=="/" && *y==0{return Err(error("division by zero"));}
                    let n=match op.as_str(){"+"=>x.checked_add(*y),"-"=>x.checked_sub(*y),"*"=>x.checked_mul(*y),_=>x.checked_div(*y)}.ok_or_else(||error("integer overflow"))?;
                    Value{expr:Expr::IntLiteral(n),ty:primitive("Int")}
                },
                ("=="|"!=",_,_) if a.ty==b.ty=>{
                    let same=data_equal(&a.expr,&b.expr);
                    Value{expr:Expr::Variable(if same==(op=="=="){"True"}else{"False"}.into()),ty:primitive("Bool")}
                },
                ("<"|"<="|">"|">=",Expr::IntLiteral(x),Expr::IntLiteral(y)) if a.ty==primitive("Int") && b.ty==primitive("Int")=>{
                    let yes=match op.as_str(){"<"=>x<y,"<="=>x<=y,">"=>x>y,_=>x>=y};
                    Value{expr:Expr::Variable(if yes{"True"}else{"False"}.into()),ty:primitive("Bool")}
                },
                _=>return Err(error(format!("operator {} is incompatible with {:?} and {:?}",op,a.ty,b.ty)))
            };result
        },
        _=>return Err(error("initializer must contain only literals, constant names, type queries and data operators; calls, closures, indexing, references and actor state are not compile-time expressions"))
    };
    if let Some(ty)=expected{
        if !compatible(ty,&value.ty){return Err(error(format!("expected {:?}, found {:?}",ty,value.ty)));}
        value.ty=ty.clone();
    }
    Ok(value)
}
fn common_type(values:&[Value],expected:Option<&TypeSpec>)->Result<TypeSpec,SemanticError>{
    if let Some(t)=expected{return Ok(t.clone());}
    if values.is_empty(){return Ok(primitive("Json"));}
    if values.iter().all(|v|v.ty==values[0].ty){return Ok(values[0].ty.clone());}
    if values.iter().all(|v|json_types::is_json_type(&v.ty)){return Ok(primitive("Json"));}
    Err(error("collection values require a common compile-time data type"))
}
fn data_equal(a:&Expr,b:&Expr)->bool{
    match (a,b){
        (Expr::ListLiteral(a),Expr::ListLiteral(b))=>a.len()==b.len() && a.iter().zip(b).all(|(a,b)|data_equal(a,b)),
        (Expr::DictLiteral(a),Expr::DictLiteral(b))=>a.len()==b.len() && a.iter().all(|(k,v)|b.iter().find(|(other,_)|other==k).is_some_and(|(_,other)|data_equal(v,other))),
        _=>a==b
    }
}
struct Evaluator {
    declarations:HashMap<String,ConstDecl>,
    values:HashMap<String,Value>,
    visiting:Vec<String>,
}
impl Evaluator{
    fn global(&mut self,name:&str)->Result<Value,SemanticError>{
        if let Some(v)=self.values.get(name){return Ok(v.clone());}
        if self.visiting.iter().any(|n|n==name){return Err(error(format!("dependency cycle: {} -> {}",self.visiting.join(" -> "),name)));}
        let c=self.declarations.get(name).cloned().ok_or_else(||error(format!("{} is not a compile-time constant",name)))?;
        self.visiting.push(name.into());
        let v=evaluate(&c.value,c.type_annot.as_ref(),&mut |n|self.global(n)).map_err(|e|error(format!("{}: {}",name,e)))?;
        self.visiting.pop();self.values.insert(name.into(),v.clone());Ok(v)
    }
    fn lookup(&mut self,name:&str,scope:&Scope)->Result<Value,SemanticError>{
        if let Some(value)=scope.get(name){return value.clone().ok_or_else(||error(format!("{} is a runtime binding",name)));}
        if name.contains('.') && scope.contains_key(name.split('.').next().unwrap()){return Err(error(format!("{} is a runtime binding",name)));}
        self.global(name)
    }
    fn expression(&mut self,expr:&mut Expr,scope:&Scope)->Result<(),SemanticError>{
        if let Some(name)=qualified(expr){
            let root=name.split('.').next().unwrap();
            if !scope.get(&name).is_some_and(Option::is_none) && !(name.contains('.') && scope.contains_key(root)){
                if let Some(Some(v))=scope.get(&name){*expr=v.reference();return Ok(());}
                if self.declarations.contains_key(&name){*expr=self.global(&name)?.reference();return Ok(());}
            }
        }
        match expr {
            Expr::TypeOf(n) if scope.get(n).is_some_and(|v|v.is_some()) || !scope.contains_key(n) && self.declarations.contains_key(n)=>{
                *expr=Expr::StringLiteral(type_name(&self.lookup(n,scope)?.ty));
            },
            Expr::ConstValue{value,value_type}=>validate_value(value,value_type)?,
            Expr::Closure{params,body,..}=>{let mut local=scope.clone();for p in params{local.insert(p.name.clone(),None);}self.body(body,&local)?;},
            Expr::ListLiteral(items)=>for e in items{self.expression(e,scope)?;},
            Expr::DictLiteral(items)=>for (_,e) in items{self.expression(e,scope)?;},
            Expr::Compose{left,right}|Expr::BinaryOp{left,right,..}|Expr::Index{target:left,key:right}=>{self.expression(left,scope)?;self.expression(right,scope)?;},
            Expr::IndirectCall{target,args}=>{self.expression(target,scope)?;for e in args{self.expression(e,scope)?;}},
            Expr::FunctionCall{name,args}=>{
                for e in args.iter_mut(){self.expression(e,scope)?;}
                if !scope.contains_key(name) && self.declarations.contains_key(name) || scope.get(name).is_some_and(Option::is_some){
                    *expr=Expr::IndirectCall{target:Box::new(self.lookup(name,scope)?.reference()),args:args.clone()};
                }
            },
            Expr::MemberAccess{target,..}|Expr::Specialize{target,..}|Expr::RefCreate(target)|Expr::TailCall(target)=>self.expression(target,scope)?,
            _=>{}
        }
        Ok(())
    }
    fn body(&mut self,body:&mut [Statement],outer:&Scope)->Result<(),SemanticError>{
        let mut scope=outer.clone();
        for stmt in body {
            match stmt {
                Statement::Const(c)=>{
                    if matches!(c.name.as_str(),"True"|"False"){return Err(error(format!("reserved constant name {}",c.name)));}
                    let v=evaluate(&c.value,c.type_annot.as_ref(),&mut |n|self.lookup(n,&scope)).map_err(|e|error(format!("{}: {}",c.name,e)))?;
                    let name=c.name.clone();let type_annot=Some(v.ty.clone());let value=v.reference();
                    scope.insert(name.clone(),Some(v));
                    *stmt=Statement::VarDecl{name,is_mutable:false,type_annot,value};
                },
                Statement::VarDecl{name,value,..}=>{self.expression(value,&scope)?;scope.insert(name.clone(),None);},
                Statement::ExprStmt(e)|Statement::Spawn(e)|Statement::Throw(e)|Statement::MemorySet{value:e,..}|Statement::Signal{payload:e,..}=>self.expression(e,&scope)?,
                Statement::ForLoop{var_name,collection,body}=>{self.expression(collection,&scope)?;let mut local=scope.clone();local.insert(var_name.clone(),None);self.body(body,&local)?;},
                Statement::Match{target,cases}=>{self.expression(target,&scope)?;for case in cases{let mut local=scope.clone();pattern_scope(&case.pattern,&mut local);if let Some(g)=&mut case.guard{self.expression(g,&local)?;}self.body(&mut case.body,&local)?;}},
                Statement::TryCatch{try_body,error_name,catch_body}=>{self.body(try_body,&scope)?;let mut local=scope.clone();local.insert(error_name.clone(),None);self.body(catch_body,&local)?;},
                Statement::Terminate=>{}
            }
        }
        Ok(())
    }
}
fn pattern_scope(pattern:&Pattern,scope:&mut Scope){
    match pattern{Pattern::Identifier(n)=>{scope.insert(n.clone(),None);},Pattern::Constructor(_,parts)=>for p in parts{pattern_scope(p,scope);},_=>{}}
}
pub fn lower_program(program:&Program)->Result<Program,SemanticError>{
    let mut evaluator=Evaluator{declarations:HashMap::new(),values:HashMap::new(),visiting:vec![]};
    let mut declared=HashSet::new();
    for d in &program.declarations{
        let name=match d{TopLevelDecl::Const(c)=>{evaluator.declarations.insert(c.name.clone(),c.clone());&c.name},TopLevelDecl::Func(f)=>&f.name,TopLevelDecl::Routine(r)=>&r.name,TopLevelDecl::Loop(l)=>&l.name,TopLevelDecl::Type(t)=>&t.name,_=>continue};
        if !declared.insert(name.clone()) && program.declarations.iter().any(|d|matches!(d,TopLevelDecl::Const(c) if &c.name==name)){
            return Err(error(format!("duplicate constant or declaration {}",name)));
        }
    }
    let constructors=super::adt::Constructors::new(program);
    let mut names=evaluator.declarations.keys().cloned().collect::<Vec<_>>();names.sort();
    for name in &names {
        if matches!(name.as_str(),"True"|"False") || constructors.entries.contains_key(name) || constructors.ambiguous.contains(name){return Err(error(format!("reserved constant name {}",name)));}
        evaluator.global(name)?;
    }
    let mut result=program.clone();result.declarations.retain(|d|!matches!(d,TopLevelDecl::Const(_)));
    for d in &mut result.declarations {
        match d {
            TopLevelDecl::Func(f)=>{let scope=f.params.iter().map(|p|(p.name.clone(),None)).collect();evaluator.body(&mut f.body,&scope)?;},
            TopLevelDecl::Routine(r)=>{let scope=r.params.iter().map(|p|(p.name.clone(),None)).collect();evaluator.body(&mut r.body,&scope)?;},
            TopLevelDecl::Loop(l)=>evaluator.body(&mut l.body,&Scope::new())?,
            _=>{}
        }
    }
    Ok(result)
}

/// Cheap scan before semantic entry points that also accept raw parser ASTs.
pub fn needs_lowering(program:&Program)->bool{
    fn expr(e:&Expr)->bool{match e{
        Expr::Closure{body,..}=>body_has(body),
        Expr::ListLiteral(v)=>v.iter().any(expr),Expr::DictLiteral(v)=>v.iter().any(|(_,e)|expr(e)),
        Expr::FunctionCall{args,..}=>args.iter().any(expr),
        Expr::IndirectCall{target,args}=>expr(target)||args.iter().any(expr),
        Expr::Compose{left:target,right:key}|Expr::Index{target,key}|Expr::BinaryOp{left:target,right:key,..}=>expr(target)||expr(key),
        Expr::RefCreate(t)|Expr::TailCall(t)|Expr::MemberAccess{target:t,..}|Expr::Specialize{target:t,..}=>expr(t),
        _=>false
    }}
    fn body_has(body:&[Statement])->bool{body.iter().any(|s|match s{
        Statement::Const(_)=>true,
        Statement::ForLoop{collection,body,..}=>expr(collection)||body_has(body),
        Statement::Match{target,cases}=>expr(target)||cases.iter().any(|c|c.guard.as_ref().is_some_and(expr)||body_has(&c.body)),
        Statement::TryCatch{try_body,catch_body,..}=>body_has(try_body)||body_has(catch_body),
        Statement::VarDecl{value:e,..}|Statement::ExprStmt(e)|Statement::Spawn(e)|Statement::Throw(e)|Statement::MemorySet{value:e,..}|Statement::Signal{payload:e,..}=>expr(e),
        _=>false
    })}
    program.declarations.iter().any(|d|match d{TopLevelDecl::Const(_)=>true,TopLevelDecl::Func(f)=>body_has(&f.body),TopLevelDecl::Routine(r)=>body_has(&r.body),TopLevelDecl::Loop(l)=>body_has(&l.body),_=>false})
}
