use super::paths;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Config file not found at {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub path: PathBuf,
    pub auto_save: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: paths::database_path(),
            auto_save: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisConfig {
    pub max_depth: usize,
    pub include_stdlib: bool,
    pub timeout_seconds: u64,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            max_depth: 50,
            include_stdlib: false,
            timeout_seconds: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct EnvironmentConfig {
    pub python_path: Vec<PathBuf>,
    pub venv_path: Option<PathBuf>,
    pub site_packages: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IgnoreConfig {
    pub packages: Vec<String>,
    pub functions: Vec<String>,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ArborConfig {
    pub database: DatabaseConfig,
    pub analysis: AnalysisConfig,
    pub environment: EnvironmentConfig,
    pub ignore: IgnoreConfig,
}

impl ArborConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::NotFound(path.display().to_string()));
        }

        let content = std::fs::read_to_string(path)?;
        let config: ArborConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_or_default() -> Self {
        match Self::find_config() {
            Some(path) => Self::load(&path).unwrap_or_default(),
            None => Self::default(),
        }
    }

    pub fn find_config() -> Option<PathBuf> {
        let config_path = paths::config_path();
        if config_path.exists() {
            return Some(config_path);
        }
        None
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self).map_err(|e| {
            ConfigError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            ))
        })?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn database_path(&self) -> PathBuf {
        self.database.path.clone()
    }

    pub fn should_ignore_package(&self, package: &str) -> bool {
        self.ignore.packages.iter().any(|p| {
            if p.contains('*') {
                glob_match(p, package)
            } else {
                p == package
            }
        })
    }

    pub fn should_ignore_function(&self, function: &str) -> bool {
        self.ignore.functions.iter().any(|f| {
            if f.contains('*') {
                glob_match(f, function)
            } else {
                f == function
            }
        })
    }

    pub fn default_toml() -> String {
        format!(
            r#"# Arbor Configuration

[database]
path = "{}/{}"
auto_save = true

[analysis]
max_depth = 50
include_stdlib = false
timeout_seconds = 300

[environment]
python_path = ["."]
# venv_path = ".venv"

[ignore]
packages = ["tests", "__pycache__", ".git"]
functions = []
"#,
            paths::ARBOR_DIR,
            paths::DATABASE_FILE
        )
    }
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('*').collect();

    if pattern_parts.len() == 1 {
        return pattern == text;
    }

    let mut pos = 0;

    for (i, part) in pattern_parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }

        if i == 0 {
            if !text.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == pattern_parts.len() - 1 {
            if !text.ends_with(part) {
                return false;
            }
        } else {
            match text[pos..].find(part) {
                Some(idx) => pos += idx + part.len(),
                None => return false,
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ArborConfig::default();
        assert_eq!(config.analysis.max_depth, 50);
        assert!(!config.analysis.include_stdlib);
        assert_eq!(config.analysis.timeout_seconds, 300);
        assert_eq!(config.database.path, paths::database_path());
    }

    #[test]
    fn test_parse_config() {
        let toml_str = r#"
[database]
path = "custom.json"
auto_save = false

[analysis]
max_depth = 100
include_stdlib = true
timeout_seconds = 600

[environment]
python_path = ["src", "lib"]
venv_path = ".venv"

[ignore]
packages = ["tests", "docs"]
functions = ["*._private_*"]
"#;

        let config: ArborConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.database.path, PathBuf::from("custom.json"));
        assert!(!config.database.auto_save);
        assert_eq!(config.analysis.max_depth, 100);
        assert!(config.analysis.include_stdlib);
        assert_eq!(config.environment.python_path.len(), 2);
        assert_eq!(
            config.environment.venv_path,
            Some(PathBuf::from(".venv"))
        );
        assert_eq!(config.ignore.packages.len(), 2);
        assert_eq!(config.ignore.functions.len(), 1);
    }

    #[test]
    fn test_partial_config() {
        let toml_str = r#"
[analysis]
max_depth = 25
"#;

        let config: ArborConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.analysis.max_depth, 25);
        assert!(!config.analysis.include_stdlib);
        assert_eq!(config.database.path, paths::database_path());
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*._private_*", "module._private_func"));
        assert!(glob_match("*._private_*", "pkg.module._private_helper"));
        assert!(!glob_match("*._private_*", "module.public_func"));

        assert!(glob_match("test_*", "test_module"));
        assert!(!glob_match("test_*", "module_test"));

        assert!(glob_match("*_test", "module_test"));
        assert!(!glob_match("*_test", "test_module"));

        assert!(glob_match("foo", "foo"));
        assert!(!glob_match("foo", "bar"));
    }

    #[test]
    fn test_should_ignore() {
        let config: ArborConfig = toml::from_str(
            r#"
[ignore]
packages = ["tests", "__pycache__"]
functions = ["*._private_*", "test_*"]
"#,
        )
        .unwrap();

        assert!(config.should_ignore_package("tests"));
        assert!(config.should_ignore_package("__pycache__"));
        assert!(!config.should_ignore_package("mypackage"));

        assert!(config.should_ignore_function("module._private_func"));
        assert!(config.should_ignore_function("test_something"));
        assert!(!config.should_ignore_function("public_func"));
    }

    #[test]
    fn test_default_toml_parses() {
        let toml_str = ArborConfig::default_toml();
        let config: Result<ArborConfig, _> = toml::from_str(&toml_str);
        assert!(config.is_ok());
    }
}
