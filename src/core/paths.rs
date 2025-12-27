use std::path::PathBuf;

pub const ARBOR_DIR: &str = ".arbor";
pub const DATABASE_FILE: &str = "database.json";
pub const CONFIG_FILE: &str = "config.toml";
pub const COMMANDS_DIR: &str = "commands";

pub fn arbor_dir() -> PathBuf {
    PathBuf::from(ARBOR_DIR)
}

pub fn database_path() -> PathBuf {
    arbor_dir().join(DATABASE_FILE)
}

pub fn config_path() -> PathBuf {
    arbor_dir().join(CONFIG_FILE)
}

pub fn commands_dir() -> PathBuf {
    arbor_dir().join(COMMANDS_DIR)
}

pub fn ensure_arbor_dir() -> std::io::Result<PathBuf> {
    let dir = arbor_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    let commands = commands_dir();
    if !commands.exists() {
        std::fs::create_dir_all(&commands)?;
    }
    Ok(dir)
}

pub fn arbor_dir_exists() -> bool {
    arbor_dir().exists()
}
