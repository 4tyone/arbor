use crate::core::database::{SymbolIndex, SymbolLocation};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Error, Debug)]
pub enum IndexerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parser error: {0}")]
    Parser(String),

    #[error("Walkdir error: {0}")]
    WalkDir(#[from] walkdir::Error),
}

pub struct Indexer {
    parser: tree_sitter::Parser,
}

impl Indexer {
    pub fn new() -> Result<Self, IndexerError> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .map_err(|e| IndexerError::Parser(e.to_string()))?;
        Ok(Self { parser })
    }

    pub fn index_directories(&mut self, directories: &[PathBuf]) -> Result<SymbolIndex, IndexerError> {
        let mut index = SymbolIndex::new();

        for dir in directories {
            self.index_directory(dir, &mut index)?;
        }

        index.mark_indexed();
        Ok(index)
    }

    fn index_directory(&mut self, dir: &Path, index: &mut SymbolIndex) -> Result<(), IndexerError> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| !Self::is_venv_dir(e.path()))
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "py") {
                if let Err(e) = self.index_file(path, dir, index) {
                    eprintln!("Warning: Failed to index {}: {}", path.display(), e);
                }
            }
        }

        Ok(())
    }

    fn is_venv_dir(path: &Path) -> bool {
        if !path.is_dir() {
            return false;
        }

        let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let skip_dirs = ["__pycache__", ".git", "node_modules", ".tox", ".nox",
                         ".mypy_cache", ".pytest_cache", ".ruff_cache", ".eggs"];

        if skip_dirs.contains(&dir_name) || dir_name.ends_with(".egg-info") {
            return true;
        }

        // Detect venv ROOT directories (not site-packages inside them)
        // A venv root has pyvenv.cfg or bin/python - site-packages doesn't have these
        let has_pyvenv_cfg = path.join("pyvenv.cfg").exists();
        let has_bin_python = path.join("bin/python").exists();
        let has_scripts_python = path.join("Scripts/python.exe").exists();

        has_pyvenv_cfg || has_bin_python || has_scripts_python
    }

    fn index_file(
        &mut self,
        path: &Path,
        base_dir: &Path,
        index: &mut SymbolIndex,
    ) -> Result<(), IndexerError> {
        let content = std::fs::read_to_string(path)?;
        let hash = Self::hash_content(&content);

        let tree = self
            .parser
            .parse(&content, None)
            .ok_or_else(|| IndexerError::Parser(format!("Failed to parse {}", path.display())))?;

        let module_path = Self::path_to_module(path, base_dir);

        self.extract_symbols(&tree, &content, path, &module_path, index);

        index.set_file_hash(path.to_path_buf(), hash);

        Ok(())
    }

    fn extract_symbols(
        &self,
        tree: &tree_sitter::Tree,
        content: &str,
        file_path: &Path,
        module_path: &str,
        index: &mut SymbolIndex,
    ) {
        let root = tree.root_node();
        self.extract_from_node(root, content, file_path, module_path, None, index);
    }

    fn extract_from_node(
        &self,
        node: tree_sitter::Node,
        content: &str,
        file_path: &Path,
        module_path: &str,
        current_class: Option<&str>,
        index: &mut SymbolIndex,
    ) {
        match node.kind() {
            "function_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let qualified_name = match current_class {
                        Some(class) => format!("{}.{}.{}", module_path, class, name),
                        None => format!("{}.{}", module_path, name),
                    };

                    let location = SymbolLocation {
                        file_path: file_path.to_path_buf(),
                        line_start: node.start_position().row as u32 + 1,
                        line_end: node.end_position().row as u32 + 1,
                        is_method: current_class.is_some(),
                        parent_class: current_class.map(|s| s.to_string()),
                    };

                    index.add(qualified_name, location);
                }
            }
            "class_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let class_name = &content[name_node.byte_range()];
                    let qualified_name = format!("{}.{}", module_path, class_name);

                    let location = SymbolLocation {
                        file_path: file_path.to_path_buf(),
                        line_start: node.start_position().row as u32 + 1,
                        line_end: node.end_position().row as u32 + 1,
                        is_method: false,
                        parent_class: None,
                    };

                    index.add(qualified_name, location);

                    if let Some(body) = node.child_by_field_name("body") {
                        for i in 0..body.child_count() {
                            if let Some(child) = body.child(i) {
                                self.extract_from_node(
                                    child,
                                    content,
                                    file_path,
                                    module_path,
                                    Some(class_name),
                                    index,
                                );
                            }
                        }
                    }
                }
            }
            "decorated_definition" => {
                if let Some(definition) = node.child_by_field_name("definition") {
                    self.extract_from_node(
                        definition,
                        content,
                        file_path,
                        module_path,
                        current_class,
                        index,
                    );
                }
            }
            "module" => {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        self.extract_from_node(
                            child,
                            content,
                            file_path,
                            module_path,
                            current_class,
                            index,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn path_to_module(path: &Path, base_dir: &Path) -> String {
        let relative = path
            .strip_prefix(base_dir)
            .unwrap_or(path);

        let mut module_parts: Vec<&str> = relative
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();

        if let Some(last) = module_parts.last_mut() {
            if last.ends_with(".py") {
                *last = &last[..last.len() - 3];
            }
        }

        if module_parts.last() == Some(&"__init__") {
            module_parts.pop();
        }

        module_parts.join(".")
    }

    fn hash_content(content: &str) -> String {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

impl Default for Indexer {
    fn default() -> Self {
        Self::new().expect("Failed to create indexer")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_module() {
        let base = PathBuf::from("/project/src");

        assert_eq!(
            Indexer::path_to_module(&PathBuf::from("/project/src/mypackage/api.py"), &base),
            "mypackage.api"
        );

        assert_eq!(
            Indexer::path_to_module(&PathBuf::from("/project/src/mypackage/__init__.py"), &base),
            "mypackage"
        );

        assert_eq!(
            Indexer::path_to_module(&PathBuf::from("/project/src/utils.py"), &base),
            "utils"
        );
    }

    #[test]
    fn test_hash_content() {
        let hash1 = Indexer::hash_content("hello world");
        let hash2 = Indexer::hash_content("hello world");
        let hash3 = Indexer::hash_content("different");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
}
