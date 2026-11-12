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
}

impl PackageManager {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
            loaded_packages: HashMap::new(),
            loading_stack: HashSet::new(),
        }
    }

    /// Load and resolve all imports recursively for a main program, returning a consolidated Program
    pub fn resolve_program(&mut self, main_prog: Program) -> Result<Program, SemanticError> {
        let mut final_declarations = Vec::new();
        let mut imported_decls = Vec::new();

        for imp in &main_prog.imports {
            let pkg_decls = self.resolve_import(imp)?;
            imported_decls.extend(pkg_decls);
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

        let mut matched_decls = Vec::new();

        match &imp.kind {
            ImportKind::All => {
                for decl in &pkg_prog.declarations {
                    match decl {
                        TopLevelDecl::Func(f) => {
                            // Expose both as qualified pkg.name and unqualified name
                            let mut qualified = f.clone();
                            qualified.name = format!("{}.{}", pkg_name, f.name);
                            matched_decls.push(TopLevelDecl::Func(qualified));
                            matched_decls.push(TopLevelDecl::Func(f.clone()));
                        }
                        TopLevelDecl::Routine(r) => {
                            let mut qualified = r.clone();
                            qualified.name = format!("{}.{}", pkg_name, r.name);
                            matched_decls.push(TopLevelDecl::Routine(qualified));
                            matched_decls.push(TopLevelDecl::Routine(r.clone()));
                        }
                        TopLevelDecl::Loop(l) => {
                            let mut qualified = l.clone();
                            qualified.name = format!("{}.{}", pkg_name, l.name);
                            matched_decls.push(TopLevelDecl::Loop(qualified));
                            matched_decls.push(TopLevelDecl::Loop(l.clone()));
                        }
                        TopLevelDecl::Type(t) => {
                            let mut qualified = t.clone();
                            qualified.name = format!("{}.{}", pkg_name, t.name);
                            matched_decls.push(TopLevelDecl::Type(qualified));
                            matched_decls.push(TopLevelDecl::Type(t.clone()));
                        }
                        _ => {}
                    }
                }
            }
            ImportKind::Single(item_name) => {
                let mut found = false;
                for decl in &pkg_prog.declarations {
                    match decl {
                        TopLevelDecl::Func(f) if &f.name == item_name => {
                            let mut qualified = f.clone();
                            qualified.name = format!("{}.{}", pkg_name, f.name);
                            matched_decls.push(TopLevelDecl::Func(qualified));
                            matched_decls.push(TopLevelDecl::Func(f.clone()));
                            found = true;
                        }
                        TopLevelDecl::Routine(r) if &r.name == item_name => {
                            let mut qualified = r.clone();
                            qualified.name = format!("{}.{}", pkg_name, r.name);
                            matched_decls.push(TopLevelDecl::Routine(qualified));
                            matched_decls.push(TopLevelDecl::Routine(r.clone()));
                            found = true;
                        }
                        TopLevelDecl::Loop(l) if &l.name == item_name => {
                            let mut qualified = l.clone();
                            qualified.name = format!("{}.{}", pkg_name, l.name);
                            matched_decls.push(TopLevelDecl::Loop(qualified));
                            matched_decls.push(TopLevelDecl::Loop(l.clone()));
                            found = true;
                        }
                        TopLevelDecl::Type(t) if &t.name == item_name => {
                            let mut qualified = t.clone();
                            qualified.name = format!("{}.{}", pkg_name, t.name);
                            matched_decls.push(TopLevelDecl::Type(qualified));
                            matched_decls.push(TopLevelDecl::Type(t.clone()));
                            found = true;
                        }
                        _ => {}
                    }
                }
                if !found {
                    return Err(SemanticError::PackageError(format!(
                        "Item '{}' not found in package '{}'",
                        item_name, pkg_name
                    )));
                }
            }
            ImportKind::Items(items) => {
                for item_name in items {
                    let mut found = false;
                    for decl in &pkg_prog.declarations {
                        match decl {
                            TopLevelDecl::Func(f) if &f.name == item_name => {
                                let mut qualified = f.clone();
                                qualified.name = format!("{}.{}", pkg_name, f.name);
                                matched_decls.push(TopLevelDecl::Func(qualified));
                                matched_decls.push(TopLevelDecl::Func(f.clone()));
                                found = true;
                            }
                            TopLevelDecl::Routine(r) if &r.name == item_name => {
                                let mut qualified = r.clone();
                                qualified.name = format!("{}.{}", pkg_name, r.name);
                                matched_decls.push(TopLevelDecl::Routine(qualified));
                                matched_decls.push(TopLevelDecl::Routine(r.clone()));
                                found = true;
                            }
                            TopLevelDecl::Loop(l) if &l.name == item_name => {
                                let mut qualified = l.clone();
                                qualified.name = format!("{}.{}", pkg_name, l.name);
                                matched_decls.push(TopLevelDecl::Loop(qualified));
                                matched_decls.push(TopLevelDecl::Loop(l.clone()));
                                found = true;
                            }
                            TopLevelDecl::Type(t) if &t.name == item_name => {
                                let mut qualified = t.clone();
                                qualified.name = format!("{}.{}", pkg_name, t.name);
                                matched_decls.push(TopLevelDecl::Type(qualified));
                                matched_decls.push(TopLevelDecl::Type(t.clone()));
                                found = true;
                            }
                            _ => {}
                        }
                    }
                    if !found {
                        return Err(SemanticError::PackageError(format!(
                            "Item '{}' not found in package '{}'",
                            item_name, pkg_name
                        )));
                    }
                }
            }
        }

        Ok(matched_decls)
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

        // `std` is privileged because its declarations map to runtime code.
        // Keep that surface closed to primitives that are certified finite.
        if pkg_name == "std" {
            crate::stdlib::validate_standard_library(&raw_prog)
                .map_err(SemanticError::PackageError)?;
        }

        // Recursively resolve imports of the imported package
        let mut pkg_manager = PackageManager::new(&self.base_dir);
        let resolved_prog = pkg_manager.resolve_program(raw_prog)?;

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
        // Look for:
        // 1. base_dir / <pkg_name>.proglang
        // 2. base_dir / <pkg_name>.prolang
        // 3. base_dir / <pkg_name> / mod.proglang
        // 4. base_dir / <pkg_name> / mod.prolang
        let candidates = [
            self.base_dir.join(format!("{}.proglang", pkg_name)),
            self.base_dir.join(format!("{}.prolang", pkg_name)),
            self.base_dir.join(pkg_name).join("mod.proglang"),
            self.base_dir.join(pkg_name).join("mod.prolang"),
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

