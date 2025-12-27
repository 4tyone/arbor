use crate::core::types::{CallGraph, FunctionAnalysis, ResolvedFunction};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Database not found at {0}")]
    NotFound(String),

    #[error("Database version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: String, found: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub python_version: String,
    pub venv_path: Option<String>,
    pub site_packages: Vec<String>,
    pub python_path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupingSuggestion {
    pub group_name: String,
    pub exceptions: Vec<String>,
    pub rationale: String,
    pub handler_example: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolLocation {
    pub file_path: PathBuf,
    pub line_start: u32,
    pub line_end: u32,
    pub is_method: bool,
    pub parent_class: Option<String>,
}

impl From<ResolvedFunction> for SymbolLocation {
    fn from(rf: ResolvedFunction) -> Self {
        Self {
            file_path: rf.file_path,
            line_start: rf.line_start,
            line_end: rf.line_end,
            is_method: rf.is_method,
            parent_class: rf.parent_class,
        }
    }
}

impl SymbolLocation {
    pub fn to_resolved(&self, name: &str) -> ResolvedFunction {
        ResolvedFunction {
            file_path: self.file_path.clone(),
            function_name: name.to_string(),
            line_start: self.line_start,
            line_end: self.line_end,
            is_method: self.is_method,
            parent_class: self.parent_class.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolIndex {
    pub symbols: HashMap<String, SymbolLocation>,
    pub indexed_at: Option<DateTime<Utc>>,
    pub file_hashes: HashMap<PathBuf, String>,
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, qualified_name: String, location: SymbolLocation) {
        self.symbols.insert(qualified_name, location);
    }

    pub fn get(&self, qualified_name: &str) -> Option<&SymbolLocation> {
        self.symbols.get(qualified_name)
    }

    pub fn contains(&self, qualified_name: &str) -> bool {
        self.symbols.contains_key(qualified_name)
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    pub fn mark_indexed(&mut self) {
        self.indexed_at = Some(Utc::now());
    }

    pub fn set_file_hash(&mut self, path: PathBuf, hash: String) {
        self.file_hashes.insert(path, hash);
    }

    pub fn file_changed(&self, path: &Path, current_hash: &str) -> bool {
        match self.file_hashes.get(path) {
            Some(stored_hash) => stored_hash != current_hash,
            None => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArborDatabase {
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub environment: Environment,
    pub symbol_index: SymbolIndex,
    pub functions: HashMap<String, FunctionAnalysis>,
    pub dependency_graph: CallGraph,
    pub grouping_suggestions: HashMap<String, GroupingSuggestion>,
}

impl ArborDatabase {
    pub fn new(environment: Environment) -> Self {
        let now = Utc::now();
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: now,
            updated_at: now,
            environment,
            symbol_index: SymbolIndex::new(),
            functions: HashMap::new(),
            dependency_graph: CallGraph::new(),
            grouping_suggestions: HashMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self, DatabaseError> {
        if !path.exists() {
            return Err(DatabaseError::NotFound(path.display().to_string()));
        }
        let content = std::fs::read_to_string(path)?;
        let db: Self = serde_json::from_str(&content)?;
        Ok(db)
    }

    pub fn save(&self, path: &Path) -> Result<(), DatabaseError> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn add_function(&mut self, analysis: FunctionAnalysis) {
        self.updated_at = Utc::now();
        self.functions.insert(analysis.function_id.clone(), analysis);
    }

    pub fn get_function(&self, id: &str) -> Option<&FunctionAnalysis> {
        self.functions.get(id)
    }

    pub fn remove_function(&mut self, id: &str) -> Option<FunctionAnalysis> {
        self.updated_at = Utc::now();
        self.functions.remove(id)
    }

    pub fn function_count(&self) -> usize {
        self.functions.len()
    }

    pub fn symbol_count(&self) -> usize {
        self.symbol_index.len()
    }

    pub fn resolve_from_index(&self, qualified_name: &str) -> Option<ResolvedFunction> {
        self.symbol_index
            .get(qualified_name)
            .map(|loc| loc.to_resolved(qualified_name))
    }
}
