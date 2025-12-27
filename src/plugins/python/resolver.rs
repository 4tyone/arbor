use crate::core::types::ResolvedFunction;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ResolveError {
    #[error("Module not found: {0}")]
    ModuleNotFound(String),

    #[error("Function not found: {0} in {1}")]
    FunctionNotFound(String, String),

    #[error("Invalid qualified name: {0}")]
    InvalidQualifiedName(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parser error: {0}")]
    ParserError(String),
}

#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub name: String,
    pub source_module: String,
    pub original_name: Option<String>,
}

pub struct PythonResolver {
    pub python_path: Vec<PathBuf>,
    pub site_packages: Vec<PathBuf>,
    pub venv_path: Option<PathBuf>,
    parser: Option<tree_sitter::Parser>,
    #[allow(dead_code)]
    import_cache: HashMap<PathBuf, Vec<ImportInfo>>,
}

impl PythonResolver {
    pub fn new(python_path: Vec<PathBuf>, site_packages: Vec<PathBuf>) -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .ok();

        Self {
            python_path,
            site_packages,
            venv_path: None,
            parser: Some(parser),
            import_cache: HashMap::new(),
        }
    }

    pub fn with_venv(mut self, venv_path: PathBuf) -> Self {
        self.venv_path = Some(venv_path);
        self
    }

    pub fn from_environment() -> Result<Self, ResolveError> {
        let python_path = Self::detect_python_path();
        let site_packages = Self::detect_site_packages()?;

        Ok(Self::new(python_path, site_packages))
    }

    fn detect_python_path() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Ok(cwd) = std::env::current_dir() {
            paths.push(cwd.clone());
            if cwd.join("src").exists() {
                paths.push(cwd.join("src"));
            }
        }

        if let Ok(pythonpath) = std::env::var("PYTHONPATH") {
            for p in pythonpath.split(':') {
                let path = PathBuf::from(p);
                if path.exists() {
                    paths.push(path);
                }
            }
        }

        paths
    }

    fn detect_site_packages() -> Result<Vec<PathBuf>, ResolveError> {
        let mut packages = Vec::new();

        if let Ok(cwd) = std::env::current_dir() {
            for venv_name in &[".venv", "venv", ".env", "env"] {
                let venv = cwd.join(venv_name);
                if venv.exists() {
                    if let Ok(sp) = Self::find_site_packages(&venv) {
                        packages.push(sp);
                        break;
                    }
                }
            }
        }

        if let Ok(virtual_env) = std::env::var("VIRTUAL_ENV") {
            let venv = PathBuf::from(virtual_env);
            if let Ok(sp) = Self::find_site_packages(&venv) {
                if !packages.contains(&sp) {
                    packages.push(sp);
                }
            }
        }

        Ok(packages)
    }

    pub fn find_site_packages(venv: &Path) -> Result<PathBuf, ResolveError> {
        let lib = venv.join("lib");
        if !lib.exists() {
            return Err(ResolveError::ModuleNotFound(format!(
                "No lib directory in venv: {}",
                venv.display()
            )));
        }

        for entry in std::fs::read_dir(&lib)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("python") {
                let site_packages = entry.path().join("site-packages");
                if site_packages.exists() {
                    return Ok(site_packages);
                }
            }
        }

        Err(ResolveError::ModuleNotFound(format!(
            "No site-packages found in venv: {}",
            venv.display()
        )))
    }

    pub fn resolve(&mut self, qualified_name: &str) -> Result<ResolvedFunction, ResolveError> {
        if qualified_name.is_empty() {
            return Err(ResolveError::InvalidQualifiedName(
                "Empty qualified name".to_string(),
            ));
        }

        let parts: Vec<&str> = qualified_name.split('.').collect();
        if parts.is_empty() {
            return Err(ResolveError::InvalidQualifiedName(qualified_name.to_string()));
        }

        for i in (1..=parts.len()).rev() {
            let module_parts = &parts[..i];
            let remaining = &parts[i..];

            if let Some(module_path) = self.resolve_module_path(module_parts) {
                let function_name = if remaining.is_empty() {
                    parts.last().unwrap().to_string()
                } else {
                    remaining.join(".")
                };

                if remaining.is_empty() {
                    if let Some(resolved) =
                        self.find_in_init_reexport(&module_path, parts.last().unwrap())?
                    {
                        return Ok(resolved);
                    }
                }

                if module_path.is_file() {
                    if let Some(resolved) = self.find_function_in_file(&module_path, &function_name)? {
                        return Ok(resolved);
                    }
                }

                let init_path = if module_path.is_dir() {
                    module_path.join("__init__.py")
                } else {
                    module_path.clone()
                };

                if init_path.exists() {
                    if let Some(resolved) =
                        self.find_in_init_reexport(&init_path, &function_name)?
                    {
                        return Ok(resolved);
                    }
                }
            }
        }

        let module_parts = &parts[..parts.len() - 1];
        let function_name = parts.last().unwrap();

        if let Some(module_path) = self.resolve_module_path(module_parts) {
            let file_path = if module_path.is_dir() {
                module_path.join("__init__.py")
            } else {
                module_path
            };

            if file_path.exists() {
                if let Some(resolved) = self.find_function_in_file(&file_path, function_name)? {
                    return Ok(resolved);
                }

                if let Some(resolved) = self.find_in_init_reexport(&file_path, function_name)? {
                    return Ok(resolved);
                }
            }
        }

        Err(ResolveError::FunctionNotFound(
            qualified_name.to_string(),
            "all search paths".to_string(),
        ))
    }

    fn resolve_module_path(&self, parts: &[&str]) -> Option<PathBuf> {
        let module_subpath = parts.join("/");

        let search_paths: Vec<&PathBuf> = self
            .python_path
            .iter()
            .chain(self.site_packages.iter())
            .collect();

        for base in search_paths {
            let dir_path = base.join(&module_subpath);
            if dir_path.is_dir() {
                let init_path = dir_path.join("__init__.py");
                if init_path.exists() {
                    return Some(dir_path);
                }
            }

            let file_path = base.join(format!("{}.py", module_subpath));
            if file_path.exists() {
                return Some(file_path);
            }

            if parts.len() > 1 {
                let parent_parts = &parts[..parts.len() - 1];
                let last = parts.last().unwrap();
                let parent_path = base.join(parent_parts.join("/"));

                if parent_path.is_dir() {
                    let file_in_parent = parent_path.join(format!("{}.py", last));
                    if file_in_parent.exists() {
                        return Some(file_in_parent);
                    }
                }
            }
        }

        None
    }

    fn find_function_in_file(
        &mut self,
        file_path: &Path,
        name: &str,
    ) -> Result<Option<ResolvedFunction>, ResolveError> {
        let content = std::fs::read_to_string(file_path)?;

        let parser = self.parser.as_mut().ok_or_else(|| {
            ResolveError::ParserError("Parser not initialized".to_string())
        })?;

        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| ResolveError::ParserError(format!("Failed to parse {}", file_path.display())))?;

        let (class_name, method_name) = if name.contains('.') {
            let parts: Vec<&str> = name.split('.').collect();
            (Some(parts[0]), parts.get(1).copied())
        } else {
            (None, None)
        };

        let result = if let Some(class) = class_name {
            if let Some(method) = method_name {
                self.find_method_in_class(&tree, &content, file_path, class, method)
            } else {
                self.find_class_definition(&tree, &content, file_path, class)
            }
        } else {
            self.find_top_level_function(&tree, &content, file_path, name)
                .or_else(|| self.find_class_definition(&tree, &content, file_path, name))
        };

        Ok(result)
    }

    fn find_top_level_function(
        &self,
        tree: &tree_sitter::Tree,
        content: &str,
        file_path: &Path,
        name: &str,
    ) -> Option<ResolvedFunction> {
        let root = tree.root_node();

        for i in 0..root.child_count() {
            let child = root.child(i)?;

            let func_node = if child.kind() == "function_definition" {
                Some(child)
            } else if child.kind() == "decorated_definition" {
                child.child_by_field_name("definition")
            } else {
                None
            };

            if let Some(func) = func_node {
                if func.kind() == "function_definition" {
                    if let Some(name_node) = func.child_by_field_name("name") {
                        let func_name = &content[name_node.byte_range()];
                        if func_name == name {
                            return Some(ResolvedFunction {
                                file_path: file_path.to_path_buf(),
                                function_name: name.to_string(),
                                line_start: func.start_position().row as u32 + 1,
                                line_end: func.end_position().row as u32 + 1,
                                is_method: false,
                                parent_class: None,
                            });
                        }
                    }
                }
            }
        }

        None
    }

    fn find_class_definition(
        &self,
        tree: &tree_sitter::Tree,
        content: &str,
        file_path: &Path,
        class_name: &str,
    ) -> Option<ResolvedFunction> {
        let root = tree.root_node();

        for i in 0..root.child_count() {
            let child = root.child(i)?;

            let class_node = if child.kind() == "class_definition" {
                Some(child)
            } else if child.kind() == "decorated_definition" {
                child
                    .child_by_field_name("definition")
                    .filter(|n| n.kind() == "class_definition")
            } else {
                None
            };

            if let Some(class) = class_node {
                if let Some(name_node) = class.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    if name == class_name {
                        return Some(ResolvedFunction {
                            file_path: file_path.to_path_buf(),
                            function_name: class_name.to_string(),
                            line_start: class.start_position().row as u32 + 1,
                            line_end: class.end_position().row as u32 + 1,
                            is_method: false,
                            parent_class: None,
                        });
                    }
                }
            }
        }

        None
    }

    fn find_method_in_class(
        &self,
        tree: &tree_sitter::Tree,
        content: &str,
        file_path: &Path,
        class_name: &str,
        method_name: &str,
    ) -> Option<ResolvedFunction> {
        let root = tree.root_node();

        for i in 0..root.child_count() {
            let child = root.child(i)?;

            let class_node = if child.kind() == "class_definition" {
                Some(child)
            } else if child.kind() == "decorated_definition" {
                child
                    .child_by_field_name("definition")
                    .filter(|n| n.kind() == "class_definition")
            } else {
                None
            };

            if let Some(class) = class_node {
                if let Some(name_node) = class.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    if name == class_name {
                        if let Some(body) = class.child_by_field_name("body") {
                            for j in 0..body.child_count() {
                                let member = body.child(j)?;

                                let method_node = if member.kind() == "function_definition" {
                                    Some(member)
                                } else if member.kind() == "decorated_definition" {
                                    member.child_by_field_name("definition")
                                } else {
                                    None
                                };

                                if let Some(method) = method_node {
                                    if method.kind() == "function_definition" {
                                        if let Some(mname_node) = method.child_by_field_name("name")
                                        {
                                            let mname = &content[mname_node.byte_range()];
                                            if mname == method_name {
                                                return Some(ResolvedFunction {
                                                    file_path: file_path.to_path_buf(),
                                                    function_name: format!(
                                                        "{}.{}",
                                                        class_name, method_name
                                                    ),
                                                    line_start: method.start_position().row as u32
                                                        + 1,
                                                    line_end: method.end_position().row as u32 + 1,
                                                    is_method: true,
                                                    parent_class: Some(class_name.to_string()),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn find_in_init_reexport(
        &mut self,
        init_path: &Path,
        name: &str,
    ) -> Result<Option<ResolvedFunction>, ResolveError> {
        if !init_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(init_path)?;
        let imports = self.parse_imports(&content, init_path)?;

        for import in imports {
            if import.name == name || import.original_name.as_deref() == Some(name) {
                let target_name = import.original_name.as_deref().unwrap_or(&import.name);

                let source_path = self.resolve_relative_import(init_path, &import.source_module);

                if let Some(path) = source_path {
                    if path.exists() {
                        if let Some(resolved) = self.find_function_in_file(&path, target_name)? {
                            return Ok(Some(ResolvedFunction {
                                function_name: name.to_string(),
                                ..resolved
                            }));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    fn parse_imports(
        &mut self,
        content: &str,
        file_path: &Path,
    ) -> Result<Vec<ImportInfo>, ResolveError> {
        let parser = self.parser.as_mut().ok_or_else(|| {
            ResolveError::ParserError("Parser not initialized".to_string())
        })?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| ResolveError::ParserError(format!("Failed to parse {}", file_path.display())))?;

        let mut imports = Vec::new();
        let root = tree.root_node();

        for i in 0..root.child_count() {
            if let Some(child) = root.child(i) {
                if child.kind() == "import_from_statement" {
                    let mut module_name = String::new();
                    let mut prefix = String::new();
                    let mut in_names = false;

                    for j in 0..child.child_count() {
                        if let Some(c) = child.child(j) {
                            match c.kind() {
                                "relative_import" => {
                                    for k in 0..c.child_count() {
                                        if let Some(rel_child) = c.child(k) {
                                            match rel_child.kind() {
                                                "import_prefix" => {
                                                    for d in 0..rel_child.child_count() {
                                                        if let Some(dot) = rel_child.child(d) {
                                                            if dot.kind() == "." {
                                                                prefix.push('.');
                                                            }
                                                        }
                                                    }
                                                }
                                                "dotted_name" => {
                                                    module_name = content[rel_child.byte_range()].to_string();
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                                "dotted_name" => {
                                    if !in_names {
                                        if module_name.is_empty() {
                                            module_name = content[c.byte_range()].to_string();
                                        }
                                    } else {
                                        let name = content[c.byte_range()].to_string();
                                        if !name.is_empty() {
                                            imports.push(ImportInfo {
                                                name,
                                                source_module: format!("{}{}", prefix, module_name),
                                                original_name: None,
                                            });
                                        }
                                    }
                                }
                                "aliased_import" => {
                                    let orig = c
                                        .child_by_field_name("name")
                                        .map(|n| content[n.byte_range()].to_string());
                                    let alias = c
                                        .child_by_field_name("alias")
                                        .map(|n| content[n.byte_range()].to_string());
                                    if let Some(name) = alias.or(orig.clone()) {
                                        imports.push(ImportInfo {
                                            name,
                                            source_module: format!("{}{}", prefix, module_name),
                                            original_name: orig,
                                        });
                                    }
                                }
                                "import" => {
                                    in_names = true;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        Ok(imports)
    }

    fn resolve_relative_import(&self, from_file: &Path, module: &str) -> Option<PathBuf> {
        let parent = from_file.parent()?;

        let dot_count = module.chars().take_while(|c| *c == '.').count();
        let module_rest = &module[dot_count..];

        let mut base = parent.to_path_buf();
        for _ in 1..dot_count {
            base = base.parent()?.to_path_buf();
        }

        if module_rest.is_empty() {
            Some(base.join("__init__.py"))
        } else {
            let subpath = module_rest.replace('.', "/");
            let file_path = base.join(format!("{}.py", subpath));
            if file_path.exists() {
                return Some(file_path);
            }

            let dir_path = base.join(&subpath);
            if dir_path.is_dir() {
                let init = dir_path.join("__init__.py");
                if init.exists() {
                    return Some(init);
                }
            }

            None
        }
    }

    pub fn search_paths(&self) -> Vec<&PathBuf> {
        self.python_path
            .iter()
            .chain(self.site_packages.iter())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_qualified_name() {
        let parts: Vec<&str> = "requests.get".split('.').collect();
        assert_eq!(parts, vec!["requests", "get"]);

        let parts: Vec<&str> = "requests.sessions.Session.request".split('.').collect();
        assert_eq!(parts, vec!["requests", "sessions", "Session", "request"]);
    }

    #[test]
    fn test_empty_resolver() {
        let mut resolver = PythonResolver::new(vec![], vec![]);
        let result = resolver.resolve("nonexistent.function");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_relative_imports() {
        let content = r#"from .api import get_data, post_data"#;

        let mut resolver = PythonResolver::new(vec![], vec![]);
        let fake_path = PathBuf::from("/fake/__init__.py");
        let imports = resolver.parse_imports(content, &fake_path).unwrap();

        assert_eq!(imports.len(), 2);

        let get_data_import = imports.iter().find(|i| i.name == "get_data").unwrap();
        assert_eq!(get_data_import.source_module, ".api");

        let post_data_import = imports.iter().find(|i| i.name == "post_data").unwrap();
        assert_eq!(post_data_import.source_module, ".api");
    }

    #[test]
    fn test_resolve_relative_module() {
        let resolver = PythonResolver::new(vec![], vec![]);
        let init_path = PathBuf::from("/project/mypackage/__init__.py");

        let result = resolver.resolve_relative_import(&init_path, ".api");
        println!("Resolved .api from {:?}: {:?}", init_path, result);
    }
}
