use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use crate::ast::*;
use crate::parser::Parser;
use crate::semantic::SemanticError;

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
    pub fn resolve_program(&mut self, main_prog: Program) -> Result<Program, SemanticError> {
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
            ImportKind::All => pkg_prog.declarations.iter().filter_map(Self::decl_name).map(str::to_owned).collect(),
            ImportKind::Single(name) => [name.clone()].into_iter().collect(),
            ImportKind::Items(names) => names.iter().cloned().collect(),
        };
        for name in &selected {
            if !pkg_prog.declarations.iter().any(|decl| Self::decl_name(decl) == Some(name)) {
                return Err(SemanticError::PackageError(format!("Item '{}' not found in package '{}'", name, pkg_name)));
            }
        }
        let names: HashMap<String, String> = pkg_prog.declarations.iter().filter_map(Self::decl_name)
            .filter(|name| local.contains(*name)).map(|name| (name.into(), format!("{}.{}", pkg_name, name))).collect();
        let mut result = Vec::new();
        for decl in &pkg_prog.declarations {
            let Some(name) = Self::decl_name(decl) else { continue; };
            // Keep qualified dependencies even for selective imports. Public
            // aliases point at bodies whose references stay inside the package.
            if !local.contains(name) { result.push(decl.clone()); continue; }
            let mut qualified = decl.clone();
            match &mut qualified {
                TopLevelDecl::Func(f) => { Self::qualify_body(&mut f.body, &names, &f.params.iter().map(|p| p.name.clone()).collect()); f.name = names[name].clone(); }
                TopLevelDecl::Routine(r) => { Self::qualify_body(&mut r.body, &names, &r.params.iter().map(|p| p.name.clone()).collect()); r.name = names[name].clone(); }
                TopLevelDecl::Loop(l) => { Self::qualify_body(&mut l.body, &names, &HashSet::new()); l.name = names[name].clone(); }
                TopLevelDecl::Type(t) => t.name = names[name].clone(),
                _ => {}
            }
            if selected.contains(name) {
                let mut alias = qualified.clone();
                match &mut alias {
                    TopLevelDecl::Func(f) => {
                        f.name = name.into();
                        if pkg_name == "std" {
                            f.body = vec![Statement::ExprStmt(Expr::FunctionCall {
                                name: names[name].clone(),
                                args: f.params.iter().map(|p| Expr::Variable(p.name.clone())).collect(),
                            })];
                        }
                    }
                    TopLevelDecl::Routine(r) => r.name = name.into(),
                    TopLevelDecl::Loop(l) => l.name = name.into(),
                    TopLevelDecl::Type(t) => t.name = name.into(),
                    _ => {}
                }
                result.push(alias);
            }
            result.push(qualified);
        }
        Ok(result)
    }

    fn decl_name(decl: &TopLevelDecl) -> Option<&str> {
        match decl { TopLevelDecl::Func(f) => Some(&f.name), TopLevelDecl::Routine(r) => Some(&r.name), TopLevelDecl::Loop(l) => Some(&l.name), TopLevelDecl::Type(t) => Some(&t.name), _ => None }
    }

    fn qualify_expr(expr: &mut Expr, names: &HashMap<String, String>, bound: &HashSet<String>) {
        match expr {
            Expr::Variable(name) => if !bound.contains(name) { if let Some(full) = names.get(name) { *name = full.clone(); } },
            Expr::FunctionCall { name, args } => { if !bound.contains(name) { if let Some(full) = names.get(name) { *name = full.clone(); } } for arg in args { Self::qualify_expr(arg, names, bound); } }
            Expr::IndirectCall { target, args } => { Self::qualify_expr(target, names, bound); for arg in args { Self::qualify_expr(arg, names, bound); } }
            Expr::BinaryOp { left, right, .. } => { Self::qualify_expr(left, names, bound); Self::qualify_expr(right, names, bound); }
            Expr::MemberAccess { target, .. } | Expr::Specialize { target, .. } | Expr::RefCreate(target) => Self::qualify_expr(target, names, bound),
            Expr::ListLiteral(items) => for item in items { Self::qualify_expr(item, names, bound); },
            _ => {}
        }
    }

    fn pattern_names(pattern: &Pattern, bound: &mut HashSet<String>) {
        match pattern { Pattern::Identifier(name) => { bound.insert(name.clone()); }, Pattern::Constructor(_, parts) => for part in parts { Self::pattern_names(part, bound); }, _ => {} }
    }

    fn qualify_body(body: &mut [Statement], names: &HashMap<String, String>, outer: &HashSet<String>) {
        let mut bound = outer.clone();
        for stmt in body {
            match stmt {
                Statement::VarDecl { name, value, .. } => { Self::qualify_expr(value, names, &bound); bound.insert(name.clone()); }
                Statement::ExprStmt(e) | Statement::Spawn(e) | Statement::Throw(e) | Statement::MemorySet { value: e, .. } => Self::qualify_expr(e, names, &bound),
                Statement::Signal { loop_name, payload } => { if let Some(full) = names.get(loop_name) { *loop_name = full.clone(); } Self::qualify_expr(payload, names, &bound); }
                Statement::ForLoop { var_name, collection, body } => { Self::qualify_expr(collection, names, &bound); let mut local = bound.clone(); local.insert(var_name.clone()); Self::qualify_body(body, names, &local); }
                Statement::Match { target, cases } => { Self::qualify_expr(target, names, &bound); for case in cases { let mut local = bound.clone(); Self::pattern_names(&case.pattern, &mut local); Self::qualify_body(&mut case.body, names, &local); } }
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

        let pkg_file = self.find_package_file(pkg_name)?;
        let source = fs::read_to_string(&pkg_file).map_err(|e| {
            SemanticError::PackageError(format!(
                "Failed to read package file '{}': {}",
                pkg_file.display(),
                e
            ))
        })?;

        let mut parser = Parser::new(&source).map_err(|e| {
            SemanticError::PackageError(format!(
                "Failed to parse package '{}': {}",
                pkg_name, e
            ))
        })?;

        let raw_prog = parser.parse_program().map_err(|e| {
            SemanticError::PackageError(format!(
                "Syntax error in package '{}': {}",
                pkg_name, e
            ))
        })?;
        self.local_declarations.insert(pkg_name.to_owned(), raw_prog.declarations.iter().filter_map(Self::decl_name).map(str::to_owned).collect());

        // `std` is privileged because its declarations map to runtime code.
        // Keep that surface closed to primitives that are certified finite.
        if pkg_name == "std" {
            crate::stdlib::validate_standard_library(&raw_prog)
                .map_err(SemanticError::PackageError)?;
        }

        // Recursively resolve imports of the imported package
        let resolved_prog = self.resolve_program(raw_prog)?;

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

