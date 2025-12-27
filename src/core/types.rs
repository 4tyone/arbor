use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeLocation {
    pub file: PathBuf,
    pub line: u32,
    pub column: Option<u32>,
    pub containing_function: Option<String>,
}

impl CodeLocation {
    pub fn new(file: PathBuf, line: u32) -> Self {
        Self {
            file,
            line,
            column: None,
            containing_function: None,
        }
    }

    pub fn with_column(mut self, column: u32) -> Self {
        self.column = Some(column);
        self
    }

    pub fn with_function(mut self, function: impl Into<String>) -> Self {
        self.containing_function = Some(function.into());
        self
    }

    pub fn to_string_short(&self) -> String {
        match self.column {
            Some(col) => format!("{}:{}:{}", self.file.display(), self.line, col),
            None => format!("{}:{}", self.file.display(), self.line),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaiseStatement {
    pub exception_type: String,
    pub qualified_type: String,
    pub raise_location: CodeLocation,
    pub definition_location: Option<CodeLocation>,
    pub condition: Option<String>,
    pub message: Option<String>,
}

impl RaiseStatement {
    pub fn new(exception_type: String, qualified_type: String, raise_location: CodeLocation) -> Self {
        Self {
            exception_type,
            qualified_type,
            raise_location,
            definition_location: None,
            condition: None,
            message: None,
        }
    }

    pub fn with_definition(mut self, location: CodeLocation) -> Self {
        self.definition_location = Some(location);
        self
    }

    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NoneSourceKind {
    ExplicitReturn,
    ImplicitReturn,
    FunctionCall,
    CollectionAccess,
    AttributeAccess,
    ConditionalExpr,
    MatchArm,
}

impl NoneSourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NoneSourceKind::ExplicitReturn => "explicit return",
            NoneSourceKind::ImplicitReturn => "implicit return",
            NoneSourceKind::FunctionCall => "function call",
            NoneSourceKind::CollectionAccess => "collection access",
            NoneSourceKind::AttributeAccess => "attribute access",
            NoneSourceKind::ConditionalExpr => "conditional expression",
            NoneSourceKind::MatchArm => "match arm",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoneSource {
    pub kind: NoneSourceKind,
    pub location: CodeLocation,
    pub source_definition: Option<CodeLocation>,
    pub condition: Option<String>,
}

impl NoneSource {
    pub fn new(kind: NoneSourceKind, location: CodeLocation) -> Self {
        Self {
            kind,
            location,
            source_definition: None,
            condition: None,
        }
    }

    pub fn with_source_definition(mut self, location: CodeLocation) -> Self {
        self.source_definition = Some(location);
        self
    }

    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionAnalysis {
    pub function_id: String,
    pub signature: String,
    pub location: CodeLocation,
    pub raises: Vec<RaiseStatement>,
    pub none_sources: Vec<NoneSource>,
    pub functions_traced: usize,
    pub call_depth: usize,
    pub call_chains: HashMap<String, Vec<String>>,
}

impl FunctionAnalysis {
    pub fn new(function_id: String, signature: String, location: CodeLocation) -> Self {
        Self {
            function_id,
            signature,
            location,
            raises: Vec::new(),
            none_sources: Vec::new(),
            functions_traced: 0,
            call_depth: 0,
            call_chains: HashMap::new(),
        }
    }

    pub fn exception_count(&self) -> usize {
        self.raises.len()
    }

    pub fn none_source_count(&self) -> usize {
        self.none_sources.len()
    }

    pub fn risk_level(&self) -> RiskLevel {
        let exc_count = self.exception_count();
        let none_count = self.none_source_count();

        if exc_count >= 10 || none_count >= 5 {
            RiskLevel::High
        } else if exc_count >= 5 || none_count >= 2 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "Low",
            RiskLevel::Medium => "Medium",
            RiskLevel::High => "High",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            RiskLevel::Low => "🟢",
            RiskLevel::Medium => "🟡",
            RiskLevel::High => "🔴",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub id: String,
    pub name: String,
    pub file: PathBuf,
    pub line_start: u32,
    pub line_end: u32,
    pub is_method: bool,
    pub parent_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedFunction {
    pub file_path: PathBuf,
    pub function_name: String,
    pub line_start: u32,
    pub line_end: u32,
    pub is_method: bool,
    pub parent_class: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CallGraph {
    pub calls: HashMap<String, Vec<String>>,
    pub called_by: HashMap<String, Vec<String>>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_call(&mut self, caller: &str, callee: &str) {
        self.calls
            .entry(caller.to_string())
            .or_default()
            .push(callee.to_string());

        self.called_by
            .entry(callee.to_string())
            .or_default()
            .push(caller.to_string());
    }

    pub fn get_callees(&self, function: &str) -> Option<&Vec<String>> {
        self.calls.get(function)
    }

    pub fn get_callers(&self, function: &str) -> Option<&Vec<String>> {
        self.called_by.get(function)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleFunctionAnalysis {
    pub raises: Vec<RaiseStatement>,
    pub none_sources: Vec<NoneSource>,
    pub calls: Vec<String>,
}

impl SingleFunctionAnalysis {
    pub fn new() -> Self {
        Self {
            raises: Vec::new(),
            none_sources: Vec::new(),
            calls: Vec::new(),
        }
    }
}

impl Default for SingleFunctionAnalysis {
    fn default() -> Self {
        Self::new()
    }
}
