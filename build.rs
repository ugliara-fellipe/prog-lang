use std::{env,path::PathBuf,process::Command};
fn run(command:&mut Command){let status=command.status().expect("C toolchain required for FFI");assert!(status.success(),"native FFI build failed");}
fn main(){
    println!("cargo:rerun-if-changed=native/ffi.c");
    let out=PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let flags=Command::new("pkg-config").args(["--cflags","libffi"]).output().expect("pkg-config and libffi development headers required");
    assert!(flags.status.success(),"install libffi development headers");
    let mut cc=Command::new(env::var("CC").unwrap_or("cc".into()));
    cc.args(["-std=c11","-fPIC","-Wall","-Wextra","-Werror","-c","native/ffi.c","-o"]).arg(out.join("ffi.o"));
    for flag in String::from_utf8(flags.stdout).unwrap().split_whitespace(){cc.arg(flag);}
    run(&mut cc);
    run(Command::new(env::var("AR").unwrap_or("ar".into())).arg("crs").arg(out.join("liblia_ffi.a")).arg(out.join("ffi.o")));
    println!("cargo:rustc-link-search=native={}",out.display());
    println!("cargo:rustc-link-lib=static=lia_ffi");
    let libs=Command::new("pkg-config").args(["--libs","libffi"]).output().unwrap();
    assert!(libs.status.success(),"libffi linker flags unavailable");
    for flag in String::from_utf8(libs.stdout).unwrap().split_whitespace(){
        if let Some(name)=flag.strip_prefix("-l"){println!("cargo:rustc-link-lib={name}");}
        else if let Some(path)=flag.strip_prefix("-L"){println!("cargo:rustc-link-search=native={path}");}
    }
    if env::var("CARGO_CFG_TARGET_OS").unwrap()=="linux"{println!("cargo:rustc-link-lib=dl");}
    println!("cargo:rustc-link-lib=pthread");
}
