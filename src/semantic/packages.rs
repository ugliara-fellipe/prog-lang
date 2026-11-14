use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use crate::ast::*;
use crate::parser::Parser;
use crate::semantic::SemanticError;

#[derive(Clone)]
pub struct PackageManager {
    base_dir: PathBuf,
    loaded_packages: HashMap<String, Program>,
    loading_stack: HashSet<String>,
    local_declarations: HashMap<String, HashSet<String>>,
}

impl PackageManager {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
            loaded_packages: HashMap::new(),
            loading_stack: HashSet::new(),
            local_declarations: HashMap::new(),
        }
    }

    /// Load and resolve all imports recursively for a main program, returning a consolidated Program
    pub fn resolve_source(&mut self,mut program:Program,path:&Path)->Result<Program,SemanticError>{
        crate::ffi::set_source(&mut program,path);
        self.resolve_program(program)
    }

    pub fn resolve_program(&mut self, mut main_prog: Program) -> Result<Program, SemanticError> {
        Self::foreign_paths(&mut main_prog,&self.base_dir)?;
        let resolved=self.resolve_imports(main_prog)?;
        let resolved=super::builtins::augment(&resolved)?.unwrap_or(resolved);
        let constants=crate::semantic::constants::lower_program(&resolved)?;
        crate::semantic::pipes::lower_program(&constants)
    }

    fn foreign_paths(program:&mut Program,base:&Path)->Result<(),SemanticError>{
        for decl in &mut program.declarations{if let TopLevelDecl::Func(f)=decl{if let Some(foreign)=&mut f.foreign{if let Some(path)=&mut foreign.library{
            if !Path::new(path).is_absolute(){let absolute=std::env::current_dir().map_err(|e|SemanticError::PackageError(e.to_string()))?.join(base).join(&path);*path=absolute.to_string_lossy().into_owned();}
        }}}}Ok(())
    }

    pub(crate) fn language_prelude()->Result<Program,SemanticError>{
        let mut manager=Self::new(".");
        let imports=["option","result","dict"].into_iter().map(|n|ImportDecl{package:n.into(),kind:ImportKind::All}).collect();
        manager.resolve_imports(Program{imports,declarations:vec![]})
    }

    fn resolve_imports(&mut self, main_prog: Program) -> Result<Program, SemanticError> {
        let mut final_declarations = Vec::new();
        let mut imported_decls = Vec::new();

        for imp in &main_prog.imports {
            let pkg_decls = self.resolve_import(imp)?;
            for decl in pkg_decls {
                if let Some(name) = Self::decl_name(&decl) {
                    if let Some(previous) = imported_decls.iter().find(|previous| Self::decl_name(previous) == Some(name)) {
                        if previous == &decl { continue; }
                        return Err(SemanticError::PackageError(format!("Ambiguous imported declaration '{}'", name)));
                    }
                }
                imported_decls.push(decl);
            }
        }

        // Imported declarations come first, then main program's non-import declarations
        final_declarations.extend(imported_decls);
        for decl in main_prog.declarations {
            if !matches!(decl, TopLevelDecl::Import(_)) {
                final_declarations.push(decl);
            }
        }

        Ok(Program {
            imports: main_prog.imports,
            declarations: final_declarations,
        })
    }

    fn resolve_import(&mut self, imp: &ImportDecl) -> Result<Vec<TopLevelDecl>, SemanticError> {
        let pkg_name = &imp.package;
        let pkg_prog = self.load_package(pkg_name)?;
        let local = self.local_declarations.get(pkg_name).cloned().unwrap_or_default();

        let selected: HashSet<String> = match &imp.kind {
            ImportKind::All if crate::stdlib::bundled_package(pkg_name).is_some() => pkg_prog.declarations.iter().filter_map(|d|if matches!(d,TopLevelDecl::Type(_)){Self::decl_name(d)}else{None}).map(str::to_owned).collect(),
            ImportKind::All => pkg_prog.declarations.iter().filter_map(Self::decl_name).map(str::to_owned).collect(),
            ImportKind::Single(name) => [name.clone()].into_iter().collect(),
            ImportKind::Items(names) => names.iter().cloned().collect(),
        };
        for name in &selected {
            if !pkg_prog.declarations.iter().any(|decl| Self::decl_name(decl) == Some(name)) {
                return Err(SemanticError::PackageError(format!("Item '{}' not found in package '{}'", name, pkg_name)));
            }
        }
        let mut names: HashMap<String, String> = pkg_prog.declarations.iter().filter_map(Self::decl_name)
            .filter(|name| local.contains(*name)).map(|name| (name.into(), format!("{}.{}", pkg_name, name))).collect();
        let mut enum_names:HashMap<String,String>=pkg_prog.declarations.iter().filter_map(|d|match d {TopLevelDecl::Type(t) if local.contains(&t.name) && !t.variants.is_empty()=>Some((t.name.clone(),names[&t.name].clone())),_=>None}).collect();
        let mut variant_names:HashMap<String,Vec<String>>=HashMap::new();
        for decl in &pkg_prog.declarations {if let TopLevelDecl::Type(t)=decl {if let Some(full)=enum_names.get(&t.name) {for v in &t.variants {
            let qualified=format!("{}.{}",full,v.name);names.insert(format!("{}.{}",t.name,v.name),qualified.clone());variant_names.entry(v.name.clone()).or_default().push(qualified);
        }}}}
        let local_constructors:HashSet<_>=variant_names.keys().cloned().collect();
        for (short,full) in variant_names {if full.len()==1 {names.insert(short,full[0].clone());}}
        // Resolve each module's implicit prelude in its own lexical scope.
        // A constructor/type in the importing program must not retarget it.
        for (ty,package,variants) in [("Option","option",vec!["Some","None"]),("Result","result",vec!["Ok","Err"])]{
            if !local.contains(ty){
                let owner=format!("{}.{}",package,ty);
                enum_names.entry(ty.into()).or_insert(owner.clone());
                names.entry(ty.into()).or_insert(owner.clone());
                for variant in &variants{names.entry(format!("{}.{}",ty,variant)).or_insert(format!("{}.{}",owner,variant));}
            }
            for variant in variants{
                if !local.contains(variant) && !local_constructors.contains(variant){
                    names.entry(variant.into()).or_insert(format!("{}.{}.{}",package,ty,variant));
                }
            }
        }

        let mut result = Vec::new();
        for decl in &pkg_prog.declarations {
            let Some(name) = Self::decl_name(decl) else { continue; };
            // Keep qualified dependencies even for selective imports. Public
            // aliases point at bodies whose references stay inside the package.
            if !local.contains(name) { result.push(decl.clone()); continue; }
            let mut qualified = decl.clone();
            Self::qualify_decl_types(&mut qualified,&enum_names);
            match &mut qualified {
                TopLevelDecl::Func(f) => { Self::qualify_body(&mut f.body, &names, &f.params.iter().map(|p| p.name.clone()).collect()); f.name = names[name].clone(); }
                TopLevelDecl::Routine(r) => { Self::qualify_body(&mut r.body, &names, &r.params.iter().map(|p| p.name.clone()).collect()); r.name = names[name].clone(); }
                TopLevelDecl::Loop(l) => { Self::qualify_body(&mut l.body, &names, &HashSet::new()); l.name = names[name].clone(); }
                TopLevelDecl::Type(t) => t.name = names[name].clone(),
                TopLevelDecl::Const(c)=>{Self::qualify_expr(&mut c.value,&names,&HashSet::new());c.name=names[name].clone();},
                _ => {}
            }
            if selected.contains(name) {
                let mut alias = qualified.clone();
                match &mut alias {
                    TopLevelDecl::Func(f) => {
                        f.name = name.into();
                        if pkg_name == "std" || crate::stdlib::is_bundled_native_function(&names[name]) {
                            f.body = vec![Statement::ExprStmt(Expr::FunctionCall {
                                name: names[name].clone(),
                                args: f.params.iter().map(|p| Expr::Variable(p.name.clone())).collect(),
                            })];
                        }
                    }
                    TopLevelDecl::Routine(r) => r.name = name.into(),
                    TopLevelDecl::Loop(l) => l.name = name.into(),
                    TopLevelDecl::Type(t) => t.name = name.into(),
                    TopLevelDecl::Const(c)=>c.name=name.into(),
                    _ => {}
                }
                result.push(alias);
            }
            result.push(qualified);
        }
        Ok(result)
    }

    fn decl_name(decl: &TopLevelDecl) -> Option<&str> {
        match decl { TopLevelDecl::Const(c)=>Some(&c.name), TopLevelDecl::Func(f) => Some(&f.name), TopLevelDecl::Routine(r) => Some(&r.name), TopLevelDecl::Loop(l) => Some(&l.name), TopLevelDecl::Type(t) => Some(&t.name), _ => None }
    }

    fn qualify_expr(expr: &mut Expr, names: &HashMap<String, String>, bound: &HashSet<String>) {
        match expr {
            Expr::Closure{params,body,..}=>{
                let mut local=bound.clone();local.extend(params.iter().map(|p|p.name.clone()));
                Self::qualify_body(body,names,&local);
            },
            Expr::TypeOf(name) | Expr::Variable(name) => if !bound.contains(name) { if let Some(full) = names.get(name) { *name = full.clone(); } },
            Expr::FunctionCall { name, args } => { if !bound.contains(name) { if let Some(full) = names.get(name) { *name = full.clone(); } } for arg in args { Self::qualify_expr(arg, names, bound); } }
            Expr::IndirectCall { target, args } => { Self::qualify_expr(target, names, bound); for arg in args { Self::qualify_expr(arg, names, bound); } }
            Expr::Compose{left,right} | Expr::BinaryOp { left, right, .. } => { Self::qualify_expr(left, names, bound); Self::qualify_expr(right, names, bound); }
            Expr::MemberAccess { target, .. } | Expr::Specialize { target, .. } | Expr::RefCreate(target) | Expr::TailCall(target) => Self::qualify_expr(target, names, bound),
            Expr::ListLiteral(items) => for item in items { Self::qualify_expr(item, names, bound); },
            Expr::DictLiteral(items)=>for (_,v) in items{Self::qualify_expr(v,names,bound);},
            Expr::Index{target,key}=>{Self::qualify_expr(target,names,bound);Self::qualify_expr(key,names,bound);},
            _ => {}
        }
    }

    fn qualify_pattern(pattern:&mut Pattern,names:&HashMap<String,String>) {
        if let Pattern::Constructor(name,parts)=pattern {if let Some(full)=names.get(name){*name=full.clone();}for part in parts {Self::qualify_pattern(part,names);}}
    }
    fn qualify_type(ty:&mut TypeSpec,names:&HashMap<String,String>) {
        match ty {
            TypeSpec::Primitive(n)=>{if let Some(full)=names.get(n){*n=full.clone();}},
            TypeSpec::Generic(n,args)=>{if let Some(full)=names.get(n){*n=full.clone();}for arg in args {Self::qualify_type(arg,names);}},
            TypeSpec::Function(args,r)=>{for arg in args {Self::qualify_type(arg,names);}Self::qualify_type(r,names);},
            TypeSpec::Ref(inner)=>Self::qualify_type(inner,names),
        }
    }
    fn qualify_expr_types(expr:&mut Expr,names:&HashMap<String,String>){
        match expr{
            Expr::Closure{params,return_type,body}=>{for p in params{Self::qualify_type(&mut p.param_type,names);}Self::qualify_type(return_type,names);Self::qualify_body_types(body,names);},
            Expr::FunctionCall{args,..}|Expr::ListLiteral(args)=>for arg in args{Self::qualify_expr_types(arg,names);},
            Expr::DictLiteral(items)=>for (_,v) in items{Self::qualify_expr_types(v,names);},
            Expr::Index{target,key}=>{Self::qualify_expr_types(target,names);Self::qualify_expr_types(key,names);},
            Expr::IndirectCall{target,args}=>{Self::qualify_expr_types(target,names);for arg in args{Self::qualify_expr_types(arg,names);}},
            Expr::Specialize{target,type_args}=>{for t in type_args{Self::qualify_type(t,names);}Self::qualify_expr_types(target,names);},
            Expr::RefCreate(t)|Expr::TailCall(t)|Expr::MemberAccess{target:t,..}=>Self::qualify_expr_types(t,names),
            Expr::Compose{left,right}|Expr::BinaryOp{left,right,..}=>{Self::qualify_expr_types(left,names);Self::qualify_expr_types(right,names);},_=>{}
        }
    }

    fn qualify_body_types(body:&mut [Statement],names:&HashMap<String,String>) {
        for stmt in body {match stmt {
            Statement::Const(c)=>{if let Some(t)=&mut c.type_annot{Self::qualify_type(t,names);}Self::qualify_expr_types(&mut c.value,names);},
            Statement::VarDecl{type_annot,value,..}=>{if let Some(ty)=type_annot{Self::qualify_type(ty,names);}Self::qualify_expr_types(value,names);},
            Statement::ExprStmt(e)|Statement::Spawn(e)|Statement::Throw(e)|Statement::MemorySet{value:e,..}|Statement::Signal{payload:e,..}=>Self::qualify_expr_types(e,names),
            Statement::ForLoop{collection,body,..}=>{Self::qualify_expr_types(collection,names);Self::qualify_body_types(body,names);},
            Statement::Match{target,cases}=>{Self::qualify_expr_types(target,names);for case in cases{if let Some(g)=&mut case.guard{Self::qualify_expr_types(g,names);}Self::qualify_body_types(&mut case.body,names);}},
            Statement::TryCatch{try_body,catch_body,..}=>{Self::qualify_body_types(try_body,names);Self::qualify_body_types(catch_body,names);},
            _=>{}
        }}
    }
    fn qualify_decl_types(decl:&mut TopLevelDecl,names:&HashMap<String,String>) {
        match decl {
            TopLevelDecl::Const(c)=>{if let Some(t)=&mut c.type_annot{Self::qualify_type(t,names);}Self::qualify_expr_types(&mut c.value,names);},
            TopLevelDecl::Func(f)=>{for p in &mut f.params {Self::qualify_type(&mut p.param_type,names);}if let Some(r)=&mut f.return_type{Self::qualify_type(r,names);}Self::qualify_body_types(&mut f.body,names);},
            TopLevelDecl::Routine(r)=>{for p in &mut r.params{Self::qualify_type(&mut p.param_type,names);}Self::qualify_body_types(&mut r.body,names);},
            TopLevelDecl::Loop(l)=>Self::qualify_body_types(&mut l.body,names),
            TopLevelDecl::Type(t)=>{for f in &mut t.fields{Self::qualify_type(&mut f.field_type,names);}for v in &mut t.variants{for f in &mut v.fields{Self::qualify_type(f,names);}}},
            _=>{}
        }
    }

    fn pattern_names(pattern: &Pattern, bound: &mut HashSet<String>) {
        match pattern { Pattern::Identifier(name) => { bound.insert(name.clone()); }, Pattern::Constructor(_, parts) => for part in parts { Self::pattern_names(part, bound); }, _ => {} }
    }

    fn qualify_body(body: &mut [Statement], names: &HashMap<String, String>, outer: &HashSet<String>) {
        let mut bound = outer.clone();
        for stmt in body {
            match stmt {
                Statement::Const(c)=>{Self::qualify_expr(&mut c.value,names,&bound);bound.insert(c.name.clone());},
                Statement::VarDecl { name, value, .. } => { Self::qualify_expr(value, names, &bound); bound.insert(name.clone()); }
                Statement::ExprStmt(e) | Statement::Spawn(e) | Statement::Throw(e) | Statement::MemorySet { value: e, .. } => Self::qualify_expr(e, names, &bound),
                Statement::Signal { loop_name, payload } => { if let Some(full) = names.get(loop_name) { *loop_name = full.clone(); } Self::qualify_expr(payload, names, &bound); }
                Statement::ForLoop { var_name, collection, body } => { Self::qualify_expr(collection, names, &bound); let mut local = bound.clone(); local.insert(var_name.clone()); Self::qualify_body(body, names, &local); }
                Statement::Match { target, cases } => { Self::qualify_expr(target, names, &bound); for case in cases { Self::qualify_pattern(&mut case.pattern,names); let mut local = bound.clone(); Self::pattern_names(&case.pattern, &mut local); if let Some(g)=&mut case.guard{Self::qualify_expr(g,names,&local);} Self::qualify_body(&mut case.body, names, &local); } }
                Statement::TryCatch { try_body, error_name, catch_body } => { Self::qualify_body(try_body, names, &bound); let mut local = bound.clone(); local.insert(error_name.clone()); Self::qualify_body(catch_body, names, &local); }
                Statement::Terminate => {}
            }
        }
    }

    fn load_package(&mut self, pkg_name: &str) -> Result<Program, SemanticError> {
        if let Some(prog) = self.loaded_packages.get(pkg_name) {
            return Ok(prog.clone());
        }

        if self.loading_stack.contains(pkg_name) {
            return Err(SemanticError::PackageError(format!(
                "Circular package import detected involving '{}'",
                pkg_name
            )));
        }

        self.loading_stack.insert(pkg_name.to_string());

        let source = if let Some(source)=crate::stdlib::bundled_package(pkg_name) {source.to_owned()} else if pkg_name=="std" && self.find_package_file(pkg_name).is_err(){include_str!("../../programs/std.lia").to_owned()} else {
            let pkg_file=self.find_package_file(pkg_name)?;
            fs::read_to_string(&pkg_file).map_err(|e|SemanticError::PackageError(format!("Failed to read package file '{}': {}",pkg_file.display(),e)))?
        };

        let mut parser = Parser::new(&source).map_err(|e| {
            SemanticError::PackageError(format!(
                "Failed to parse package '{}': {}",
                pkg_name, e
            ))
        })?;

        let mut raw_prog = parser.parse_program().map_err(|e| {
            SemanticError::PackageError(format!(
                "Syntax error in package '{}': {}",
                pkg_name, e
            ))
        })?;
        if crate::stdlib::bundled_package(pkg_name).is_some(){
            // Prelude binders must not collide with user types named T, E, etc.
            for decl in &mut raw_prog.declarations{
                let parameters=match decl{
                    TopLevelDecl::Func(f)=>&mut f.type_params,
                    TopLevelDecl::Type(t)=>&mut t.type_params,
                    _=>continue
                };
                let rename:HashMap<String,String>=parameters.iter().map(|n|(n.clone(),format!("{}builtin_{}",char::from(0),n))).collect();
                for parameter in parameters{*parameter=rename[parameter].clone();}
                Self::qualify_decl_types(decl,&rename);
            }
        }
        let package_base=if crate::stdlib::bundled_package(pkg_name).is_some() || pkg_name=="std"{self.base_dir.clone()}else{self.find_package_file(pkg_name)?.parent().unwrap_or(&self.base_dir).to_path_buf()};
        if crate::stdlib::bundled_package(pkg_name).is_none(){
            if let Ok(path)=self.find_package_file(pkg_name){crate::ffi::set_source(&mut raw_prog,&path);}
        }
        Self::foreign_paths(&mut raw_prog,&package_base)?;
        self.local_declarations.insert(pkg_name.to_owned(), raw_prog.declarations.iter().filter_map(Self::decl_name).map(str::to_owned).collect());

        // `std` is privileged because its declarations map to runtime code.
        // Keep that surface closed to primitives that are certified finite.
        if pkg_name == "std" {
            crate::stdlib::validate_standard_library(&raw_prog)
                .map_err(SemanticError::PackageError)?;
        }

        // Recursively resolve imports of the imported package
        let resolved_prog = self.resolve_imports(raw_prog)?;

        self.loading_stack.remove(pkg_name);
        self.loaded_packages.insert(pkg_name.to_string(), resolved_prog.clone());

        Ok(resolved_prog)
    }

    fn find_package_file(&self, pkg_name: &str) -> Result<PathBuf, SemanticError> {
        // Quoted imports may start with digits, but they are still package
        // stems rather than paths. Restricting the character set prevents an
        // import from escaping the configured package directory.
        if pkg_name.is_empty()
            || !pkg_name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err(SemanticError::PackageError(format!(
                "Invalid package name '{}': use letters, digits, and underscores only",
                pkg_name
            )));
        }
        // Look for a flat package file first, then a module file.
        let candidates = [
            self.base_dir.join(format!("{}.lia", pkg_name)),
            self.base_dir.join(pkg_name).join("mod.lia"),
        ];

        for path in &candidates {
            if path.exists() && path.is_file() {
                return Ok(path.clone());
            }
        }

        Err(SemanticError::PackageError(format!(
            "Package '{}' could not be found. Searched paths: {:?}",
            pkg_name,
            candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()
        )))
    }
}

