use crate::core::database::SymbolIndex;
use crate::core::types::{
    CodeLocation, FunctionAnalysis, NoneSource, RaiseStatement, SingleFunctionAnalysis,
};
use crate::plugins::python::extractor::{self, CallContext};
use crate::plugins::python::parser::PythonParser;
use crate::plugins::python::resolver::PythonResolver;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TraversalError {
    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Max depth exceeded: {0}")]
    MaxDepthExceeded(usize),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Resolution error: {0}")]
    ResolutionError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Extractor error: {0}")]
    Extractor(#[from] extractor::ExtractorError),
}

pub struct Traverser {
    pub resolver: PythonResolver,
    pub symbol_index: Option<SymbolIndex>,
    pub max_depth: usize,
    parser: PythonParser,
}

#[derive(Debug, Clone)]
struct QueueItem {
    function_id: String,
    depth: usize,
    call_chain: Vec<String>,
}

impl Traverser {
    pub fn new(resolver: PythonResolver, max_depth: usize) -> Result<Self, TraversalError> {
        let parser = PythonParser::new().map_err(|e| TraversalError::ParseError(e.to_string()))?;
        Ok(Self {
            resolver,
            symbol_index: None,
            max_depth,
            parser,
        })
    }

    pub fn with_symbol_index(mut self, index: SymbolIndex) -> Self {
        self.symbol_index = Some(index);
        self
    }

    pub fn analyze_function(&mut self, function_id: &str) -> Result<FunctionAnalysis, TraversalError> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut all_raises: Vec<RaiseStatement> = Vec::new();
        let mut all_none_sources: Vec<NoneSource> = Vec::new();
        let mut call_chains: HashMap<String, Vec<String>> = HashMap::new();
        let mut functions_traced = 0;
        let mut max_call_depth = 0;

        let mut queue: VecDeque<QueueItem> = VecDeque::new();
        queue.push_back(QueueItem {
            function_id: function_id.to_string(),
            depth: 0,
            call_chain: vec![function_id.to_string()],
        });

        let mut root_location: Option<CodeLocation> = None;
        let mut root_signature = String::new();

        while let Some(item) = queue.pop_front() {
            if visited.contains(&item.function_id) {
                continue;
            }

            if item.depth > self.max_depth {
                continue;
            }

            visited.insert(item.function_id.clone());
            functions_traced += 1;
            max_call_depth = max_call_depth.max(item.depth);

            let resolved = match self.resolve_function(&item.function_id) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if item.depth == 0 {
                root_location = Some(CodeLocation::new(
                    resolved.file_path.clone(),
                    resolved.line_start,
                ));
                root_signature = format!(
                    "def {}(...)",
                    resolved.function_name
                );
            }

            let analysis = match self.analyze_single_function(&resolved, &item.function_id) {
                Ok(a) => a,
                Err(_) => continue,
            };

            for raise in analysis.raises {
                let chain_key = format!(
                    "{}@{}:{}",
                    raise.exception_type,
                    raise.raise_location.file.display(),
                    raise.raise_location.line
                );
                call_chains.insert(chain_key, item.call_chain.clone());
                all_raises.push(raise);
            }

            for none_source in analysis.none_sources {
                let chain_key = format!(
                    "{}@{}:{}",
                    none_source.kind.as_str(),
                    none_source.location.file.display(),
                    none_source.location.line
                );
                call_chains.insert(chain_key, item.call_chain.clone());
                all_none_sources.push(none_source);
            }

            for call in analysis.calls {
                if !visited.contains(&call) {
                    let mut new_chain = item.call_chain.clone();
                    new_chain.push(call.clone());
                    queue.push_back(QueueItem {
                        function_id: call,
                        depth: item.depth + 1,
                        call_chain: new_chain,
                    });
                }
            }
        }

        let location = root_location.unwrap_or_else(|| {
            CodeLocation::new(PathBuf::from("unknown"), 0)
        });

        let mut analysis = FunctionAnalysis::new(
            function_id.to_string(),
            root_signature,
            location,
        );
        analysis.raises = all_raises;
        analysis.none_sources = all_none_sources;
        analysis.functions_traced = functions_traced;
        analysis.call_depth = max_call_depth;
        analysis.call_chains = call_chains;

        Ok(analysis)
    }

    fn resolve_function(&mut self, function_id: &str) -> Result<ResolvedLocation, TraversalError> {
        if let Some(ref index) = self.symbol_index {
            if let Some(loc) = index.get(function_id) {
                return Ok(ResolvedLocation {
                    file_path: loc.file_path.clone(),
                    function_name: function_id.split('.').last().unwrap_or(function_id).to_string(),
                    line_start: loc.line_start,
                    line_end: loc.line_end,
                });
            }
        }

        match self.resolver.resolve(function_id) {
            Ok(resolved) => Ok(ResolvedLocation {
                file_path: resolved.file_path,
                function_name: resolved.function_name,
                line_start: resolved.line_start,
                line_end: resolved.line_end,
            }),
            Err(e) => Err(TraversalError::ResolutionError(e.to_string())),
        }
    }

    fn analyze_single_function(
        &mut self,
        resolved: &ResolvedLocation,
        function_id: &str,
    ) -> Result<SingleFunctionAnalysis, TraversalError> {
        let content = std::fs::read_to_string(&resolved.file_path)?;
        let tree = self
            .parser
            .parse_str(&content, &resolved.file_path)
            .map_err(|e| TraversalError::ParseError(e.to_string()))?;

        let mut raises = extractor::extract_raises_in_range(
            &tree,
            &content,
            &resolved.file_path,
            resolved.line_start,
            resolved.line_end,
        )?;

        let imports = extractor::extract_imports(&tree, &content);
        for raise in &mut raises {
            if let Some(def_location) = self.resolve_exception_definition(
                &raise.exception_type,
                &imports,
                &resolved.file_path,
            ) {
                raise.definition_location = Some(def_location);
                if raise.qualified_type == raise.exception_type {
                    if let Some(qualified) = self.qualify_exception_type(&raise.exception_type, &imports) {
                        raise.qualified_type = qualified;
                    }
                }
            }
        }

        let none_sources = extractor::extract_none_sources_in_range(
            &tree,
            &content,
            &resolved.file_path,
            resolved.line_start,
            resolved.line_end,
        )?;

        let call_context = CallContext {
            current_module: get_full_module_path(&resolved.file_path),
            current_class: extract_class_from_function_id(function_id),
            imports,
        };

        let calls = extractor::extract_calls_in_range_with_context(
            &tree,
            &content,
            resolved.line_start,
            resolved.line_end,
            &call_context,
        )?;

        Ok(SingleFunctionAnalysis {
            raises,
            none_sources,
            calls,
        })
    }

    fn resolve_exception_definition(
        &self,
        exc_type: &str,
        imports: &HashMap<String, String>,
        current_file: &PathBuf,
    ) -> Option<CodeLocation> {
        if is_builtin_exception(exc_type) {
            return None;
        }

        if let Some(loc) = self.lookup_in_index(exc_type) {
            return Some(loc);
        }

        if let Some(qualified) = imports.get(exc_type) {
            if let Some(loc) = self.lookup_in_index(qualified) {
                return Some(loc);
            }
        }

        if let Some(module) = get_module_from_path(current_file) {
            let qualified = format!("{}.{}", module, exc_type);
            if let Some(loc) = self.lookup_in_index(&qualified) {
                return Some(loc);
            }
        }

        None
    }

    fn lookup_in_index(&self, name: &str) -> Option<CodeLocation> {
        if let Some(ref index) = self.symbol_index {
            if let Some(loc) = index.get(name) {
                return Some(CodeLocation::new(loc.file_path.clone(), loc.line_start));
            }
        }
        None
    }

    fn qualify_exception_type(&self, exc_type: &str, imports: &HashMap<String, String>) -> Option<String> {
        imports.get(exc_type).cloned()
    }
}

fn is_builtin_exception(exc_type: &str) -> bool {
    let builtins = [
        "Exception", "BaseException", "ValueError", "TypeError", "KeyError",
        "IndexError", "AttributeError", "RuntimeError", "StopIteration",
        "GeneratorExit", "AssertionError", "ImportError", "ModuleNotFoundError",
        "OSError", "IOError", "FileNotFoundError", "PermissionError",
        "ConnectionError", "TimeoutError", "NameError", "UnboundLocalError",
        "LookupError", "ArithmeticError", "ZeroDivisionError", "OverflowError",
        "FloatingPointError", "SystemError", "SystemExit", "KeyboardInterrupt",
        "MemoryError", "RecursionError", "NotImplementedError", "SyntaxError",
        "IndentationError", "TabError", "UnicodeError", "UnicodeDecodeError",
        "UnicodeEncodeError", "UnicodeTranslateError", "Warning", "UserWarning",
        "DeprecationWarning", "PendingDeprecationWarning", "RuntimeWarning",
        "SyntaxWarning", "ResourceWarning", "FutureWarning", "ImportWarning",
        "BytesWarning", "EncodingWarning",
    ];
    builtins.contains(&exc_type)
}

fn get_module_from_path(path: &PathBuf) -> Option<String> {
    let file_stem = path.file_stem()?.to_str()?;
    if file_stem == "__init__" {
        path.parent()?.file_name()?.to_str().map(|s| s.to_string())
    } else {
        Some(file_stem.to_string())
    }
}

fn get_full_module_path(path: &PathBuf) -> String {
    let mut components = Vec::new();
    let mut current = path.clone();

    if let Some(stem) = current.file_stem() {
        let stem_str = stem.to_string_lossy();
        if stem_str != "__init__" {
            components.push(stem_str.to_string());
        }
    }

    current = current.parent().map(|p| p.to_path_buf()).unwrap_or_default();

    while current.join("__init__.py").exists() || current.file_name().map_or(false, |n| n == "src") {
        if let Some(name) = current.file_name() {
            components.push(name.to_string_lossy().to_string());
        }
        current = current.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        if current.as_os_str().is_empty() {
            break;
        }
    }

    components.reverse();
    components.join(".")
}

fn extract_class_from_function_id(function_id: &str) -> Option<String> {
    let parts: Vec<&str> = function_id.split('.').collect();
    if parts.len() >= 2 {
        let potential_class = parts[parts.len() - 2];
        if potential_class.chars().next().map_or(false, |c| c.is_uppercase()) {
            return Some(potential_class.to_string());
        }
    }
    None
}

#[derive(Debug)]
struct ResolvedLocation {
    file_path: PathBuf,
    function_name: String,
    line_start: u32,
    line_end: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_traverser_creation() {
        let resolver = PythonResolver::new(vec![], vec![]);
        let traverser = Traverser::new(resolver, 10);
        assert!(traverser.is_ok());
    }
}
