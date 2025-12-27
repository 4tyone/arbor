use crate::analysis::grouping::suggest_groups;
use crate::analysis::traversal::Traverser;
use crate::core::config::ArborConfig;
use crate::core::database::ArborDatabase;
use crate::core::types::FunctionAnalysis;
use crate::plugins::python::resolver::PythonResolver;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AnalyzeError {
    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),

    #[error("Database not found. Run 'arbor init' first.")]
    DatabaseNotFound,

    #[error("Database error: {0}")]
    Database(#[from] crate::core::database::DatabaseError),

    #[error("Traversal error: {0}")]
    Traversal(#[from] crate::analysis::traversal::TraversalError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct AnalyzeArgs {
    pub functions: Vec<String>,
    pub depth: usize,
    pub output_format: OutputFormat,
    pub venv_path: Option<PathBuf>,
}

#[derive(Clone, Copy)]
pub enum OutputFormat {
    Markdown,
    Json,
}

pub fn run_analyze(args: AnalyzeArgs) -> Result<(), AnalyzeError> {
    let config = ArborConfig::load_or_default();

    let db_path = std::env::current_dir()?.join(&config.database.path);

    if !db_path.exists() {
        return Err(AnalyzeError::DatabaseNotFound);
    }

    println!("Loading database...");
    let mut db = ArborDatabase::load(&db_path)?;

    let python_path: Vec<PathBuf> = if config.environment.python_path.is_empty() {
        db.environment
            .python_path
            .iter()
            .map(PathBuf::from)
            .collect()
    } else {
        config.environment.python_path.clone()
    };

    let site_packages: Vec<PathBuf> = if let Some(ref venv) = args.venv_path {
        let venv_site_packages = find_venv_site_packages(venv);
        if venv_site_packages.is_empty() {
            eprintln!("Warning: No site-packages found in venv: {}", venv.display());
        }
        venv_site_packages
    } else if let Some(ref venv) = config.environment.venv_path {
        find_venv_site_packages(venv)
    } else if !config.environment.site_packages.is_empty() {
        config.environment.site_packages.clone()
    } else {
        db.environment
            .site_packages
            .iter()
            .map(PathBuf::from)
            .collect()
    };

    let resolver = PythonResolver::new(python_path, site_packages);

    let max_depth = args.depth;
    let mut traverser = Traverser::new(resolver, max_depth)?
        .with_symbol_index(db.symbol_index.clone());

    for function_id in &args.functions {
        if config.should_ignore_function(function_id) {
            println!("\nSkipping {} (ignored by config)", function_id);
            continue;
        }

        if let Some(package) = function_id.split('.').next() {
            if config.should_ignore_package(package) {
                println!("\nSkipping {} (package {} ignored by config)", function_id, package);
                continue;
            }
        }

        println!("\nAnalyzing {}...", function_id);

        let analysis = traverser.analyze_function(function_id)?;

        if !analysis.raises.is_empty() {
            let suggestions = suggest_groups(&analysis.raises);
            for suggestion in suggestions {
                db.grouping_suggestions.insert(suggestion.group_name.clone(), suggestion);
            }
        }

        print_analysis_summary(&analysis, args.output_format);

        db.functions.insert(function_id.clone(), analysis);
    }

    if !db.grouping_suggestions.is_empty() {
        println!("\n## Grouping Suggestions\n");
        for suggestion in db.grouping_suggestions.values() {
            println!("### {}\n", suggestion.group_name);
            println!("**Exceptions:** {}\n", suggestion.exceptions.join(", "));
            println!("**Rationale:** {}\n", suggestion.rationale);
            println!("```python\n{}\n```\n", suggestion.handler_example);
        }
    }

    db.save(&db_path)?;
    println!("\nResults saved to {}", db_path.display());

    Ok(())
}

fn print_analysis_summary(analysis: &FunctionAnalysis, format: OutputFormat) {
    match format {
        OutputFormat::Markdown => print_markdown(analysis),
        OutputFormat::Json => print_json(analysis),
    }
}

fn print_markdown(analysis: &FunctionAnalysis) {
    let risk = analysis.risk_level();

    println!("\n## {}", analysis.function_id);
    println!();
    println!("**Risk:** {} {}", risk.emoji(), risk.as_str());
    println!("**Location:** {}", analysis.location.to_string_short());
    println!("**Functions traced:** {}", analysis.functions_traced);
    println!("**Max call depth:** {}", analysis.call_depth);
    println!();

    if !analysis.raises.is_empty() {
        println!("### Exceptions ({})", analysis.raises.len());
        println!();
        println!("| Type | Raise Location | Definition | Condition |");
        println!("|------|----------------|------------|-----------|");
        for raise in &analysis.raises {
            let condition = raise.condition.as_deref().unwrap_or("-");
            let def_loc = raise.definition_location.as_ref()
                .map(|loc| format!("{}:{}",
                    loc.file.file_name().unwrap_or_default().to_string_lossy(),
                    loc.line))
                .unwrap_or_else(|| "(builtin)".to_string());
            println!(
                "| `{}` | {}:{} | {} | {} |",
                raise.exception_type,
                raise.raise_location.file.file_name().unwrap_or_default().to_string_lossy(),
                raise.raise_location.line,
                def_loc,
                condition
            );
        }
        println!();
    }

    if !analysis.none_sources.is_empty() {
        println!("### None Sources ({})", analysis.none_sources.len());
        println!();
        println!("| Kind | Location | Condition |");
        println!("|------|----------|-----------|");
        for source in &analysis.none_sources {
            let condition = source.condition.as_deref().unwrap_or("-");
            println!(
                "| {} | {}:{} | {} |",
                source.kind.as_str(),
                source.location.file.file_name().unwrap_or_default().to_string_lossy(),
                source.location.line,
                condition
            );
        }
        println!();
    }
}

fn print_json(analysis: &FunctionAnalysis) {
    match serde_json::to_string_pretty(analysis) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Failed to serialize: {}", e),
    }
}

fn find_venv_site_packages(venv_path: &PathBuf) -> Vec<PathBuf> {
    let mut results = Vec::new();

    let candidates = [
        venv_path.join("lib").join("python3.12").join("site-packages"),
        venv_path.join("lib").join("python3.11").join("site-packages"),
        venv_path.join("lib").join("python3.10").join("site-packages"),
        venv_path.join("lib").join("python3.9").join("site-packages"),
        venv_path.join("lib").join("python3.8").join("site-packages"),
        venv_path.join("Lib").join("site-packages"), // Windows
    ];

    for candidate in &candidates {
        if candidate.exists() {
            results.push(candidate.clone());
            break;
        }
    }

    if results.is_empty() {
        if let Ok(entries) = std::fs::read_dir(venv_path.join("lib")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    if name.starts_with("python") {
                        let site_packages = path.join("site-packages");
                        if site_packages.exists() {
                            results.push(site_packages);
                            break;
                        }
                    }
                }
            }
        }
    }

    results
}
