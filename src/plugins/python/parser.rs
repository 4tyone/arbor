use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("Failed to initialize parser")]
    InitializationFailed,

    #[error("Parse failed for file: {0}")]
    ParseFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Query error: {0}")]
    QueryError(String),
}

pub struct PythonParser {
    parser: tree_sitter::Parser,
}

impl PythonParser {
    pub fn new() -> Result<Self, ParserError> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .map_err(|_| ParserError::InitializationFailed)?;

        Ok(Self { parser })
    }

    pub fn parse_file(&mut self, path: &Path) -> Result<tree_sitter::Tree, ParserError> {
        let content = std::fs::read_to_string(path)?;
        self.parse_str(&content, path)
    }

    pub fn parse_str(&mut self, content: &str, path: &Path) -> Result<tree_sitter::Tree, ParserError> {
        self.parser
            .parse(content, None)
            .ok_or_else(|| ParserError::ParseFailed(path.display().to_string()))
    }
}
