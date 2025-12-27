use crate::analysis::grouping::RecoveryStrategy;
use crate::core::database::GroupingSuggestion;
use crate::core::paths;
use crate::core::types::{FunctionAnalysis, NoneSource, RaiseStatement, RiskLevel};

pub trait MarkdownOutput {
    fn to_markdown(&self) -> String;

    fn to_markdown_summary(&self) -> String {
        self.to_markdown()
    }

    fn to_markdown_detailed(&self) -> String {
        self.to_markdown()
    }
}

pub struct MarkdownTable {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl MarkdownTable {
    pub fn new(headers: Vec<&str>) -> Self {
        Self {
            headers: headers.into_iter().map(String::from).collect(),
            rows: Vec::new(),
        }
    }

    pub fn add_row(&mut self, row: Vec<&str>) {
        self.rows.push(row.into_iter().map(String::from).collect());
    }

    pub fn render(&self) -> String {
        if self.headers.is_empty() {
            return String::new();
        }

        let mut output = String::new();

        output.push_str("| ");
        output.push_str(&self.headers.join(" | "));
        output.push_str(" |\n");

        output.push_str("|");
        for _ in &self.headers {
            output.push_str("------|");
        }
        output.push('\n');

        for row in &self.rows {
            output.push_str("| ");
            output.push_str(&row.join(" | "));
            output.push_str(" |\n");
        }

        output
    }
}

pub fn format_risk(risk: RiskLevel) -> String {
    format!("{} {}", risk.emoji(), risk.as_str())
}

pub fn format_recovery(strategy: RecoveryStrategy) -> String {
    let retryable = matches!(strategy, RecoveryStrategy::Retry);
    format!(
        "{} ({})",
        strategy.as_str(),
        if retryable { "retryable" } else { "not retryable" }
    )
}

pub fn format_code_block(code: &str, lang: &str) -> String {
    format!("```{}\n{}\n```", lang, code)
}

pub fn format_header(level: usize, text: &str) -> String {
    format!("{} {}\n", "#".repeat(level), text)
}

pub fn format_key_value(key: &str, value: &str) -> String {
    format!("**{}:** {}\n", key, value)
}

pub fn format_list_item(text: &str) -> String {
    format!("- {}\n", text)
}

impl MarkdownOutput for FunctionAnalysis {
    fn to_markdown(&self) -> String {
        self.to_markdown_summary()
    }

    fn to_markdown_summary(&self) -> String {
        let risk = self.risk_level();
        format!(
            "{} {} | {} exceptions, {} None sources | depth: {}",
            risk.emoji(),
            risk.as_str(),
            self.exception_count(),
            self.none_source_count(),
            self.call_depth
        )
    }

    fn to_markdown_detailed(&self) -> String {
        let mut output = String::new();
        let risk = self.risk_level();

        output.push_str(&format_header(1, &format!("Function Analysis: `{}`", self.function_id)));
        output.push('\n');

        output.push_str(&format_header(2, "Overview"));
        output.push('\n');

        let mut table = MarkdownTable::new(vec!["Property", "Value"]);
        table.add_row(vec!["**Qualified Name**", &format!("`{}`", self.function_id)]);
        table.add_row(vec!["**Signature**", &format!("`{}`", self.signature)]);
        table.add_row(vec!["**File**", &format!("`{}`", self.location.file.display())]);
        table.add_row(vec!["**Line**", &self.location.line.to_string()]);
        table.add_row(vec!["**Risk**", &format_risk(risk)]);
        output.push_str(&table.render());
        output.push('\n');

        output.push_str(&format_header(2, "Analysis Summary"));
        output.push('\n');

        let mut summary = MarkdownTable::new(vec!["Metric", "Count"]);
        summary.add_row(vec!["Exceptions", &self.raises.len().to_string()]);
        summary.add_row(vec!["None sources", &self.none_sources.len().to_string()]);
        summary.add_row(vec!["Functions traced", &self.functions_traced.to_string()]);
        summary.add_row(vec!["Call depth", &self.call_depth.to_string()]);
        output.push_str(&summary.render());
        output.push('\n');

        if !self.raises.is_empty() {
            output.push_str(&format_header(2, "Exceptions"));
            output.push('\n');

            let mut exc_table = MarkdownTable::new(vec!["Type", "Location", "Recovery"]);
            for raise in &self.raises {
                let strategy = RecoveryStrategy::from_exception_type(&raise.exception_type);
                exc_table.add_row(vec![
                    &format!("`{}`", raise.exception_type),
                    &raise.raise_location.to_string_short(),
                    strategy.as_str(),
                ]);
            }
            output.push_str(&exc_table.render());
            output.push('\n');
        }

        if !self.none_sources.is_empty() {
            output.push_str(&format_header(2, "None Sources"));
            output.push('\n');

            let mut none_table = MarkdownTable::new(vec!["Kind", "Location", "Condition"]);
            for source in &self.none_sources {
                none_table.add_row(vec![
                    source.kind.as_str(),
                    &source.location.to_string_short(),
                    source.condition.as_deref().unwrap_or("-"),
                ]);
            }
            output.push_str(&none_table.render());
        }

        output
    }
}

impl MarkdownOutput for RaiseStatement {
    fn to_markdown(&self) -> String {
        self.to_markdown_summary()
    }

    fn to_markdown_summary(&self) -> String {
        let strategy = RecoveryStrategy::from_exception_type(&self.exception_type);
        format!(
            "`{}` at {} ({})",
            self.exception_type,
            self.raise_location.to_string_short(),
            strategy.as_str()
        )
    }

    fn to_markdown_detailed(&self) -> String {
        let mut output = String::new();
        let strategy = RecoveryStrategy::from_exception_type(&self.exception_type);
        let retryable = matches!(strategy, RecoveryStrategy::Retry);

        output.push_str(&format_header(3, &self.exception_type));
        output.push('\n');

        output.push_str(&format_key_value("Type", &format!("`{}`", self.qualified_type)));
        output.push_str(&format_key_value(
            "Raised at",
            &format!("`{}`", self.raise_location.to_string_short()),
        ));

        if let Some(ref def) = self.definition_location {
            output.push_str(&format_key_value("Defined at", &format!("`{}`", def.to_string_short())));
        } else {
            output.push_str(&format_key_value("Defined at", "(builtin)"));
        }

        if let Some(ref cond) = self.condition {
            output.push_str(&format_key_value("Condition", cond));
        }

        if let Some(ref msg) = self.message {
            output.push_str(&format_key_value("Message", &format!("\"{}\"", msg)));
        }

        output.push_str(&format_key_value(
            "Recovery",
            &format!(
                "{} ({})",
                strategy.as_str(),
                if retryable { "retryable" } else { "not retryable" }
            ),
        ));

        output
    }
}

impl MarkdownOutput for NoneSource {
    fn to_markdown(&self) -> String {
        self.to_markdown_summary()
    }

    fn to_markdown_summary(&self) -> String {
        format!(
            "{} at {}",
            self.kind.as_str(),
            self.location.to_string_short()
        )
    }

    fn to_markdown_detailed(&self) -> String {
        let mut output = String::new();

        output.push_str(&format_header(
            3,
            &format!("{} at {}", self.kind.as_str(), self.location.to_string_short()),
        ));
        output.push('\n');

        output.push_str(&format_key_value("Kind", &format!("`{}`", self.kind.as_str())));
        output.push_str(&format_key_value(
            "Location",
            &format!("`{}`", self.location.to_string_short()),
        ));

        if let Some(ref def) = self.source_definition {
            output.push_str(&format_key_value("Source", &format!("`{}`", def.to_string_short())));
        }

        if let Some(ref cond) = self.condition {
            output.push_str(&format_key_value("Condition", cond));
        }

        output
    }
}

impl MarkdownOutput for GroupingSuggestion {
    fn to_markdown(&self) -> String {
        self.to_markdown_summary()
    }

    fn to_markdown_summary(&self) -> String {
        format!(
            "**{}**: {} exceptions",
            self.group_name,
            self.exceptions.len()
        )
    }

    fn to_markdown_detailed(&self) -> String {
        let mut output = String::new();

        let first_exc = self.exceptions.first().map(|s| s.as_str()).unwrap_or("");
        let strategy = RecoveryStrategy::from_exception_type(first_exc);
        let retryable = matches!(strategy, RecoveryStrategy::Retry);

        output.push_str(&format_header(2, &self.group_name));
        output.push('\n');

        output.push_str(&format_key_value(
            "Retryable",
            if retryable { "Yes" } else { "No" },
        ));
        output.push_str(&format_key_value("Reason", &self.rationale));
        output.push_str(&format_key_value("Recovery", strategy.as_str()));
        output.push('\n');

        let mut table = MarkdownTable::new(vec!["Exception", "Recovery Strategy"]);
        for exc in &self.exceptions {
            let exc_strategy = RecoveryStrategy::from_exception_type(exc);
            table.add_row(vec![&format!("`{}`", exc), exc_strategy.as_str()]);
        }
        output.push_str(&table.render());
        output.push('\n');

        output.push_str("**Recommended Handler:**\n");
        output.push_str(&format_code_block(&self.handler_example, "python"));

        output
    }
}

pub struct DatabaseStats {
    pub version: String,
    pub created_at: String,
    pub updated_at: String,
    pub function_count: usize,
    pub symbol_count: usize,
    pub unique_exceptions: usize,
    pub unique_none_sources: usize,
    pub package_count: usize,
    pub group_count: usize,
    pub high_risk: usize,
    pub medium_risk: usize,
    pub low_risk: usize,
}

impl MarkdownOutput for DatabaseStats {
    fn to_markdown(&self) -> String {
        self.to_markdown_detailed()
    }

    fn to_markdown_detailed(&self) -> String {
        let mut output = String::new();

        output.push_str(&format_header(1, "Arbor Database Statistics"));
        output.push('\n');

        output.push_str(&format_key_value("Database", &format!("`{}/{}`", paths::ARBOR_DIR, paths::DATABASE_FILE)));
        output.push_str(&format_key_value("Version", &self.version));
        output.push_str(&format_key_value("Created", &self.created_at));
        output.push_str(&format_key_value("Updated", &self.updated_at));
        output.push('\n');

        output.push_str(&format_header(2, "Summary"));
        output.push('\n');

        let mut summary = MarkdownTable::new(vec!["Metric", "Count"]);
        summary.add_row(vec!["Functions analyzed", &self.function_count.to_string()]);
        summary.add_row(vec!["Symbols indexed", &self.symbol_count.to_string()]);
        summary.add_row(vec!["Unique exceptions", &self.unique_exceptions.to_string()]);
        summary.add_row(vec!["Unique None sources", &self.unique_none_sources.to_string()]);
        summary.add_row(vec!["Packages covered", &self.package_count.to_string()]);
        summary.add_row(vec!["Grouping suggestions", &self.group_count.to_string()]);
        output.push_str(&summary.render());
        output.push('\n');

        output.push_str(&format_header(2, "By Risk Level"));
        output.push('\n');

        let total = self.function_count;
        let mut risk_table = MarkdownTable::new(vec!["Risk", "Functions", "Percentage"]);

        if total > 0 {
            risk_table.add_row(vec![
                "🔴 High",
                &self.high_risk.to_string(),
                &format!("{}%", (self.high_risk * 100) / total),
            ]);
            risk_table.add_row(vec![
                "🟡 Medium",
                &self.medium_risk.to_string(),
                &format!("{}%", (self.medium_risk * 100) / total),
            ]);
            risk_table.add_row(vec![
                "🟢 Low",
                &self.low_risk.to_string(),
                &format!("{}%", (self.low_risk * 100) / total),
            ]);
        }
        output.push_str(&risk_table.render());

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_table() {
        let mut table = MarkdownTable::new(vec!["Name", "Value"]);
        table.add_row(vec!["foo", "bar"]);
        table.add_row(vec!["baz", "qux"]);

        let output = table.render();
        assert!(output.contains("| Name | Value |"));
        assert!(output.contains("| foo | bar |"));
        assert!(output.contains("| baz | qux |"));
    }

    #[test]
    fn test_format_header() {
        assert_eq!(format_header(1, "Title"), "# Title\n");
        assert_eq!(format_header(2, "Section"), "## Section\n");
        assert_eq!(format_header(3, "Subsection"), "### Subsection\n");
    }

    #[test]
    fn test_format_key_value() {
        assert_eq!(format_key_value("Key", "Value"), "**Key:** Value\n");
    }

    #[test]
    fn test_format_code_block() {
        let code = "print('hello')";
        let output = format_code_block(code, "python");
        assert!(output.contains("```python"));
        assert!(output.contains(code));
        assert!(output.ends_with("```"));
    }

    #[test]
    fn test_format_risk() {
        assert!(format_risk(RiskLevel::High).contains("High"));
        assert!(format_risk(RiskLevel::Medium).contains("Medium"));
        assert!(format_risk(RiskLevel::Low).contains("Low"));
    }

    #[test]
    fn test_format_recovery() {
        assert!(format_recovery(RecoveryStrategy::Retry).contains("retryable"));
        assert!(format_recovery(RecoveryStrategy::Abort).contains("not retryable"));
    }
}
