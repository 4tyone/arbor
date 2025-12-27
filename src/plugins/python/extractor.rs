use crate::core::types::{CodeLocation, NoneSource, NoneSourceKind, RaiseStatement};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExtractorError {
    #[error("Query compilation failed: {0}")]
    QueryCompilation(String),

    #[error("Extraction failed: {0}")]
    ExtractionFailed(String),
}

#[derive(Debug, Clone, Default)]
pub struct CallContext {
    pub current_module: String,
    pub current_class: Option<String>,
    pub imports: HashMap<String, String>,
}

pub fn extract_raises(
    tree: &tree_sitter::Tree,
    content: &str,
    path: &Path,
) -> Result<Vec<RaiseStatement>, ExtractorError> {
    let mut raises = Vec::new();
    extract_raises_from_node(tree.root_node(), content, path, &mut raises, None);
    Ok(raises)
}

pub fn extract_raises_in_range(
    tree: &tree_sitter::Tree,
    content: &str,
    path: &Path,
    line_start: u32,
    line_end: u32,
) -> Result<Vec<RaiseStatement>, ExtractorError> {
    let mut raises = Vec::new();
    extract_raises_from_node(tree.root_node(), content, path, &mut raises, Some((line_start, line_end)));
    Ok(raises)
}

fn extract_raises_from_node(
    node: tree_sitter::Node,
    content: &str,
    path: &Path,
    raises: &mut Vec<RaiseStatement>,
    line_range: Option<(u32, u32)>,
) {
    if node.kind() == "raise_statement" {
        let line = node.start_position().row as u32 + 1;

        if let Some((start, end)) = line_range {
            if line < start || line > end {
                return;
            }
        }

        if let Some(raise_stmt) = parse_raise_statement(node, content, path) {
            raises.push(raise_stmt);
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            extract_raises_from_node(child, content, path, raises, line_range);
        }
    }
}

fn parse_raise_statement(node: tree_sitter::Node, content: &str, path: &Path) -> Option<RaiseStatement> {
    let line = node.start_position().row as u32 + 1;
    let column = node.start_position().column as u32;

    let location = CodeLocation::new(path.to_path_buf(), line).with_column(column);

    let mut cursor = node.walk();
    cursor.goto_first_child();

    let mut exception_type = String::new();
    let mut message = None;

    loop {
        let child = cursor.node();
        match child.kind() {
            "raise" => {}
            "call" => {
                if let Some(func) = child.child_by_field_name("function") {
                    exception_type = get_node_text(func, content);
                }
                if let Some(args) = child.child_by_field_name("arguments") {
                    message = extract_first_string_arg(args, content);
                }
            }
            "identifier" => {
                exception_type = get_node_text(child, content);
            }
            "attribute" => {
                exception_type = get_node_text(child, content);
            }
            _ => {}
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    if exception_type.is_empty() {
        exception_type = "(re-raise)".to_string();
    }

    let qualified_type = exception_type.clone();

    let mut stmt = RaiseStatement::new(exception_type, qualified_type, location);
    if let Some(msg) = message {
        stmt = stmt.with_message(msg);
    }

    if let Some(condition) = find_guarding_condition(node, content) {
        stmt = stmt.with_condition(condition);
    }

    Some(stmt)
}

fn extract_first_string_arg(args_node: tree_sitter::Node, content: &str) -> Option<String> {
    for i in 0..args_node.child_count() {
        if let Some(child) = args_node.child(i) {
            if child.kind() == "string" {
                let text = get_node_text(child, content);
                let trimmed = text.trim_matches(|c| c == '"' || c == '\'');
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn find_guarding_condition(node: tree_sitter::Node, content: &str) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "if_statement" {
            if let Some(condition) = parent.child_by_field_name("condition") {
                return Some(get_node_text(condition, content));
            }
        }
        current = parent.parent();
    }
    None
}

pub fn extract_none_sources(
    tree: &tree_sitter::Tree,
    content: &str,
    path: &Path,
) -> Result<Vec<NoneSource>, ExtractorError> {
    let mut sources = Vec::new();
    extract_none_from_node(tree.root_node(), content, path, &mut sources, None);
    Ok(sources)
}

pub fn extract_none_sources_in_range(
    tree: &tree_sitter::Tree,
    content: &str,
    path: &Path,
    line_start: u32,
    line_end: u32,
) -> Result<Vec<NoneSource>, ExtractorError> {
    let mut sources = Vec::new();
    extract_none_from_node(tree.root_node(), content, path, &mut sources, Some((line_start, line_end)));
    Ok(sources)
}

fn extract_none_from_node(
    node: tree_sitter::Node,
    content: &str,
    path: &Path,
    sources: &mut Vec<NoneSource>,
    line_range: Option<(u32, u32)>,
) {
    let line = node.start_position().row as u32 + 1;

    let in_range = line_range.map_or(true, |(start, end)| line >= start && line <= end);

    if in_range {
        match node.kind() {
            "return_statement" => {
                if let Some(source) = parse_return_none(node, content, path) {
                    sources.push(source);
                }
            }
            "call" => {
                if let Some(source) = check_none_returning_call(node, content, path) {
                    sources.push(source);
                }
            }
            _ => {}
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            extract_none_from_node(child, content, path, sources, line_range);
        }
    }
}

fn parse_return_none(node: tree_sitter::Node, content: &str, path: &Path) -> Option<NoneSource> {
    let line = node.start_position().row as u32 + 1;
    let column = node.start_position().column as u32;
    let location = CodeLocation::new(path.to_path_buf(), line).with_column(column);

    let mut has_value = false;
    let mut is_explicit_none = false;

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() != "return" {
                has_value = true;
                if child.kind() == "none" {
                    is_explicit_none = true;
                }
            }
        }
    }

    if is_explicit_none {
        let mut source = NoneSource::new(NoneSourceKind::ExplicitReturn, location);
        if let Some(condition) = find_guarding_condition(node, content) {
            source = source.with_condition(condition);
        }
        Some(source)
    } else if !has_value {
        let mut source = NoneSource::new(NoneSourceKind::ImplicitReturn, location);
        if let Some(condition) = find_guarding_condition(node, content) {
            source = source.with_condition(condition);
        }
        Some(source)
    } else {
        None
    }
}

fn check_none_returning_call(node: tree_sitter::Node, content: &str, path: &Path) -> Option<NoneSource> {
    let func = node.child_by_field_name("function")?;

    if func.kind() == "attribute" {
        let method_name = func.child_by_field_name("attribute")?;
        let method = get_node_text(method_name, content);

        let none_methods = ["get", "pop", "setdefault", "getattr"];

        if none_methods.contains(&method.as_str()) {
            let line = node.start_position().row as u32 + 1;
            let column = node.start_position().column as u32;
            let location = CodeLocation::new(path.to_path_buf(), line).with_column(column);

            let kind = if method == "get" || method == "getattr" {
                NoneSourceKind::CollectionAccess
            } else {
                NoneSourceKind::FunctionCall
            };

            return Some(NoneSource::new(kind, location));
        }
    }

    None
}

pub fn extract_calls(
    tree: &tree_sitter::Tree,
    content: &str,
) -> Result<Vec<String>, ExtractorError> {
    let mut calls = Vec::new();
    extract_calls_from_node(tree.root_node(), content, &mut calls, None, None);
    Ok(calls)
}

pub fn extract_calls_in_range(
    tree: &tree_sitter::Tree,
    content: &str,
    line_start: u32,
    line_end: u32,
) -> Result<Vec<String>, ExtractorError> {
    let mut calls = Vec::new();
    extract_calls_from_node(tree.root_node(), content, &mut calls, Some((line_start, line_end)), None);
    Ok(calls)
}

pub fn extract_calls_in_range_with_context(
    tree: &tree_sitter::Tree,
    content: &str,
    line_start: u32,
    line_end: u32,
    context: &CallContext,
) -> Result<Vec<String>, ExtractorError> {
    let mut calls = Vec::new();
    extract_calls_from_node(tree.root_node(), content, &mut calls, Some((line_start, line_end)), Some(context));
    Ok(calls)
}

fn extract_calls_from_node(
    node: tree_sitter::Node,
    content: &str,
    calls: &mut Vec<String>,
    line_range: Option<(u32, u32)>,
    context: Option<&CallContext>,
) {
    if node.kind() == "call" {
        let line = node.start_position().row as u32 + 1;

        let in_range = line_range.map_or(true, |(start, end)| line >= start && line <= end);

        if in_range {
            if let Some(func) = node.child_by_field_name("function") {
                let call_name = get_node_text(func, content);
                let qualified = qualify_call(&call_name, context);
                if !calls.contains(&qualified) {
                    calls.push(qualified);
                }
            }
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            extract_calls_from_node(child, content, calls, line_range, context);
        }
    }
}

fn qualify_call(call_name: &str, context: Option<&CallContext>) -> String {
    let ctx = match context {
        Some(c) => c,
        None => return call_name.to_string(),
    };

    let parts: Vec<&str> = call_name.split('.').collect();

    if parts.is_empty() {
        return call_name.to_string();
    }

    if parts[0] == "self" {
        if let Some(ref class_name) = ctx.current_class {
            let method = parts[1..].join(".");
            if method.is_empty() {
                return call_name.to_string();
            }
            return format!("{}.{}.{}", ctx.current_module, class_name, method);
        }
        return call_name.to_string();
    }

    if let Some(qualified_base) = ctx.imports.get(parts[0]) {
        if parts.len() == 1 {
            return qualified_base.clone();
        }
        let rest = parts[1..].join(".");
        return format!("{}.{}", qualified_base, rest);
    }

    if parts.len() == 1 && !ctx.current_module.is_empty() {
        return format!("{}.{}", ctx.current_module, call_name);
    }

    call_name.to_string()
}

fn get_node_text(node: tree_sitter::Node, content: &str) -> String {
    content[node.byte_range()].to_string()
}

/// Extract imports from a Python file, returning a map from local name to qualified name
/// e.g., "from requests.exceptions import ConnectionError" -> {"ConnectionError": "requests.exceptions.ConnectionError"}
pub fn extract_imports(tree: &tree_sitter::Tree, content: &str) -> HashMap<String, String> {
    let mut imports = HashMap::new();
    extract_imports_from_node(tree.root_node(), content, &mut imports);
    imports
}

fn extract_imports_from_node(
    node: tree_sitter::Node,
    content: &str,
    imports: &mut HashMap<String, String>,
) {
    match node.kind() {
        "import_from_statement" => {
            parse_import_from(node, content, imports);
        }
        "import_statement" => {
            parse_import(node, content, imports);
        }
        _ => {}
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            extract_imports_from_node(child, content, imports);
        }
    }
}

fn parse_import_from(node: tree_sitter::Node, content: &str, imports: &mut HashMap<String, String>) {
    let mut module_name = String::new();
    let mut names: Vec<(String, Option<String>)> = Vec::new();

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            match child.kind() {
                "dotted_name" => {
                    if module_name.is_empty() {
                        module_name = get_node_text(child, content);
                    } else {
                        let name = get_node_text(child, content);
                        names.push((name, None));
                    }
                }
                "relative_import" => {
                    module_name = parse_relative_import(child, content);
                }
                "aliased_import" => {
                    if let Some((name, alias)) = parse_aliased_import(child, content) {
                        names.push((name, Some(alias)));
                    }
                }
                "identifier" => {
                    let name = get_node_text(child, content);
                    if name != "from" && name != "import" {
                        names.push((name.clone(), None));
                    }
                }
                _ => {}
            }
        }
    }

    for (name, alias) in names {
        let local_name = alias.unwrap_or_else(|| name.clone());
        let qualified = format!("{}.{}", module_name, name);
        imports.insert(local_name, qualified);
    }
}

fn parse_import(node: tree_sitter::Node, content: &str, imports: &mut HashMap<String, String>) {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            match child.kind() {
                "dotted_name" => {
                    let name = get_node_text(child, content);
                    let local_name = name.split('.').last().unwrap_or(&name).to_string();
                    imports.insert(local_name, name);
                }
                "aliased_import" => {
                    if let Some((name, alias)) = parse_aliased_import(child, content) {
                        imports.insert(alias, name);
                    }
                }
                _ => {}
            }
        }
    }
}

fn parse_relative_import(node: tree_sitter::Node, content: &str) -> String {
    let mut result = String::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            match child.kind() {
                "import_prefix" => {
                    result = get_node_text(child, content);
                }
                "dotted_name" => {
                    let module = get_node_text(child, content);
                    if result.is_empty() {
                        result = module;
                    } else {
                        result = format!("{}{}", result, module);
                    }
                }
                _ => {}
            }
        }
    }
    result
}

fn parse_aliased_import(node: tree_sitter::Node, content: &str) -> Option<(String, String)> {
    let mut name = None;
    let mut alias = None;

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            match child.kind() {
                "dotted_name" | "identifier" => {
                    if name.is_none() {
                        name = Some(get_node_text(child, content));
                    }
                }
                "as" => {}
                _ => {
                    if name.is_some() && alias.is_none() && child.kind() == "identifier" {
                        alias = Some(get_node_text(child, content));
                    }
                }
            }
        }
    }

    let mut found_as = false;
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "as" {
                found_as = true;
            } else if found_as && child.kind() == "identifier" {
                alias = Some(get_node_text(child, content));
                break;
            }
        }
    }

    match (name, alias) {
        (Some(n), Some(a)) => Some((n, a)),
        (Some(n), None) => Some((n.clone(), n)),
        _ => None,
    }
}

pub fn find_exception_definition(_exc_type: &str) -> Option<CodeLocation> {
    // This will be implemented when we have the symbol index available
    // For now, return None - the caller can look up in the index
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_python(code: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_python::LANGUAGE.into()).unwrap();
        parser.parse(code, None).unwrap()
    }

    #[test]
    fn test_extract_simple_raise() {
        let code = r#"
def foo():
    raise ValueError("error message")
"#;
        let tree = parse_python(code);
        let path = Path::new("test.py");
        let raises = extract_raises(&tree, code, path).unwrap();

        assert_eq!(raises.len(), 1);
        assert_eq!(raises[0].exception_type, "ValueError");
        assert_eq!(raises[0].message, Some("error message".to_string()));
    }

    #[test]
    fn test_extract_raise_no_args() {
        let code = r#"
def foo():
    raise KeyError
"#;
        let tree = parse_python(code);
        let path = Path::new("test.py");
        let raises = extract_raises(&tree, code, path).unwrap();

        assert_eq!(raises.len(), 1);
        assert_eq!(raises[0].exception_type, "KeyError");
        assert_eq!(raises[0].message, None);
    }

    #[test]
    fn test_extract_bare_raise() {
        let code = r#"
try:
    something()
except:
    raise
"#;
        let tree = parse_python(code);
        let path = Path::new("test.py");
        let raises = extract_raises(&tree, code, path).unwrap();

        assert_eq!(raises.len(), 1);
        assert_eq!(raises[0].exception_type, "(re-raise)");
    }

    #[test]
    fn test_extract_raise_with_condition() {
        let code = r#"
def foo(x):
    if x < 0:
        raise ValueError("must be positive")
"#;
        let tree = parse_python(code);
        let path = Path::new("test.py");
        let raises = extract_raises(&tree, code, path).unwrap();

        assert_eq!(raises.len(), 1);
        assert_eq!(raises[0].condition, Some("x < 0".to_string()));
    }

    #[test]
    fn test_extract_qualified_raise() {
        let code = r#"
def foo():
    raise requests.exceptions.ConnectionError("failed")
"#;
        let tree = parse_python(code);
        let path = Path::new("test.py");
        let raises = extract_raises(&tree, code, path).unwrap();

        assert_eq!(raises.len(), 1);
        assert_eq!(raises[0].exception_type, "requests.exceptions.ConnectionError");
    }

    #[test]
    fn test_extract_explicit_none_return() {
        let code = r#"
def foo():
    return None
"#;
        let tree = parse_python(code);
        let path = Path::new("test.py");
        let sources = extract_none_sources(&tree, code, path).unwrap();

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].kind, NoneSourceKind::ExplicitReturn);
    }

    #[test]
    fn test_extract_implicit_none_return() {
        let code = r#"
def foo():
    print("hello")
    return
"#;
        let tree = parse_python(code);
        let path = Path::new("test.py");
        let sources = extract_none_sources(&tree, code, path).unwrap();

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].kind, NoneSourceKind::ImplicitReturn);
    }

    #[test]
    fn test_extract_dict_get() {
        let code = r#"
def foo():
    d = {}
    return d.get("key")
"#;
        let tree = parse_python(code);
        let path = Path::new("test.py");
        let sources = extract_none_sources(&tree, code, path).unwrap();

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].kind, NoneSourceKind::CollectionAccess);
    }

    #[test]
    fn test_extract_calls() {
        let code = r#"
def foo():
    bar()
    obj.method()
    module.func()
"#;
        let tree = parse_python(code);
        let calls = extract_calls(&tree, code).unwrap();

        assert_eq!(calls.len(), 3);
        assert!(calls.contains(&"bar".to_string()));
        assert!(calls.contains(&"obj.method".to_string()));
        assert!(calls.contains(&"module.func".to_string()));
    }

    #[test]
    fn test_extract_calls_in_range() {
        let code = r#"
def foo():
    bar()
    baz()

def other():
    qux()
"#;
        let tree = parse_python(code);
        let calls = extract_calls_in_range(&tree, code, 2, 4).unwrap();

        assert_eq!(calls.len(), 2);
        assert!(calls.contains(&"bar".to_string()));
        assert!(calls.contains(&"baz".to_string()));
        assert!(!calls.contains(&"qux".to_string()));
    }

    #[test]
    fn test_extract_imports_from() {
        let code = r#"
from requests.exceptions import ConnectionError, Timeout
from os.path import join as path_join
"#;
        let tree = parse_python(code);
        let imports = extract_imports(&tree, code);

        assert_eq!(imports.get("ConnectionError"), Some(&"requests.exceptions.ConnectionError".to_string()));
        assert_eq!(imports.get("Timeout"), Some(&"requests.exceptions.Timeout".to_string()));
        assert_eq!(imports.get("path_join"), Some(&"os.path.join".to_string()));
    }

    #[test]
    fn test_extract_imports_regular() {
        let code = r#"
import requests
import os.path
import json as j
"#;
        let tree = parse_python(code);
        let imports = extract_imports(&tree, code);

        assert_eq!(imports.get("requests"), Some(&"requests".to_string()));
        assert_eq!(imports.get("path"), Some(&"os.path".to_string()));
        assert_eq!(imports.get("j"), Some(&"json".to_string()));
    }
}
