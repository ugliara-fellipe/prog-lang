use crate::ast::TypeSpec;
pub fn is_json_type(ty:&TypeSpec)->bool{is_json_shape(ty,false)}
pub fn is_json_shape(ty:&TypeSpec,unknown:bool)->bool{
    match ty{
        TypeSpec::Primitive(n)=>matches!(n.as_str(),"Int"|"String"|"Bool"|"Json") || unknown && n=="Unknown",
        TypeSpec::Generic(n,args) if matches!(n.as_str(),"List"|"Dict") && args.len()==1=>is_json_shape(&args[0],unknown),
        _=>false
    }
}
