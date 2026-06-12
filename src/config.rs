use anyhow::Result;
use camino::Utf8PathBuf;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// PHPVM configuration, loaded from ~/.phpvm/config.toml or project-local .phpvm.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Default PHP version constraint (e.g. ">=8.1")
    pub php_constraint: Option<String>,

    /// Default profile (wordpress, laravel, minimal)
    pub profile: Option<String>,

    /// Matrix of PHP versions to test against
    pub matrix: Option<Vec<String>>,

    /// Remote manifest URL
    pub manifest_url: Option<String>,
}

/// Returns the PHPVM data directory (e.g. ~/.phpvm/)
pub fn data_dir() -> Result<Utf8PathBuf> {
    let dirs = ProjectDirs::from("com", "phpvm", "phpvm")
        .ok_or_else(|| anyhow::anyhow!("Could not determine PHPVM data directory"))?;
    let path = Utf8PathBuf::from(dirs.data_dir().to_string_lossy().as_ref());
    Ok(path)
}

/// Returns the runtimes directory (e.g. ~/.phpvm/runtimes/)
pub fn runtimes_dir() -> Result<Utf8PathBuf> {
    Ok(data_dir()?.join("runtimes"))
}

/// Returns the cache directory (e.g. ~/.phpvm/cache/)
#[allow(dead_code)]
pub fn cache_dir() -> Result<Utf8PathBuf> {
    Ok(data_dir()?.join("cache"))
}

/// Load config from a project-local .phpvm.toml, falling back to defaults.
pub fn load_config(project_dir: &std::path::Path) -> Result<Config> {
    let local_config = project_dir.join(".phpvm.toml");
    if local_config.exists() {
        let contents = std::fs::read_to_string(&local_config)?;
        let config: Config = toml::from_str(&contents)?;
        return Ok(config);
    }

    let global_config = data_dir()?.join("config.toml");
    if global_config.exists() {
        let contents = std::fs::read_to_string(&global_config)?;
        let config: Config = toml::from_str(&contents)?;
        return Ok(config);
    }

    Ok(Config::default())
}
