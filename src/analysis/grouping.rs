use crate::core::database::GroupingSuggestion;
use crate::core::types::RaiseStatement;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GroupingSignal {
    RecoveryStrategy,
    SourcePackage,
    SemanticSimilarity,
    CommonParent,
}

impl GroupingSignal {
    pub fn as_str(&self) -> &'static str {
        match self {
            GroupingSignal::RecoveryStrategy => "recovery strategy",
            GroupingSignal::SourcePackage => "source package",
            GroupingSignal::SemanticSimilarity => "semantic similarity",
            GroupingSignal::CommonParent => "common parent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecoveryStrategy {
    Retry,
    FixInput,
    ReAuthenticate,
    Abort,
    Ignore,
}

impl RecoveryStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecoveryStrategy::Retry => "retry",
            RecoveryStrategy::FixInput => "fix input",
            RecoveryStrategy::ReAuthenticate => "re-authenticate",
            RecoveryStrategy::Abort => "abort",
            RecoveryStrategy::Ignore => "ignore",
        }
    }

    pub fn from_exception_type(exc_type: &str) -> Self {
        let lower = exc_type.to_lowercase();

        if lower.contains("timeout")
            || lower.contains("connection")
            || lower.contains("network")
            || lower.contains("temporary")
            || lower.contains("retry")
            || lower.contains("throttl")
            || lower.contains("ratelimit")
        {
            return RecoveryStrategy::Retry;
        }

        if lower.contains("auth")
            || lower.contains("permission")
            || lower.contains("forbidden")
            || lower.contains("unauthorized")
            || lower.contains("credential")
            || lower.contains("token")
        {
            return RecoveryStrategy::ReAuthenticate;
        }

        if lower.contains("validation")
            || lower.contains("invalid")
            || lower.contains("value")
            || lower.contains("type")
            || lower.contains("argument")
            || lower.contains("format")
            || lower.contains("parse")
        {
            return RecoveryStrategy::FixInput;
        }

        if lower.contains("notfound")
            || lower.contains("missing")
            || lower.contains("doesnotexist")
        {
            return RecoveryStrategy::Ignore;
        }

        RecoveryStrategy::Abort
    }
}

#[derive(Debug, Clone)]
struct ExceptionInfo {
    exception_type: String,
    package: Option<String>,
    semantic_category: Option<String>,
    recovery_strategy: RecoveryStrategy,
}

impl ExceptionInfo {
    fn from_raise(raise: &RaiseStatement) -> Self {
        let package = extract_package(&raise.qualified_type);
        let semantic_category = detect_semantic_category(&raise.exception_type);
        let recovery_strategy = RecoveryStrategy::from_exception_type(&raise.exception_type);

        Self {
            exception_type: raise.exception_type.clone(),
            package,
            semantic_category,
            recovery_strategy,
        }
    }
}

fn extract_package(qualified_type: &str) -> Option<String> {
    let parts: Vec<&str> = qualified_type.split('.').collect();
    if parts.len() >= 2 {
        Some(parts[0].to_string())
    } else {
        None
    }
}

fn detect_semantic_category(exc_type: &str) -> Option<String> {
    let lower = exc_type.to_lowercase();

    let categories = [
        ("timeout", "Timeout"),
        ("connection", "Connection"),
        ("network", "Network"),
        ("auth", "Authentication"),
        ("permission", "Permission"),
        ("validation", "Validation"),
        ("notfound", "NotFound"),
        ("io", "IO"),
        ("file", "File"),
        ("encoding", "Encoding"),
        ("json", "JSON"),
        ("http", "HTTP"),
        ("ssl", "SSL"),
        ("dns", "DNS"),
        ("socket", "Socket"),
    ];

    for (pattern, category) in categories {
        if lower.contains(pattern) {
            return Some(category.to_string());
        }
    }

    None
}

pub fn suggest_groups(exceptions: &[RaiseStatement]) -> Vec<GroupingSuggestion> {
    if exceptions.is_empty() {
        return Vec::new();
    }

    let mut suggestions = Vec::new();

    let infos: Vec<ExceptionInfo> = exceptions.iter().map(ExceptionInfo::from_raise).collect();

    let package_groups = group_by_package(&infos);
    for (package, exc_types) in package_groups {
        if exc_types.len() >= 2 {
            suggestions.push(GroupingSuggestion {
                group_name: format!("{} exceptions", package),
                exceptions: exc_types.clone(),
                rationale: format!("All exceptions from the {} package", package),
                handler_example: generate_handler_example(&exc_types, &package),
            });
        }
    }

    let semantic_groups = group_by_semantic(&infos);
    for (category, exc_types) in semantic_groups {
        if exc_types.len() >= 2 {
            suggestions.push(GroupingSuggestion {
                group_name: format!("{} errors", category),
                exceptions: exc_types.clone(),
                rationale: format!("Semantically related {} exceptions", category.to_lowercase()),
                handler_example: generate_handler_example(&exc_types, &category),
            });
        }
    }

    let recovery_groups = group_by_recovery(&infos);
    for (strategy, exc_types) in recovery_groups {
        if exc_types.len() >= 2 {
            let strategy_name = strategy.as_str();
            suggestions.push(GroupingSuggestion {
                group_name: format!("{} exceptions", capitalize(strategy_name)),
                exceptions: exc_types.clone(),
                rationale: format!("Exceptions that can be handled with {} strategy", strategy_name),
                handler_example: generate_recovery_handler(&exc_types, strategy),
            });
        }
    }

    deduplicate_suggestions(&mut suggestions);

    suggestions
}

fn group_by_package(infos: &[ExceptionInfo]) -> HashMap<String, Vec<String>> {
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();

    for info in infos {
        if let Some(ref package) = info.package {
            groups
                .entry(package.clone())
                .or_default()
                .push(info.exception_type.clone());
        }
    }

    for types in groups.values_mut() {
        types.sort();
        types.dedup();
    }

    groups
}

fn group_by_semantic(infos: &[ExceptionInfo]) -> HashMap<String, Vec<String>> {
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();

    for info in infos {
        if let Some(ref category) = info.semantic_category {
            groups
                .entry(category.clone())
                .or_default()
                .push(info.exception_type.clone());
        }
    }

    for types in groups.values_mut() {
        types.sort();
        types.dedup();
    }

    groups
}

fn group_by_recovery(infos: &[ExceptionInfo]) -> HashMap<RecoveryStrategy, Vec<String>> {
    let mut groups: HashMap<RecoveryStrategy, Vec<String>> = HashMap::new();

    for info in infos {
        groups
            .entry(info.recovery_strategy)
            .or_default()
            .push(info.exception_type.clone());
    }

    for types in groups.values_mut() {
        types.sort();
        types.dedup();
    }

    groups
}

fn generate_handler_example(exc_types: &[String], group_name: &str) -> String {
    let types_str = exc_types.join(", ");
    format!(
        r#"try:
    result = call_function()
except ({}) as e:
    # Handle {} errors
    logger.error(f"{} error: {{e}}")
    raise"#,
        types_str, group_name.to_lowercase(), group_name
    )
}

fn generate_recovery_handler(exc_types: &[String], strategy: RecoveryStrategy) -> String {
    let types_str = exc_types.join(", ");

    match strategy {
        RecoveryStrategy::Retry => format!(
            r#"for attempt in range(max_retries):
    try:
        result = call_function()
        break
    except ({}) as e:
        if attempt == max_retries - 1:
            raise
        time.sleep(backoff * (2 ** attempt))"#,
            types_str
        ),
        RecoveryStrategy::FixInput => format!(
            r#"try:
    result = call_function(data)
except ({}) as e:
    # Log validation error and return user-friendly message
    logger.warning(f"Invalid input: {{e}}")
    raise ValidationError(str(e)) from e"#,
            types_str
        ),
        RecoveryStrategy::ReAuthenticate => format!(
            r#"try:
    result = call_function()
except ({}) as e:
    # Refresh credentials and retry
    refresh_credentials()
    result = call_function()"#,
            types_str
        ),
        RecoveryStrategy::Ignore => format!(
            r#"try:
    result = call_function()
except ({}) as e:
    # Resource not found, use default
    logger.debug(f"Not found: {{e}}")
    result = default_value"#,
            types_str
        ),
        RecoveryStrategy::Abort => format!(
            r#"try:
    result = call_function()
except ({}) as e:
    # Unrecoverable error, abort operation
    logger.error(f"Fatal error: {{e}}")
    raise"#,
            types_str
        ),
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn deduplicate_suggestions(suggestions: &mut Vec<GroupingSuggestion>) {
    let mut to_remove = Vec::new();

    for i in 0..suggestions.len() {
        for j in 0..suggestions.len() {
            if i != j {
                let set_i: std::collections::HashSet<_> = suggestions[i].exceptions.iter().collect();
                let set_j: std::collections::HashSet<_> = suggestions[j].exceptions.iter().collect();

                if set_i.is_subset(&set_j) && set_i.len() < set_j.len() {
                    to_remove.push(i);
                    break;
                }
            }
        }
    }

    to_remove.sort();
    to_remove.dedup();
    for i in to_remove.into_iter().rev() {
        suggestions.remove(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::CodeLocation;
    use std::path::PathBuf;

    fn make_raise(exc_type: &str, qualified_type: &str) -> RaiseStatement {
        RaiseStatement::new(
            exc_type.to_string(),
            qualified_type.to_string(),
            CodeLocation::new(PathBuf::from("test.py"), 1),
        )
    }

    #[test]
    fn test_recovery_strategy_detection() {
        assert_eq!(
            RecoveryStrategy::from_exception_type("ConnectionTimeout"),
            RecoveryStrategy::Retry
        );
        assert_eq!(
            RecoveryStrategy::from_exception_type("AuthenticationError"),
            RecoveryStrategy::ReAuthenticate
        );
        assert_eq!(
            RecoveryStrategy::from_exception_type("ValidationError"),
            RecoveryStrategy::FixInput
        );
        assert_eq!(
            RecoveryStrategy::from_exception_type("ResourceNotFound"),
            RecoveryStrategy::Ignore
        );
        assert_eq!(
            RecoveryStrategy::from_exception_type("UnknownError"),
            RecoveryStrategy::Abort
        );
    }

    #[test]
    fn test_semantic_category_detection() {
        assert_eq!(detect_semantic_category("ConnectionError"), Some("Connection".to_string()));
        assert_eq!(detect_semantic_category("TimeoutError"), Some("Timeout".to_string()));
        assert_eq!(detect_semantic_category("AuthError"), Some("Authentication".to_string()));
        assert_eq!(detect_semantic_category("CustomError"), None);
    }

    #[test]
    fn test_package_extraction() {
        assert_eq!(extract_package("requests.exceptions.ConnectionError"), Some("requests".to_string()));
        assert_eq!(extract_package("ValueError"), None);
    }

    #[test]
    fn test_group_by_package() {
        let raises = vec![
            make_raise("ConnectionError", "requests.exceptions.ConnectionError"),
            make_raise("Timeout", "requests.exceptions.Timeout"),
            make_raise("HTTPError", "urllib3.exceptions.HTTPError"),
        ];

        let suggestions = suggest_groups(&raises);

        let requests_group = suggestions.iter().find(|s| s.group_name.contains("requests"));
        assert!(requests_group.is_some());
        assert_eq!(requests_group.unwrap().exceptions.len(), 2);
    }

    #[test]
    fn test_empty_exceptions() {
        let suggestions = suggest_groups(&[]);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_single_exception_no_groups() {
        let raises = vec![make_raise("ValueError", "ValueError")];
        let suggestions = suggest_groups(&raises);
        assert!(suggestions.is_empty());
    }
}
