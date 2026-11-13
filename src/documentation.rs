//! Markdown API documentation from the source AST (before constant lowering).
use std::fmt::Write;
use crate::ast::*;

pub fn type_signature(ty:&TypeSpec)->String {
    match ty {
        TypeSpec::Primitive(n)=>n.clone(),
        TypeSpec::Generic(n,args)=>format!("{}[{}]",n,args.iter().map(type_signature).collect::<Vec<_>>().join(", ")),
        TypeSpec::Ref(inner)=>format!("Ref[{}]",type_signature(inner)),
        TypeSpec::Function(args,result)=>format!("({}) -> {}",args.iter().map(type_signature).collect::<Vec<_>>().join(", "),type_signature(result)),
    }
}
fn parameters(params:&[Param])->String{
    params.iter().map(|p|format!("{}: {}",p.name,type_signature(&p.param_type))).collect::<Vec<_>>().join(", ")
}
fn generics(params:&[String])->String{
    if params.is_empty(){String::new()}else{format!("<{}>",params.join(", "))}
}
fn constant_expression(expr:&Expr)->String{
    match expr {
        Expr::IntLiteral(n)=>n.to_string(),
        Expr::StringLiteral(s)=>serde_json::to_string(s).expect("strings serialize"),
        Expr::Variable(n)=>n.clone(),
        Expr::TypeOf(n)=>format!("type {}",n),
        Expr::ListLiteral(items)=>format!("[{}]",items.iter().map(constant_expression).collect::<Vec<_>>().join(", ")),
        Expr::DictLiteral(items)=>format!("{{{}}}",items.iter().map(|(k,v)|format!("{}: {}",serde_json::to_string(k).expect("keys serialize"),constant_expression(v))).collect::<Vec<_>>().join(", ")),
        Expr::MemberAccess{target,field}=>format!("{}.{}",constant_expression(target),field),
        Expr::BinaryOp{op,left,right}=>format!("({} {} {})",constant_expression(left),op,constant_expression(right)),
        Expr::ConstValue{value,..}=>constant_expression(value),
        // Source documentation is generated only after constant validation.
        _=>unreachable!("invalid compile-time initializer"),
    }
}
fn declaration_signature(decl:&TopLevelDecl)->Option<String>{
    Some(match decl {
        TopLevelDecl::Func(f)=>format!("func {}{}({}){}",f.name,generics(&f.type_params),parameters(&f.params),f.return_type.as_ref().map(|t|format!(" -> {}",type_signature(t))).unwrap_or_default()),
        TopLevelDecl::Routine(r)=>format!("routine {}{}({})",r.name,generics(&r.type_params),parameters(&r.params)),
        TopLevelDecl::Loop(l)=>format!("loop {}()",l.name),
        TopLevelDecl::Const(c)=>format!("const {}{} = {}",c.name,c.type_annot.as_ref().map(|t|format!(": {}",type_signature(t))).unwrap_or_default(),constant_expression(&c.value)),
        TopLevelDecl::Type(t)=>if t.variants.is_empty(){
            format!("type {}{} {{\n{}\n}}",t.name,generics(&t.type_params),t.fields.iter().map(|f|format!("    {}: {}",f.name,type_signature(&f.field_type))).collect::<Vec<_>>().join(",\n"))
        }else{
            format!("type {}{} = {}",t.name,generics(&t.type_params),t.variants.iter().map(variant_signature).collect::<Vec<_>>().join(" | "))
        },
        TopLevelDecl::Import(_)=>return None,
    })
}
fn variant_signature(v:&EnumVariant)->String{
    if v.fields.is_empty(){v.name.clone()}else{format!("{}({})",v.name,v.fields.iter().map(type_signature).collect::<Vec<_>>().join(", "))}
}
fn fence(out:&mut String,source:&str){
    let longest=source.split(|c|c!='`').map(str::len).max().unwrap_or(0);
    let marker="`".repeat(3.max(longest+1));
    writeln!(out,"{}lia\n{}\n{}\n",marker,source,marker).unwrap();
}
fn text(out:&mut String,documentation:&str){
    if !documentation.is_empty(){out.push_str(documentation);out.push_str("\n\n");}
}
fn inline(value:&str)->String{
    let longest=value.split(|c|c!='`').map(str::len).max().unwrap_or(0);
    let marker="`".repeat(1.max(longest+1));
    let value=value.replace(['\n','\r']," ");
    let padded=if value.starts_with('`') || value.ends_with('`'){format!(" {} ",value)}else{value};
    format!("{}{}{}",marker,padded,marker)
}

/// Render only this file's declarations, in source order. Import resolution,
/// constant evaluation and semantic validation belong to the compiler driver.
/// Comments use Markdown verbatim, so fenced examples and lists are preserved.
pub fn render_markdown(program:&Program,title:&str)->String{
    let mut out=String::new();
    writeln!(out,"# API: {}\n",inline(title)).unwrap();
    if !program.imports.is_empty(){
        out.push_str("## Imports\n\n");
        for import in &program.imports {
            let source=format!("import {}{}",import.package,match &import.kind{ImportKind::All=>String::new(),ImportKind::Single(n)=>format!(".{}",n),ImportKind::Items(names)=>format!(".{{{}}}",names.join(", "))});
            writeln!(out,"- {}",inline(&source)).unwrap();
        }
        out.push('\n');
    }
    for decl in &program.declarations {
        let (kind,name,documentation)=match decl {
            TopLevelDecl::Func(f)=>("func",&f.name,&f.documentation),
            TopLevelDecl::Routine(r)=>("routine",&r.name,&r.documentation),
            TopLevelDecl::Loop(l)=>("loop",&l.name,&l.documentation),
            TopLevelDecl::Type(t)=>("type",&t.name,&t.documentation),
            TopLevelDecl::Const(c)=>("const",&c.name,&c.documentation),
            TopLevelDecl::Import(_)=>continue,
        };
        writeln!(out,"## {} {}\n",kind,inline(name)).unwrap();
        if let Some(signature)=declaration_signature(decl){fence(&mut out,&signature);}
        text(&mut out,documentation);
        match decl {
            TopLevelDecl::Type(t)=>{
                for field in &t.fields {
                    writeln!(out,"### Field {}\n",inline(&field.name)).unwrap();
                    fence(&mut out,&format!("{}: {}",field.name,type_signature(&field.field_type)));
                    text(&mut out,&field.documentation);
                }
                for variant in &t.variants {
                    writeln!(out,"### Variant {}\n",inline(&variant.name)).unwrap();
                    fence(&mut out,&variant_signature(variant));text(&mut out,&variant.documentation);
                }
            },
            TopLevelDecl::Func(f)=>{parameter_docs(&mut out,&f.params);if let Some(ty)=&f.return_type{writeln!(out,"### Returns\n\n{}\n",inline(&type_signature(ty))).unwrap();}},
            TopLevelDecl::Routine(r)=>parameter_docs(&mut out,&r.params),
            _=>{}
        }
    }
    out
}
fn parameter_docs(out:&mut String,params:&[Param]){
    if params.is_empty(){return;}
    out.push_str("### Parameters\n\n");
    for p in params{writeln!(out,"- {}: {}",inline(&p.name),inline(&type_signature(&p.param_type))).unwrap();}
    out.push('\n');
}
