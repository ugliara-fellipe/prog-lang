use lia::{parser::Parser, semantic::type_checker::TypeChecker};
fn check(source:&str)->bool { let p=Parser::new(source).unwrap().parse_program().unwrap(); TypeChecker::new().check_program(&p).is_ok() }
#[test]
fn generic_arguments_must_agree_and_bodies_remain_parametric() {
    for source in [
        "func same<T>(a:T,b:T)->T{a} routine main(){same(1,True)}",
        "func id<T>(x:T)->T{x} routine main(){id<String>(1)}",
        "func id<T>(x:T)->T{x} routine main(){id<Int,String>(1)}",
        "func bad<T>(x:T)->T{1}",
        "func bad<T>(x:T)->T{x+1}",
        "func bad(x:T)->T{x}",
        "routine main<T>(){}",
    ] { assert!(!check(source),"{source}"); }
}
#[test]
fn generic_values_and_struct_fields_are_instantiated() {
    assert!(check("func id<T>(x:T)->T{x} routine main(){let f:(Int)->Int=id f(3)}"));
    assert!(check("type Box<T>{value:T} func unwrap<T>(box:Box[T])->T{box.value}"));
    assert!(!check("type Box<T>{value:T} func bad(box:Box[Int])->String{box.value}"));
    assert!(!check("type Box<T>{value:T} routine bad(x:Box[Int,String]){}"));
}
