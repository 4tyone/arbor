use crate::analysis::indexer::Indexer;
use crate::core::database::{ArborDatabase, Environment};
use crate::core::paths;
use crate::plugins::python::resolver::PythonResolver;
use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbCommandError {
    #[error("Database already exists at {0}")]
    AlreadyExists(String),

    #[error("Database not found at {0}")]
    NotFound(String),

    #[error("Failed to detect Python environment: {0}")]
    EnvironmentDetection(String),

    #[error("Indexer error: {0}")]
    Indexer(#[from] crate::analysis::indexer::IndexerError),

    #[error("Database error: {0}")]
    Database(#[from] crate::core::database::DatabaseError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct InitOptions {
    pub force: bool,
    pub index_site_packages: bool,
}

impl Default for InitOptions {
    fn default() -> Self {
        Self {
            force: false,
            index_site_packages: true,
        }
    }
}

pub struct ExportOptions {
    pub output_path: Option<PathBuf>,
    pub format: String,
}

pub fn run_init(options: InitOptions) -> Result<PathBuf, DbCommandError> {
    let db_path = paths::database_path();

    if db_path.exists() && !options.force {
        return Err(DbCommandError::AlreadyExists(db_path.display().to_string()));
    }

    paths::ensure_arbor_dir()?;

    println!("Detecting Python environment...");
    let environment = detect_environment()?;

    println!("Python version: {}", environment.python_version);
    if let Some(ref venv) = environment.venv_path {
        println!("Virtual env: {}", venv);
    }
    println!("Site-packages: {:?}", environment.site_packages);

    let mut db = ArborDatabase::new(environment.clone());

    println!("Indexing Python files...");
    let mut indexer = Indexer::new()?;

    let mut dirs_to_index: Vec<PathBuf> = environment
        .python_path
        .iter()
        .map(PathBuf::from)
        .collect();

    if options.index_site_packages {
        dirs_to_index.extend(
            environment
                .site_packages
                .iter()
                .map(PathBuf::from),
        );
    }

    let index = indexer.index_directories(&dirs_to_index)?;
    println!("Indexed {} symbols", index.len());

    db.symbol_index = index;

    db.save(&db_path)?;
    println!("Created {}", db_path.display());

    let config_path = paths::config_path();
    if !config_path.exists() {
        let config_content = crate::core::config::ArborConfig::default_toml();
        std::fs::write(&config_path, config_content)?;
        println!("Created {}", config_path.display());
    }

    let command_path = paths::commands_dir().join("arbor.md");
    if !command_path.exists() {
        std::fs::write(&command_path, default_command_content())?;
        println!("Created {}", command_path.display());
    }

    Ok(db_path)
}

pub fn run_refresh(functions: Option<Vec<String>>) -> Result<usize, DbCommandError> {
    let db_path = paths::database_path();

    if !db_path.exists() {
        return Err(DbCommandError::NotFound(db_path.display().to_string()));
    }

    println!("Loading database...");
    let mut db = ArborDatabase::load(&db_path)?;

    match functions {
        Some(fn_list) => {
            let mut count = 0;
            for function_id in &fn_list {
                if db.functions.remove(function_id).is_some() {
                    println!("Marked for refresh: {}", function_id);
                    count += 1;
                } else {
                    eprintln!("Warning: {} not found in database", function_id);
                }
            }
            db.save(&db_path)?;
            Ok(count)
        }
        None => {
            println!("Re-indexing Python files...");
            let mut indexer = Indexer::new()?;

            let mut dirs_to_index: Vec<PathBuf> = db
                .environment
                .python_path
                .iter()
                .map(PathBuf::from)
                .collect();

            dirs_to_index.extend(
                db.environment
                    .site_packages
                    .iter()
                    .map(PathBuf::from),
            );

            let index = indexer.index_directories(&dirs_to_index)?;
            let count = index.len();
            println!("Indexed {} symbols", count);

            db.symbol_index = index;
            db.save(&db_path)?;
            println!("Updated {}", db_path.display());

            Ok(count)
        }
    }
}

pub fn run_remove(functions: Option<Vec<String>>) -> Result<(), DbCommandError> {
    let db_path = paths::database_path();

    if !db_path.exists() {
        return Err(DbCommandError::NotFound(db_path.display().to_string()));
    }

    match functions {
        Some(fn_list) => {
            let mut db = ArborDatabase::load(&db_path)?;
            for function_id in &fn_list {
                if db.functions.remove(function_id).is_some() {
                    println!("Removed: {}", function_id);
                } else {
                    eprintln!("Warning: {} not found in database", function_id);
                }
            }
            db.save(&db_path)?;
            Ok(())
        }
        None => {
            let arbor_dir = paths::arbor_dir();
            if arbor_dir.exists() {
                std::fs::remove_dir_all(&arbor_dir)?;
                println!("Removed {}", arbor_dir.display());
            }
            Ok(())
        }
    }
}

pub fn run_export(options: ExportOptions) -> Result<PathBuf, DbCommandError> {
    use crate::output::markdown::{MarkdownOutput, DatabaseStats};

    let db_path = paths::database_path();

    if !db_path.exists() {
        return Err(DbCommandError::NotFound(db_path.display().to_string()));
    }

    let db = ArborDatabase::load(&db_path)?;

    let output_path = options.output_path.unwrap_or_else(|| {
        let ext = if options.format == "json" { "json" } else { "md" };
        PathBuf::from(format!("arbor-export.{}", ext))
    });

    let content = match options.format.as_str() {
        "json" => {
            serde_json::to_string_pretty(&db).map_err(|e| {
                DbCommandError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
            })?
        }
        _ => {
            let mut output = String::new();

            let mut high_risk = 0;
            let mut medium_risk = 0;
            let mut low_risk = 0;
            let mut unique_exceptions = std::collections::HashSet::new();
            let mut unique_none = std::collections::HashSet::new();
            let mut packages = std::collections::HashSet::new();

            for analysis in db.functions.values() {
                match analysis.risk_level() {
                    crate::core::types::RiskLevel::High => high_risk += 1,
                    crate::core::types::RiskLevel::Medium => medium_risk += 1,
                    crate::core::types::RiskLevel::Low => low_risk += 1,
                }
                for r in &analysis.raises {
                    unique_exceptions.insert(r.exception_type.clone());
                }
                for n in &analysis.none_sources {
                    unique_none.insert(n.kind.as_str().to_string());
                }
                if let Some(pkg) = analysis.function_id.split('.').next() {
                    packages.insert(pkg.to_string());
                }
            }

            let stats = DatabaseStats {
                version: db.version.clone(),
                created_at: db.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                updated_at: db.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                function_count: db.functions.len(),
                symbol_count: db.symbol_index.len(),
                unique_exceptions: unique_exceptions.len(),
                unique_none_sources: unique_none.len(),
                package_count: packages.len(),
                group_count: db.grouping_suggestions.len(),
                high_risk,
                medium_risk,
                low_risk,
            };

            output.push_str(&stats.to_markdown());
            output.push_str("\n---\n\n");

            output.push_str("# Function Analyses\n\n");
            for analysis in db.functions.values() {
                output.push_str(&analysis.to_markdown_detailed());
                output.push_str("\n---\n\n");
            }

            if !db.grouping_suggestions.is_empty() {
                output.push_str("# Grouping Suggestions\n\n");
                for suggestion in db.grouping_suggestions.values() {
                    output.push_str(&suggestion.to_markdown_detailed());
                    output.push('\n');
                }
            }

            output
        }
    };

    std::fs::write(&output_path, content)?;

    Ok(output_path)
}

fn detect_environment() -> Result<Environment, DbCommandError> {
    let python_version = detect_python_version()?;
    let venv_path = detect_venv();
    let site_packages = detect_site_packages(&venv_path)?;
    let python_path = detect_python_path();

    Ok(Environment {
        python_version,
        venv_path: venv_path.map(|p| p.display().to_string()),
        site_packages: site_packages.iter().map(|p| p.display().to_string()).collect(),
        python_path: python_path.iter().map(|p| p.display().to_string()).collect(),
    })
}

fn detect_python_version() -> Result<String, DbCommandError> {
    let output = Command::new("python3")
        .args(["--version"])
        .output()
        .or_else(|_| Command::new("python").args(["--version"]).output())
        .map_err(|e| DbCommandError::EnvironmentDetection(e.to_string()))?;

    let version = String::from_utf8_lossy(&output.stdout);
    let version = version.trim().replace("Python ", "");

    if version.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let version = stderr.trim().replace("Python ", "");
        if version.is_empty() {
            return Err(DbCommandError::EnvironmentDetection(
                "Could not detect Python version".to_string(),
            ));
        }
        return Ok(version);
    }

    Ok(version)
}

fn detect_venv() -> Option<PathBuf> {
    if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
        return Some(PathBuf::from(venv));
    }

    let cwd = std::env::current_dir().ok()?;
    for name in &[".venv", "venv", ".env", "env"] {
        let path = cwd.join(name);
        if path.exists() && path.join("bin/python").exists() {
            return Some(path);
        }
    }

    None
}

fn detect_site_packages(venv: &Option<PathBuf>) -> Result<Vec<PathBuf>, DbCommandError> {
    let mut packages = Vec::new();

    if let Some(venv_path) = venv {
        if let Ok(sp) = PythonResolver::find_site_packages(venv_path) {
            packages.push(sp);
        }
    }

    Ok(packages)
}

fn detect_python_path() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.clone());

        for subdir in &["src", "lib", "app"] {
            let path = cwd.join(subdir);
            if path.exists() {
                paths.push(path);
            }
        }
    }

    if let Ok(pythonpath) = std::env::var("PYTHONPATH") {
        for p in pythonpath.split(':') {
            let path = PathBuf::from(p);
            if path.exists() && !paths.contains(&path) {
                paths.push(path);
            }
        }
    }

    paths
}

fn default_command_content() -> &'static str {
    include_str!("../assets/arbor_command.md")
}
