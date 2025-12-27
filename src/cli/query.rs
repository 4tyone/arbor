use crate::analysis::grouping::RecoveryStrategy;
use crate::core::database::ArborDatabase;
use crate::core::paths;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum QueryError {
    #[error("Database not initialized. Run 'arbor init' first.")]
    DatabaseNotInitialized,

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Exception not found: {0} in function {1}")]
    ExceptionNotFound(String, String),

    #[error("None source index out of bounds: {0}")]
    NoneSourceIndexOutOfBounds(usize),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Database error: {0}")]
    Database(#[from] crate::core::database::DatabaseError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

fn load_database() -> Result<ArborDatabase, QueryError> {
    let db_path = paths::database_path();
    if !db_path.exists() {
        return Err(QueryError::DatabaseNotInitialized);
    }
    Ok(ArborDatabase::load(&db_path)?)
}

// ============================================================================
// LOCAL (Entity-Level) Queries
// ============================================================================

pub fn query_risk(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    let risk = analysis.risk_level();
    let exc_count = analysis.exception_count();
    let none_count = analysis.none_source_count();

    Ok(format!(
        "{} {} | {} exceptions, {} None sources | depth: {}",
        risk.emoji(),
        risk.as_str(),
        exc_count,
        none_count,
        analysis.call_depth
    ))
}

pub fn query_has(function: &str, exception: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    let found = analysis
        .raises
        .iter()
        .find(|r| r.exception_type == exception || r.qualified_type == exception);

    match found {
        Some(raise) => {
            let strategy = RecoveryStrategy::from_exception_type(&raise.exception_type);
            let via = raise
                .raise_location
                .containing_function
                .as_deref()
                .unwrap_or("direct");

            let def_loc = raise
                .definition_location
                .as_ref()
                .map(|l| l.to_string_short())
                .unwrap_or_else(|| "(builtin)".to_string());

            let fn_name = function.split('.').last().unwrap_or(function);

            let mut result = format!("## Yes\n\n");
            result.push_str(&format!(
                "`{}` can raise `{}`.\n\n",
                function, exception
            ));
            result.push_str(&format!("- **Via:** `{}`\n", via));
            result.push_str(&format!("- **Defined at:** `{}`\n\n", def_loc));
            result.push_str("Handle with:\n");
            result.push_str("```python\n");
            result.push_str(&format!("try:\n    response = {}()\n", fn_name));
            result.push_str(&format!(
                "except {} as e:\n    # {} ({})\n    pass\n",
                exception,
                strategy.as_str(),
                if strategy == RecoveryStrategy::Retry { "retryable" } else { "not retryable" }
            ));
            result.push_str("```\n");

            Ok(result)
        }
        None => {
            let mut result = format!("## No\n\n");
            result.push_str(&format!(
                "`{}` cannot raise `{}`.\n\n",
                function, exception
            ));
            result.push_str("This exception is not in the call graph.\n");
            Ok(result)
        }
    }
}

pub fn query_handle(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    if analysis.raises.is_empty() {
        return Ok(format!(
            "# {} raises no exceptions - no handler needed\nresult = {}()",
            function,
            function.split('.').last().unwrap_or(function)
        ));
    }

    let mut retry_exceptions = Vec::new();
    let mut auth_exceptions = Vec::new();
    let mut input_exceptions = Vec::new();
    let mut other_exceptions = Vec::new();

    for raise in &analysis.raises {
        let strategy = RecoveryStrategy::from_exception_type(&raise.exception_type);
        match strategy {
            RecoveryStrategy::Retry => retry_exceptions.push(raise.exception_type.clone()),
            RecoveryStrategy::ReAuthenticate => auth_exceptions.push(raise.exception_type.clone()),
            RecoveryStrategy::FixInput => input_exceptions.push(raise.exception_type.clone()),
            _ => other_exceptions.push(raise.exception_type.clone()),
        }
    }

    retry_exceptions.sort();
    retry_exceptions.dedup();
    auth_exceptions.sort();
    auth_exceptions.dedup();
    input_exceptions.sort();
    input_exceptions.dedup();
    other_exceptions.sort();
    other_exceptions.dedup();

    let fn_name = function.split('.').last().unwrap_or(function);
    let mut handler = String::from("try:\n    result = ");
    handler.push_str(fn_name);
    handler.push_str("()\n");

    if !retry_exceptions.is_empty() {
        handler.push_str(&format!(
            "except ({}) as e:\n    # Retry with backoff\n    raise\n",
            retry_exceptions.join(", ")
        ));
    }

    if !auth_exceptions.is_empty() {
        handler.push_str(&format!(
            "except ({}) as e:\n    # Re-authenticate and retry\n    raise\n",
            auth_exceptions.join(", ")
        ));
    }

    if !input_exceptions.is_empty() {
        handler.push_str(&format!(
            "except ({}) as e:\n    # Fix input and retry\n    raise\n",
            input_exceptions.join(", ")
        ));
    }

    if !other_exceptions.is_empty() {
        handler.push_str(&format!(
            "except ({}) as e:\n    # Handle or re-raise\n    raise\n",
            other_exceptions.join(", ")
        ));
    }

    Ok(handler)
}

pub fn query_signature(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    Ok(format!(
        "{}\n  Location: {}",
        analysis.signature,
        analysis.location.to_string_short()
    ))
}

pub fn query_one_exception(function: &str, exc_type: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    let raise = analysis
        .raises
        .iter()
        .find(|r| r.exception_type == exc_type || r.qualified_type == exc_type)
        .ok_or_else(|| QueryError::ExceptionNotFound(exc_type.to_string(), function.to_string()))?;

    let mut result = format!("Exception: {}\n", raise.exception_type);
    result.push_str(&format!("Qualified: {}\n", raise.qualified_type));
    result.push_str(&format!(
        "Raised at: {}\n",
        raise.raise_location.to_string_short()
    ));

    if let Some(ref def_loc) = raise.definition_location {
        result.push_str(&format!("Defined at: {}\n", def_loc.to_string_short()));
    } else {
        result.push_str("Defined at: (builtin)\n");
    }

    if let Some(ref cond) = raise.condition {
        result.push_str(&format!("Condition: {}\n", cond));
    }

    if let Some(ref msg) = raise.message {
        result.push_str(&format!("Message: {}\n", msg));
    }

    let strategy = RecoveryStrategy::from_exception_type(&raise.exception_type);
    result.push_str(&format!("Recovery: {}", strategy.as_str()));

    Ok(result)
}

pub fn query_one_none(function: &str, index: usize) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    let source = analysis
        .none_sources
        .get(index)
        .ok_or(QueryError::NoneSourceIndexOutOfBounds(index))?;

    let mut result = format!("Kind: {}\n", source.kind.as_str());
    result.push_str(&format!("Location: {}\n", source.location.to_string_short()));

    if let Some(ref def_loc) = source.source_definition {
        result.push_str(&format!("Source: {}\n", def_loc.to_string_short()));
    }

    if let Some(ref cond) = source.condition {
        result.push_str(&format!("Condition: {}", cond));
    }

    Ok(result)
}

pub fn query_callers(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;

    if !db.functions.contains_key(function) && !db.symbol_index.contains(function) {
        return Err(QueryError::FunctionNotFound(function.to_string()));
    }

    match db.dependency_graph.get_callers(function) {
        Some(callers) if !callers.is_empty() => {
            let mut result = format!("Functions calling {}:\n", function);
            for caller in callers {
                result.push_str(&format!("  - {}\n", caller));
            }
            Ok(result)
        }
        _ => Ok(format!("No callers found for {}", function)),
    }
}

pub fn query_callees(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;

    if !db.functions.contains_key(function) && !db.symbol_index.contains(function) {
        return Err(QueryError::FunctionNotFound(function.to_string()));
    }

    match db.dependency_graph.get_callees(function) {
        Some(callees) if !callees.is_empty() => {
            let mut result = format!("Functions called by {}:\n", function);
            for callee in callees {
                result.push_str(&format!("  - {}\n", callee));
            }
            Ok(result)
        }
        _ => Ok(format!("No callees found for {}", function)),
    }
}

pub fn query_diff(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let _analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    Ok(format!(
        "Diff for {}: No previous analysis stored (history tracking not yet implemented)",
        function
    ))
}

// ============================================================================
// FULL ANALYSIS Queries
// ============================================================================

pub fn query_exceptions(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    let mut result = format!("# Exceptions for `{}`\n\n", function);
    result.push_str(&format!("**Signature:** `{}`\n", analysis.signature));
    result.push_str(&format!("**Location:** `{}`\n", analysis.location.to_string_short()));
    result.push_str(&format!("**Total Exceptions:** {}\n\n", analysis.raises.len()));

    if analysis.raises.is_empty() {
        result.push_str("This function does not raise any exceptions.\n");
        return Ok(result);
    }

    result.push_str("## Exceptions\n\n");

    for raise in &analysis.raises {
        let strategy = RecoveryStrategy::from_exception_type(&raise.exception_type);
        let retryable = matches!(strategy, RecoveryStrategy::Retry);

        result.push_str(&format!("### {}\n\n", raise.exception_type));
        result.push_str(&format!("- **Type:** `{}`\n", raise.qualified_type));
        result.push_str(&format!(
            "- **Raised at:** `{}`\n",
            raise.raise_location.to_string_short()
        ));

        if let Some(ref def_loc) = raise.definition_location {
            result.push_str(&format!("- **Defined at:** `{}`\n", def_loc.to_string_short()));
        } else {
            result.push_str("- **Defined at:** (builtin)\n");
        }

        if let Some(ref cond) = raise.condition {
            result.push_str(&format!("- **Condition:** {}\n", cond));
        }

        result.push_str(&format!(
            "- **Recovery:** {} ({})\n",
            strategy.as_str(),
            if retryable { "retryable" } else { "not retryable" }
        ));

        if let Some(ref containing_fn) = raise.raise_location.containing_function {
            if let Some(chain) = analysis.call_chains.get(containing_fn) {
                if !chain.is_empty() {
                    let chain_str = std::iter::once(function.to_string())
                        .chain(chain.iter().cloned())
                        .collect::<Vec<_>>()
                        .join(" → ");
                    result.push_str(&format!("- **Call Chain:** `{}`\n", chain_str));
                }
            }
        }

        result.push('\n');
    }

    if !db.grouping_suggestions.is_empty() {
        result.push_str("---\n\n");
        result.push_str("## Suggested Groupings\n\n");
        result.push_str(&format!(
            "For grouping details, see: `arbor query groups`\n"
        ));
    }

    Ok(result)
}

pub fn query_none(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    let mut result = format!("# None Sources for `{}`\n\n", function);
    result.push_str(&format!("**Signature:** `{}`\n", analysis.signature));
    result.push_str(&format!("**Location:** `{}`\n", analysis.location.to_string_short()));
    result.push_str(&format!("**Total None Sources:** {}\n\n", analysis.none_sources.len()));

    if analysis.none_sources.is_empty() {
        result.push_str("This function does not have any None sources.\n");
        return Ok(result);
    }

    result.push_str("## None Sources\n\n");

    for (i, source) in analysis.none_sources.iter().enumerate() {
        result.push_str(&format!("### {}. {}\n\n", i + 1, source.kind.as_str()));
        result.push_str(&format!("- **Kind:** `{}`\n", source.kind.as_str()));
        result.push_str(&format!("- **Location:** `{}`\n", source.location.to_string_short()));

        if let Some(ref def_loc) = source.source_definition {
            result.push_str(&format!("- **Source:** `{}`\n", def_loc.to_string_short()));
        }

        if let Some(ref cond) = source.condition {
            result.push_str(&format!("- **Condition:** {}\n", cond));
        }

        if let Some(ref containing_fn) = source.location.containing_function {
            if let Some(chain) = analysis.call_chains.get(containing_fn) {
                if !chain.is_empty() {
                    let chain_str = std::iter::once(function.to_string())
                        .chain(chain.iter().cloned())
                        .collect::<Vec<_>>()
                        .join(" → ");
                    result.push_str(&format!("- **Call Chain:** `{}`\n", chain_str));
                }
            }
        }

        result.push('\n');
    }

    result.push_str("---\n\n");
    result.push_str("## Recommendations\n\n");
    result.push_str("- Consider using `.get(key, default)` pattern at call sites\n");
    result.push_str("- Check for None before accessing attributes\n");
    result.push_str("- Use type hints: `-> T | None` if None is intentional\n");

    Ok(result)
}

pub fn query_function(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    let risk = analysis.risk_level();
    let mut result = format!("# Function Analysis: `{}`\n\n", analysis.function_id);

    result.push_str("## Overview\n\n");
    result.push_str("| Property | Value |\n");
    result.push_str("|----------|-------|\n");
    result.push_str(&format!("| **Qualified Name** | `{}` |\n", analysis.function_id));
    result.push_str(&format!("| **Signature** | `{}` |\n", analysis.signature));
    result.push_str(&format!(
        "| **File** | `{}` |\n",
        analysis.location.file.display()
    ));
    result.push_str(&format!("| **Line** | {} |\n", analysis.location.line));
    result.push_str(&format!("| **Risk** | {} {} |\n", risk.emoji(), risk.as_str()));
    result.push('\n');

    result.push_str("## Analysis Summary\n\n");
    result.push_str("| Metric | Count |\n");
    result.push_str("|--------|-------|\n");
    result.push_str(&format!("| Exceptions | {} |\n", analysis.raises.len()));
    result.push_str(&format!("| None sources | {} |\n", analysis.none_sources.len()));
    result.push_str(&format!("| Functions traced | {} |\n", analysis.functions_traced));
    result.push_str(&format!("| Call depth | {} |\n", analysis.call_depth));
    result.push('\n');

    if !analysis.raises.is_empty() {
        result.push_str("## Exception Groups (by Recovery Strategy)\n\n");
        result.push_str("| Group | Exceptions | Retryable |\n");
        result.push_str("|-------|------------|----------|\n");

        let mut strategy_groups: std::collections::HashMap<RecoveryStrategy, Vec<&str>> =
            std::collections::HashMap::new();

        for raise in &analysis.raises {
            let strategy = RecoveryStrategy::from_exception_type(&raise.exception_type);
            strategy_groups
                .entry(strategy)
                .or_default()
                .push(&raise.exception_type);
        }

        for (strategy, exceptions) in &strategy_groups {
            let mut unique_exceptions: Vec<&str> = exceptions.clone();
            unique_exceptions.sort();
            unique_exceptions.dedup();
            let retryable = matches!(strategy, RecoveryStrategy::Retry);
            result.push_str(&format!(
                "| {} | {} | {} |\n",
                capitalize(strategy.as_str()),
                unique_exceptions.join(", "),
                if retryable { "Yes" } else { "No" }
            ));
        }
        result.push('\n');
    }

    if !analysis.raises.is_empty() {
        result.push_str("## Exceptions\n\n");
        for raise in &analysis.raises {
            let loc = format!(
                "{}:{}",
                raise.raise_location.file.file_name().unwrap_or_default().to_string_lossy(),
                raise.raise_location.line
            );
            result.push_str(&format!("- `{}` at {}\n", raise.exception_type, loc));
        }
        result.push('\n');
    }

    if !analysis.none_sources.is_empty() {
        result.push_str("## None Sources\n\n");
        for source in &analysis.none_sources {
            let loc = format!(
                "{}:{}",
                source.location.file.file_name().unwrap_or_default().to_string_lossy(),
                source.location.line
            );
            result.push_str(&format!("- {} at {}\n", source.kind.as_str(), loc));
        }
        result.push('\n');
    }

    result.push_str("---\n\n");
    result.push_str("## Quick Commands\n\n");
    result.push_str("```bash\n");
    result.push_str(&format!("arbor query exceptions {}    # Full exception list\n", function));
    result.push_str(&format!("arbor query none {}          # Full None sources\n", function));
    result.push_str(&format!("arbor query handle {}        # Generate handler\n", function));
    result.push_str("```\n");

    Ok(result)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

pub fn query_chain(function: &str, exception: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    let raise = analysis
        .raises
        .iter()
        .find(|r| r.exception_type == exception || r.qualified_type == exception)
        .ok_or_else(|| QueryError::ExceptionNotFound(exception.to_string(), function.to_string()))?;

    let containing_fn = raise
        .raise_location
        .containing_function
        .as_deref()
        .unwrap_or("unknown");

    let chain = analysis.call_chains.get(containing_fn);

    let strategy = RecoveryStrategy::from_exception_type(&raise.exception_type);
    let retryable = matches!(strategy, RecoveryStrategy::Retry);

    let mut result = format!("# Call Chain: `{}` in `{}`\n\n", exception, function);

    let chain_vec: Vec<String> = match chain {
        Some(c) if !c.is_empty() => {
            std::iter::once(function.to_string())
                .chain(c.iter().cloned())
                .collect()
        }
        _ => vec![function.to_string()],
    };

    result.push_str("## Path\n\n");
    result.push_str("```\n");

    let raise_file = raise.raise_location.file.file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let raise_line = raise.raise_location.line;

    for (i, fn_name) in chain_vec.iter().enumerate() {
        let is_last = i == chain_vec.len() - 1;
        let indent = "    ".repeat(i);

        if i == 0 {
            result.push_str(&format!("{} ({}:{})\n", fn_name, raise_file, raise_line));
        } else {
            result.push_str(&format!("{}│\n", indent));
            result.push_str(&format!("{}└── {}\n", indent, fn_name));
        }

        if is_last {
            let final_indent = "    ".repeat(i + 1);
            result.push_str(&format!("{}│\n", final_indent));
            result.push_str(&format!("{}└── 🔴 raise {}(\"...\")\n", final_indent, exception));
        }
    }

    result.push_str("```\n\n");

    result.push_str("## Details\n\n");
    result.push_str("| Depth | Function | File | Line |\n");
    result.push_str("|-------|----------|------|------|\n");

    for (i, fn_name) in chain_vec.iter().enumerate() {
        result.push_str(&format!(
            "| {} | `{}` | {} | {} |\n",
            i,
            fn_name,
            raise_file,
            if i == chain_vec.len() - 1 { raise_line.to_string() } else { "-".to_string() }
        ));
    }
    result.push('\n');

    result.push_str("## Exception Details\n\n");
    result.push_str(&format!(
        "- **Raised at:** `{}:{}`\n",
        raise.raise_location.file.display(),
        raise.raise_location.line
    ));

    if let Some(ref def_loc) = raise.definition_location {
        result.push_str(&format!("- **Defined at:** `{}`\n", def_loc.to_string_short()));
    } else {
        result.push_str("- **Defined at:** (builtin)\n");
    }

    if let Some(ref cond) = raise.condition {
        result.push_str(&format!("- **Condition:** {}\n", cond));
    }

    if let Some(ref msg) = raise.message {
        result.push_str(&format!("- **Message:** \"{}\"\n", msg));
    }

    result.push('\n');

    result.push_str("## Handling Recommendation\n\n");
    result.push_str(&format!(
        "This exception propagates through **{} function call(s)** before reaching `{}`.\n\n",
        chain_vec.len() - 1,
        function
    ));

    let fn_name = function.split('.').last().unwrap_or(function);
    result.push_str("```python\n");
    result.push_str(&format!("try:\n    result = {}()\n", fn_name));
    result.push_str(&format!(
        "except {} as e:\n    # {} ({})\n    pass\n",
        exception,
        strategy.as_str(),
        if retryable { "retryable" } else { "not retryable" }
    ));
    result.push_str("```\n");

    Ok(result)
}

// ============================================================================
// CROSS-FUNCTION Queries
// ============================================================================

pub fn query_groups(package: Option<&str>) -> Result<String, QueryError> {
    let db = load_database()?;

    if db.grouping_suggestions.is_empty() {
        return Ok("No grouping suggestions. Run 'arbor analyze' first.".to_string());
    }

    let pkg_name = package.unwrap_or("all packages");
    let mut result = format!("# Exception Grouping Suggestions for `{}`\n\n", pkg_name);
    result.push_str("These groupings are automatically generated for error handling.\n");
    result.push_str("Each group contains exceptions that should be handled with the same recovery strategy.\n\n");
    result.push_str("---\n\n");

    let mut found_any = false;

    for suggestion in db.grouping_suggestions.values() {
        if let Some(pkg) = package {
            if !suggestion.group_name.to_lowercase().contains(&pkg.to_lowercase()) {
                continue;
            }
        }

        found_any = true;

        let first_exc = suggestion.exceptions.first().map(|s| s.as_str()).unwrap_or("");
        let strategy = RecoveryStrategy::from_exception_type(first_exc);
        let retryable = matches!(strategy, RecoveryStrategy::Retry);

        result.push_str(&format!("## {}\n\n", suggestion.group_name));
        result.push_str(&format!("**Retryable:** {}\n", if retryable { "Yes" } else { "No" }));
        result.push_str(&format!("**Reason:** {}\n", suggestion.rationale));
        result.push_str(&format!("**Recovery:** {}\n\n", strategy.as_str()));

        result.push_str("| Exception | Recovery Strategy |\n");
        result.push_str("|-----------|------------------|\n");

        for exc in &suggestion.exceptions {
            let exc_strategy = RecoveryStrategy::from_exception_type(exc);
            result.push_str(&format!("| `{}` | {} |\n", exc, exc_strategy.as_str()));
        }

        result.push_str("\n**Recommended Handler:**\n");
        result.push_str(&format!("```python\n{}\n```\n\n", suggestion.handler_example));
        result.push_str("---\n\n");
    }

    if !found_any {
        result.push_str(&format!("No grouping suggestions found for '{}'.\n", pkg_name));
    }

    Ok(result)
}

pub fn query_exception(exc_type: &str) -> Result<String, QueryError> {
    let db = load_database()?;

    struct Occurrence {
        function: String,
        file: PathBuf,
        line: u32,
        condition: Option<String>,
    }

    let mut occurrences: Vec<Occurrence> = Vec::new();
    let mut definition_loc: Option<String> = None;
    let mut qualified_name: Option<String> = None;

    for (fn_id, analysis) in &db.functions {
        for raise in &analysis.raises {
            if raise.exception_type == exc_type || raise.qualified_type == exc_type {
                if definition_loc.is_none() {
                    definition_loc = raise.definition_location.as_ref().map(|l| l.to_string_short());
                    qualified_name = Some(raise.qualified_type.clone());
                }

                occurrences.push(Occurrence {
                    function: fn_id.clone(),
                    file: raise.raise_location.file.clone(),
                    line: raise.raise_location.line,
                    condition: raise.condition.clone(),
                });
            }
        }
    }

    if occurrences.is_empty() {
        return Ok(format!("Exception `{}` not found in analyzed functions.", exc_type));
    }

    let strategy = RecoveryStrategy::from_exception_type(exc_type);
    let retryable = matches!(strategy, RecoveryStrategy::Retry);

    let mut result = format!("# Exception: `{}`\n\n", exc_type);

    result.push_str("## Definition\n\n");
    result.push_str("| Property | Value |\n");
    result.push_str("|----------|-------|\n");
    result.push_str(&format!("| **Short Name** | {} |\n", exc_type));
    result.push_str(&format!(
        "| **Qualified Name** | `{}` |\n",
        qualified_name.as_deref().unwrap_or(exc_type)
    ));
    result.push_str(&format!(
        "| **Defined At** | `{}` |\n",
        definition_loc.as_deref().unwrap_or("(builtin)")
    ));
    result.push_str(&format!("| **Recovery** | {} |\n", strategy.as_str()));
    result.push_str(&format!(
        "| **Retryable** | {} |\n",
        if retryable { "Yes" } else { "No" }
    ));
    result.push('\n');

    result.push_str("## Where It's Raised\n\n");
    result.push_str("| Location | Function | Condition |\n");
    result.push_str("|----------|----------|-----------|\n");

    for occ in &occurrences {
        let loc = format!(
            "{}:{}",
            occ.file.file_name().unwrap_or_default().to_string_lossy(),
            occ.line
        );
        let cond = occ.condition.as_deref().unwrap_or("-");
        result.push_str(&format!("| `{}` | `{}` | {} |\n", loc, occ.function, cond));
    }
    result.push('\n');

    let mut unique_functions: Vec<&str> = occurrences.iter().map(|o| o.function.as_str()).collect();
    unique_functions.sort();
    unique_functions.dedup();

    result.push_str("## Functions That Can Raise This\n\n");
    result.push_str("| Function | Occurrences |\n");
    result.push_str("|----------|-------------|\n");

    for func in &unique_functions {
        let count = occurrences.iter().filter(|o| o.function == *func).count();
        result.push_str(&format!("| `{}` | {} |\n", func, count));
    }
    result.push('\n');

    result.push_str("## Suggested Group\n\n");

    let mut found_group = false;
    for suggestion in db.grouping_suggestions.values() {
        if suggestion.exceptions.contains(&exc_type.to_string()) {
            found_group = true;
            result.push_str(&format!(
                "This exception belongs to the **{}** group.\n\n",
                suggestion.group_name
            ));
            result.push_str(&format!("**Reason:** {}\n\n", suggestion.rationale));

            let others: Vec<_> = suggestion
                .exceptions
                .iter()
                .filter(|e| *e != exc_type)
                .collect();
            if !others.is_empty() {
                result.push_str("**Other exceptions in this group:**\n");
                for other in others {
                    result.push_str(&format!("- `{}`\n", other));
                }
            }
            break;
        }
    }

    if !found_group {
        result.push_str(&format!(
            "No grouping suggestion found. Suggested recovery: **{}**\n",
            strategy.as_str()
        ));
    }

    Ok(result)
}

pub fn query_package(name: &str) -> Result<String, QueryError> {
    let db = load_database()?;

    struct ExceptionInfo {
        exception_type: String,
        qualified_type: String,
        definition_file: Option<String>,
        occurrences: usize,
    }

    let mut exception_map: std::collections::HashMap<String, ExceptionInfo> =
        std::collections::HashMap::new();
    let mut functions: Vec<(String, usize, usize)> = Vec::new(); // (name, exceptions, none_sources)

    for (fn_id, analysis) in &db.functions {
        if fn_id.starts_with(name) || fn_id.contains(&format!(".{}.", name)) {
            functions.push((
                fn_id.clone(),
                analysis.exception_count(),
                analysis.none_source_count(),
            ));

            for raise in &analysis.raises {
                let entry = exception_map
                    .entry(raise.exception_type.clone())
                    .or_insert_with(|| ExceptionInfo {
                        exception_type: raise.exception_type.clone(),
                        qualified_type: raise.qualified_type.clone(),
                        definition_file: raise.definition_location.as_ref().map(|l| {
                            l.file
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string()
                        }),
                        occurrences: 0,
                    });
                entry.occurrences += 1;
            }
        }
    }

    if functions.is_empty() {
        return Ok(format!(
            "Package `{}` not found in analyzed functions.\n\nTry `arbor query search {}` to find related functions.",
            name, name
        ));
    }

    let mut result = format!("# Package Analysis: `{}`\n\n", name);

    let total_exceptions: usize = functions.iter().map(|(_, e, _)| e).sum();
    let total_none: usize = functions.iter().map(|(_, _, n)| n).sum();

    result.push_str("## Summary\n\n");
    result.push_str("| Metric | Count |\n");
    result.push_str("|--------|-------|\n");
    result.push_str(&format!("| Functions analyzed | {} |\n", functions.len()));
    result.push_str(&format!("| Unique exception types | {} |\n", exception_map.len()));
    result.push_str(&format!("| Total exception occurrences | {} |\n", total_exceptions));
    result.push_str(&format!("| Total None sources | {} |\n", total_none));
    result.push('\n');

    if !exception_map.is_empty() {
        result.push_str("## Exceptions Defined\n\n");
        result.push_str("| Exception | Qualified Type | Definition | Occurrences | Recovery |\n");
        result.push_str("|-----------|----------------|------------|-------------|----------|\n");

        let mut exceptions: Vec<_> = exception_map.values().collect();
        exceptions.sort_by(|a, b| b.occurrences.cmp(&a.occurrences));

        for exc in exceptions {
            let strategy = RecoveryStrategy::from_exception_type(&exc.exception_type);
            result.push_str(&format!(
                "| `{}` | `{}` | {} | {} | {} |\n",
                exc.exception_type,
                exc.qualified_type,
                exc.definition_file.as_deref().unwrap_or("(builtin)"),
                exc.occurrences,
                strategy.as_str()
            ));
        }
        result.push('\n');
    }

    result.push_str("## Functions\n\n");
    result.push_str("| Function | Exceptions | None Sources | Risk |\n");
    result.push_str("|----------|------------|--------------|------|\n");

    functions.sort_by(|a, b| a.0.cmp(&b.0));

    for (fn_id, exc_count, none_count) in &functions {
        let analysis = db.get_function(fn_id);
        let risk = analysis
            .map(|a| a.risk_level())
            .unwrap_or(crate::core::types::RiskLevel::Low);
        result.push_str(&format!(
            "| `{}` | {} | {} | {} {} |\n",
            fn_id,
            exc_count,
            none_count,
            risk.emoji(),
            risk.as_str()
        ));
    }
    result.push('\n');

    result.push_str("## Suggested Groups\n\n");

    let mut found_groups = false;
    for suggestion in db.grouping_suggestions.values() {
        let has_package_exc = exception_map.keys().any(|e| suggestion.exceptions.contains(e));
        if has_package_exc {
            found_groups = true;
            let first_exc = suggestion.exceptions.first().map(|s| s.as_str()).unwrap_or("");
            let strategy = RecoveryStrategy::from_exception_type(first_exc);
            let retryable = matches!(strategy, RecoveryStrategy::Retry);

            result.push_str(&format!(
                "- **{}**: {} ({})\n",
                suggestion.group_name,
                suggestion.exceptions.join(", "),
                if retryable { "retryable" } else { "not retryable" }
            ));
        }
    }

    if !found_groups {
        result.push_str("No grouping suggestions available for this package.\n");
    }

    Ok(result)
}

pub fn query_list() -> Result<String, QueryError> {
    let db = load_database()?;

    if db.functions.is_empty() {
        return Ok("No functions analyzed. Run 'arbor analyze <function>' first.".to_string());
    }

    let mut result = format!("# Analyzed Functions\n\n");
    result.push_str(&format!("**Database:** `{}/{}`\n", paths::ARBOR_DIR, paths::DATABASE_FILE));
    result.push_str(&format!("**Total Functions:** {}\n", db.functions.len()));
    result.push_str(&format!(
        "**Last Updated:** {}\n\n",
        db.updated_at.format("%Y-%m-%d %H:%M:%S")
    ));

    let mut packages: std::collections::HashMap<String, Vec<(&String, &crate::core::types::FunctionAnalysis)>> =
        std::collections::HashMap::new();

    for (fn_id, analysis) in &db.functions {
        let package = fn_id
            .split('.')
            .next()
            .unwrap_or("unknown")
            .to_string();
        packages.entry(package).or_default().push((fn_id, analysis));
    }

    let mut package_names: Vec<_> = packages.keys().collect();
    package_names.sort();

    result.push_str("## By Package\n\n");

    for package in package_names {
        let functions = packages.get(package).unwrap();
        result.push_str(&format!("### {} ({} functions)\n\n", package, functions.len()));
        result.push_str("| Function | Exceptions | None Sources | Risk |\n");
        result.push_str("|----------|------------|--------------|------|\n");

        let mut sorted_functions = functions.clone();
        sorted_functions.sort_by_key(|(id, _)| id.as_str());

        for (fn_id, analysis) in sorted_functions {
            let risk = analysis.risk_level();
            let short_name = fn_id
                .strip_prefix(&format!("{}.", package))
                .unwrap_or(fn_id);
            result.push_str(&format!(
                "| `{}` | {} | {} | {} {} |\n",
                short_name,
                analysis.exception_count(),
                analysis.none_source_count(),
                risk.emoji(),
                risk.as_str()
            ));
        }
        result.push('\n');
    }

    result.push_str("---\n\n");
    result.push_str("## Quick Commands\n\n");
    result.push_str("```bash\n");
    result.push_str("# Analyze a new function\n");
    result.push_str("arbor analyze <function>\n\n");
    result.push_str("# Get details on any function\n");
    result.push_str("arbor query function <function>\n");
    result.push_str("```\n");

    Ok(result)
}

pub fn query_search(query: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let query_lower = query.to_lowercase();

    struct SearchMatch {
        name: String,
        is_analyzed: bool,
        exceptions: usize,
        none_sources: usize,
        risk: Option<crate::core::types::RiskLevel>,
        location: Option<String>,
    }

    let mut matches: Vec<SearchMatch> = Vec::new();

    for (fn_id, analysis) in &db.functions {
        if fn_id.to_lowercase().contains(&query_lower) {
            matches.push(SearchMatch {
                name: fn_id.clone(),
                is_analyzed: true,
                exceptions: analysis.exception_count(),
                none_sources: analysis.none_source_count(),
                risk: Some(analysis.risk_level()),
                location: Some(analysis.location.to_string_short()),
            });
        }
    }

    for (symbol, loc) in &db.symbol_index.symbols {
        if symbol.to_lowercase().contains(&query_lower) {
            if !matches.iter().any(|m| m.name == *symbol) {
                matches.push(SearchMatch {
                    name: symbol.clone(),
                    is_analyzed: false,
                    exceptions: 0,
                    none_sources: 0,
                    risk: None,
                    location: Some(format!("{}:{}", loc.file_path.display(), loc.line_start)),
                });
            }
        }
    }

    let mut exception_matches: Vec<String> = Vec::new();
    for analysis in db.functions.values() {
        for raise in &analysis.raises {
            if raise.exception_type.to_lowercase().contains(&query_lower)
                || raise.qualified_type.to_lowercase().contains(&query_lower)
            {
                if !exception_matches.contains(&raise.exception_type) {
                    exception_matches.push(raise.exception_type.clone());
                }
            }
        }
    }

    if matches.is_empty() && exception_matches.is_empty() {
        return Ok(format!("No matches for '{}'\n\nTry a different search term.", query));
    }

    let mut result = format!("# Search Results\n\n");
    result.push_str(&format!("**Query:** `{}`\n", query));
    result.push_str(&format!(
        "**Results:** {} functions, {} exceptions\n\n",
        matches.len(),
        exception_matches.len()
    ));

    if !matches.is_empty() {
        result.push_str("## Functions\n\n");

        let analyzed: Vec<_> = matches.iter().filter(|m| m.is_analyzed).collect();
        let unanalyzed: Vec<_> = matches.iter().filter(|m| !m.is_analyzed).collect();

        if !analyzed.is_empty() {
            result.push_str("### Analyzed\n\n");
            result.push_str("| Function | Exceptions | None | Risk |\n");
            result.push_str("|----------|------------|------|------|\n");

            for m in analyzed.iter().take(25) {
                let risk = m.risk.as_ref().unwrap();
                result.push_str(&format!(
                    "| `{}` | {} | {} | {} {} |\n",
                    m.name,
                    m.exceptions,
                    m.none_sources,
                    risk.emoji(),
                    risk.as_str()
                ));
            }

            if analyzed.len() > 25 {
                result.push_str(&format!("\n*... and {} more analyzed functions*\n", analyzed.len() - 25));
            }
            result.push('\n');
        }

        if !unanalyzed.is_empty() {
            result.push_str("### Not Analyzed\n\n");

            for m in unanalyzed.iter().take(25) {
                result.push_str(&format!(
                    "- `{}` - {}\n",
                    m.name,
                    m.location.as_deref().unwrap_or("unknown location")
                ));
            }

            if unanalyzed.len() > 25 {
                result.push_str(&format!("\n*... and {} more unanalyzed functions*\n", unanalyzed.len() - 25));
            }
            result.push('\n');
        }
    }

    if !exception_matches.is_empty() {
        result.push_str("## Exceptions Matching Query\n\n");

        for exc in exception_matches.iter().take(20) {
            let strategy = RecoveryStrategy::from_exception_type(exc);
            result.push_str(&format!("- `{}` ({})\n", exc, strategy.as_str()));
        }

        if exception_matches.len() > 20 {
            result.push_str(&format!("\n*... and {} more exceptions*\n", exception_matches.len() - 20));
        }
    }

    result.push_str("\n---\n\n");
    result.push_str("**Tips:**\n");
    result.push_str("- Use `arbor query function <name>` for full analysis\n");
    result.push_str("- Use `arbor analyze <name>` to analyze unanalyzed functions\n");

    Ok(result)
}

pub fn query_stats() -> Result<String, QueryError> {
    let db = load_database()?;

    let total_none: usize = db.functions.values().map(|a| a.none_source_count()).sum();

    let high_risk = db
        .functions
        .values()
        .filter(|a| a.risk_level() == crate::core::types::RiskLevel::High)
        .count();
    let medium_risk = db
        .functions
        .values()
        .filter(|a| a.risk_level() == crate::core::types::RiskLevel::Medium)
        .count();
    let low_risk = db
        .functions
        .values()
        .filter(|a| a.risk_level() == crate::core::types::RiskLevel::Low)
        .count();

    let mut unique_exceptions: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for analysis in db.functions.values() {
        for raise in &analysis.raises {
            unique_exceptions.insert(&raise.exception_type);
        }
    }

    let mut packages: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for fn_id in db.functions.keys() {
        if let Some(pkg) = fn_id.split('.').next() {
            packages.insert(pkg);
        }
    }

    let mut exception_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for analysis in db.functions.values() {
        for raise in &analysis.raises {
            *exception_counts.entry(&raise.exception_type).or_insert(0) += 1;
        }
    }

    let mut result = String::from("# Arbor Database Statistics\n\n");
    result.push_str(&format!("**Database:** `{}/{}`\n", paths::ARBOR_DIR, paths::DATABASE_FILE));
    result.push_str(&format!("**Version:** {}\n", db.version));
    result.push_str(&format!(
        "**Created:** {}\n",
        db.created_at.format("%Y-%m-%d %H:%M:%S")
    ));
    result.push_str(&format!(
        "**Updated:** {}\n\n",
        db.updated_at.format("%Y-%m-%d %H:%M:%S")
    ));

    result.push_str("## Summary\n\n");
    result.push_str("| Metric | Count |\n");
    result.push_str("|--------|-------|\n");
    result.push_str(&format!("| Functions analyzed | {} |\n", db.function_count()));
    result.push_str(&format!("| Symbols indexed | {} |\n", db.symbol_count()));
    result.push_str(&format!("| Unique exceptions | {} |\n", unique_exceptions.len()));
    result.push_str(&format!("| Unique None sources | {} |\n", total_none));
    result.push_str(&format!("| Packages covered | {} |\n", packages.len()));
    result.push_str(&format!(
        "| Grouping suggestions | {} |\n",
        db.grouping_suggestions.len()
    ));
    result.push('\n');

    let total_functions = db.function_count();
    result.push_str("## By Risk Level\n\n");
    result.push_str("| Risk | Functions | Percentage |\n");
    result.push_str("|------|-----------|------------|\n");

    if total_functions > 0 {
        result.push_str(&format!(
            "| 🔴 High | {} | {}% |\n",
            high_risk,
            (high_risk * 100) / total_functions
        ));
        result.push_str(&format!(
            "| 🟡 Medium | {} | {}% |\n",
            medium_risk,
            (medium_risk * 100) / total_functions
        ));
        result.push_str(&format!(
            "| 🟢 Low | {} | {}% |\n",
            low_risk,
            (low_risk * 100) / total_functions
        ));
    } else {
        result.push_str("| - | 0 | 0% |\n");
    }
    result.push('\n');

    if !exception_counts.is_empty() {
        result.push_str("## Top Exceptions\n\n");
        result.push_str("| Exception | Occurrences | Recovery |\n");
        result.push_str("|-----------|-------------|----------|\n");

        let mut sorted_exceptions: Vec<_> = exception_counts.iter().collect();
        sorted_exceptions.sort_by(|a, b| b.1.cmp(a.1));

        for (exc, count) in sorted_exceptions.iter().take(10) {
            let strategy = RecoveryStrategy::from_exception_type(exc);
            result.push_str(&format!("| `{}` | {} | {} |\n", exc, count, strategy.as_str()));
        }

        if sorted_exceptions.len() > 10 {
            result.push_str(&format!(
                "\n*... and {} more exception types*\n",
                sorted_exceptions.len() - 10
            ));
        }
        result.push('\n');
    }

    if !db.grouping_suggestions.is_empty() {
        result.push_str("## Grouping Suggestions Available\n\n");
        result.push_str("| Group | Exceptions | Retryable |\n");
        result.push_str("|-------|------------|----------|\n");

        for suggestion in db.grouping_suggestions.values() {
            let first_exc = suggestion.exceptions.first().map(|s| s.as_str()).unwrap_or("");
            let strategy = RecoveryStrategy::from_exception_type(first_exc);
            let retryable = matches!(strategy, RecoveryStrategy::Retry);

            result.push_str(&format!(
                "| {} | {} | {} |\n",
                suggestion.group_name,
                suggestion.exceptions.len(),
                if retryable { "Yes" } else { "No" }
            ));
        }
        result.push('\n');
    }

    result.push_str("---\n\n");
    result.push_str("## Commands\n\n");
    result.push_str("```bash\n");
    result.push_str("# Refresh all analyzed functions\n");
    result.push_str("arbor refresh\n\n");
    result.push_str("# List all analyzed functions\n");
    result.push_str("arbor query list\n\n");
    result.push_str("# View grouping suggestions\n");
    result.push_str("arbor query groups\n");
    result.push_str("```\n");

    Ok(result)
}

pub fn query_quickref() -> String {
    r#"
Arbor Query Commands - Quick Reference

LOCAL (Entity-Level) Queries:
  arbor query risk <function>           One-line risk summary
  arbor query has <function> <exc>      Check if function raises exception
  arbor query handle <function>         Generate try/except block
  arbor query signature <function>      Function signature + location
  arbor query one-exception <fn> <exc>  Single exception details
  arbor query one-none <fn> <idx>       Single None source details
  arbor query callers <function>        What calls this function
  arbor query callees <function>        What this function calls
  arbor query diff <function>           Compare current vs previous

FULL ANALYSIS Queries:
  arbor query exceptions <function>     All exceptions with locations
  arbor query none <function>           All None sources
  arbor query function <function>       Complete function summary
  arbor query chain <function> <exc>    Call chain visualization

CROSS-FUNCTION Queries:
  arbor query groups [package]          Grouping suggestions
  arbor query exception <type>          Exception type details
  arbor query package <name>            Package exception analysis
  arbor query list                      All analyzed functions
  arbor query search <query>            Search with filters
  arbor query stats                     Database statistics

OUTPUT FORMAT:
  arbor query -f json <subcommand>      Output as JSON
  arbor query -f markdown <subcommand>  Output as Markdown (default)
"#
    .to_string()
}

// ============================================================================
// JSON Output Variants
// ============================================================================

use serde::Serialize;

#[derive(Serialize)]
struct RiskJson {
    function: String,
    risk_level: String,
    risk_emoji: String,
    exception_count: usize,
    none_source_count: usize,
    call_depth: usize,
}

pub fn query_risk_json(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    let risk = analysis.risk_level();
    let output = RiskJson {
        function: function.to_string(),
        risk_level: risk.as_str().to_string(),
        risk_emoji: risk.emoji().to_string(),
        exception_count: analysis.exception_count(),
        none_source_count: analysis.none_source_count(),
        call_depth: analysis.call_depth,
    };

    serde_json::to_string_pretty(&output)
        .map_err(|e| QueryError::InvalidQuery(e.to_string()))
}

pub fn query_exceptions_json(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    serde_json::to_string_pretty(&analysis.raises)
        .map_err(|e| QueryError::InvalidQuery(e.to_string()))
}

pub fn query_none_json(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    serde_json::to_string_pretty(&analysis.none_sources)
        .map_err(|e| QueryError::InvalidQuery(e.to_string()))
}

pub fn query_function_json(function: &str) -> Result<String, QueryError> {
    let db = load_database()?;
    let analysis = db
        .get_function(function)
        .ok_or_else(|| QueryError::FunctionNotFound(function.to_string()))?;

    serde_json::to_string_pretty(analysis)
        .map_err(|e| QueryError::InvalidQuery(e.to_string()))
}

pub fn query_groups_json(package: Option<&str>) -> Result<String, QueryError> {
    let db = load_database()?;

    let groups: Vec<_> = if let Some(pkg) = package {
        db.grouping_suggestions
            .values()
            .filter(|s| s.group_name.starts_with(pkg) || s.exceptions.iter().any(|e| e.starts_with(pkg)))
            .collect()
    } else {
        db.grouping_suggestions.values().collect()
    };

    serde_json::to_string_pretty(&groups)
        .map_err(|e| QueryError::InvalidQuery(e.to_string()))
}

#[derive(Serialize)]
struct FunctionSummary {
    function_id: String,
    exception_count: usize,
    none_source_count: usize,
    risk_level: String,
    location: String,
}

pub fn query_list_json() -> Result<String, QueryError> {
    let db = load_database()?;

    let functions: Vec<FunctionSummary> = db
        .functions
        .iter()
        .map(|(id, analysis)| FunctionSummary {
            function_id: id.clone(),
            exception_count: analysis.exception_count(),
            none_source_count: analysis.none_source_count(),
            risk_level: analysis.risk_level().as_str().to_string(),
            location: analysis.location.to_string_short(),
        })
        .collect();

    serde_json::to_string_pretty(&functions)
        .map_err(|e| QueryError::InvalidQuery(e.to_string()))
}

#[derive(Serialize)]
struct StatsJson {
    version: String,
    created_at: String,
    updated_at: String,
    function_count: usize,
    symbol_count: usize,
    unique_exceptions: usize,
    unique_none_sources: usize,
    package_count: usize,
    group_count: usize,
    risk_distribution: RiskDistribution,
}

#[derive(Serialize)]
struct RiskDistribution {
    high: usize,
    medium: usize,
    low: usize,
}

pub fn query_stats_json() -> Result<String, QueryError> {
    let db = load_database()?;

    let total_none: usize = db.functions.values().map(|a| a.none_source_count()).sum();

    let high_risk = db.functions.values()
        .filter(|a| a.risk_level() == crate::core::types::RiskLevel::High)
        .count();
    let medium_risk = db.functions.values()
        .filter(|a| a.risk_level() == crate::core::types::RiskLevel::Medium)
        .count();
    let low_risk = db.functions.values()
        .filter(|a| a.risk_level() == crate::core::types::RiskLevel::Low)
        .count();

    let mut unique_exceptions: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for analysis in db.functions.values() {
        for raise in &analysis.raises {
            unique_exceptions.insert(&raise.exception_type);
        }
    }

    let mut packages: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for fn_id in db.functions.keys() {
        if let Some(pkg) = fn_id.split('.').next() {
            packages.insert(pkg);
        }
    }

    let stats = StatsJson {
        version: db.version.clone(),
        created_at: db.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        updated_at: db.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        function_count: db.function_count(),
        symbol_count: db.symbol_count(),
        unique_exceptions: unique_exceptions.len(),
        unique_none_sources: total_none,
        package_count: packages.len(),
        group_count: db.grouping_suggestions.len(),
        risk_distribution: RiskDistribution {
            high: high_risk,
            medium: medium_risk,
            low: low_risk,
        },
    };

    serde_json::to_string_pretty(&stats)
        .map_err(|e| QueryError::InvalidQuery(e.to_string()))
}
