//! A trusted C ABI boundary. No Lia pointers, callbacks or references escape.
use std::ffi::{CStr,CString};
use crate::{ast::*,runtime::actor::RuntimeValue as V};
fn kind(ty:&TypeSpec)->Option<u8>{match ty{TypeSpec::Primitive(n)=>match n.as_str(){"Unit"=>Some(0),"Int"=>Some(1),"Bool"=>Some(2),"String"=>Some(3),_=>None},_=>None}}
pub fn validate(f:&FuncDecl)->Result<(),String>{
    let foreign=f.foreign.as_ref().ok_or("missing foreign declaration")?;
    if crate::stdlib::is_total_standard_function(&f.name)||crate::stdlib::is_bundled_native_function(&f.name)||crate::stdlib::is_prelude_function(&f.name){return Err("foreign declarations cannot replace built-in functions".into());}
    declared_cost(foreign)?;
    if foreign.abi!="C"{return Err(format!("unsupported foreign ABI {}",foreign.abi));}
    if !f.body.is_empty() || !f.type_params.is_empty(){return Err("foreign declarations cannot have a body or type parameters".into());}
    if foreign.symbol.is_empty() || foreign.symbol.contains(char::from(0)) || foreign.library.as_ref().is_some_and(|p|p.is_empty()||p.contains(char::from(0))){return Err("invalid foreign library or symbol".into());}
    if f.params.len()>32{return Err("foreign declarations accept at most 32 arguments".into());}
    let mut names=std::collections::HashSet::new();
    for p in &f.params{
        if !names.insert(&p.name){return Err(format!("duplicate foreign parameter {}",p.name));}
        if !matches!(kind(&p.param_type),Some(1..=3)){return Err(format!("unsupported foreign parameter type {:?}",p.param_type));}
    }
    if f.return_type.as_ref().and_then(kind).is_none(){return Err("foreign return type must be Int, Bool, String or Unit".into());}
    Ok(())
}
extern "C"{
    fn lia_ffi_call(path:*const libc::c_char,symbol:*const libc::c_char,count:usize,kinds:*const u8,numbers:*const i64,strings:*const *const libc::c_char,result_kind:u8,number:*mut i64,text:*mut *mut libc::c_char,error:*mut libc::c_char,capacity:usize)->libc::c_int;
}
pub fn invoke(f:&FuncDecl,args:&[V])->Result<V,String>{
    validate(f)?;let foreign=f.foreign.as_ref().unwrap();
    if args.len()!=f.params.len(){return Err(format!("{} expects {} arguments, got {}",f.name,f.params.len(),args.len()));}
    let path=foreign.library.as_ref().map(|p|CString::new(p.as_str()).map_err(|e|e.to_string())).transpose()?;
    let symbol=CString::new(foreign.symbol.as_str()).map_err(|e|e.to_string())?;
    let mut kinds=Vec::new();let mut numbers=Vec::new();let mut strings=Vec::new();
    for (param,value) in f.params.iter().zip(args){
        let k=kind(&param.param_type).unwrap();kinds.push(k);
        match (k,value){
            (1,V::Int(n))=>{numbers.push(*n);strings.push(None);},
            (2,V::Bool(b))=>{numbers.push(i64::from(*b));strings.push(None);},
            (3,V::String(s))=>{numbers.push(0);strings.push(Some(CString::new(s.as_str()).map_err(|_|format!("{}: String contains NUL",param.name))?.into_bytes_with_nul()));},
            _=>return Err(format!("invalid foreign value for {}",param.name))
        }
    }
    let pointers=strings.iter_mut().map(|s|s.as_mut().map_or(std::ptr::null(),|s|s.as_mut_ptr().cast::<libc::c_char>() as *const libc::c_char)).collect::<Vec<_>>();
    let result_kind=kind(f.return_type.as_ref().unwrap()).unwrap();
    let mut number=0;let mut text=std::ptr::null_mut();let mut error=[0 as libc::c_char;1024];
    // libffi uses validated types; mutable private buffers own String copies.
    // Native code must honor the declared ABI and valid pointer contract.
    let status=unsafe{lia_ffi_call(path.as_ref().map_or(std::ptr::null(),|p|p.as_ptr()),symbol.as_ptr(),args.len(),kinds.as_ptr(),numbers.as_ptr(),pointers.as_ptr(),result_kind,&mut number,&mut text,error.as_mut_ptr(),error.len())};
    if status!=0{return Err(format!("{}: {}",f.name,unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy()));}
    match result_kind{
        0=>Ok(V::None),1=>Ok(V::Int(number)),
        2 if number==0 || number==1=>Ok(V::Bool(number==1)),
        2=>Err(format!("{} returned an invalid C Bool",f.name)),
        3=>{
            if text.is_null(){return Err("null native String".into());}
            let value=unsafe{CStr::from_ptr(text)}.to_str().map(|s|V::String(s.to_owned())).map_err(|_|format!("{} returned invalid UTF-8",f.name));
            unsafe{libc::free(text.cast())};value
        },
        _=>unreachable!()
    }
}

/// Costs are programmer assertions in the existing polynomial cost model.
pub fn declared_cost(foreign:&ForeignDecl)->Result<crate::semantic::complexity::Complexity,String>{
    use crate::semantic::complexity::Complexity as C;
    let Some(cost)=&foreign.cost else{return Ok(C::Unknown);};
    let normalized=cost.chars().filter(|c|!c.is_whitespace()).collect::<String>();
    match normalized.as_str(){
        "O(1)"=>Ok(C::Constant),"O(N)"=>Ok(C::Polynomial(1)),
        _=>if let Some(power)=normalized.strip_prefix("O(N^").and_then(|s|s.strip_suffix(')')){
            match power.parse::<u32>(){Ok(n) if n>0=>Ok(C::Polynomial(n)),_=>Err(format!("invalid foreign cost {}; expected O(1), O(N), or O(N^k) with positive integer k",cost))}
        }else{Err(format!("unsupported foreign cost {}; expected O(1), O(N), or O(N^k)",cost))}
    }
}
/// The annotation is a trusted bound for the complete call, including copies.
pub fn effective_cost(f:&FuncDecl)->crate::semantic::complexity::Complexity{
    declared_cost(f.foreign.as_ref().expect("foreign declaration")).expect("validated foreign cost")
}
pub fn set_source(program:&mut Program,path:&std::path::Path){
    let source=std::fs::canonicalize(path).unwrap_or_else(|_|path.to_path_buf()).to_string_lossy().into_owned();
    for decl in &mut program.declarations{if let TopLevelDecl::Func(f)=decl{if let Some(foreign)=&mut f.foreign{if foreign.source.is_none(){foreign.source=Some(source.clone());}}}}
}
