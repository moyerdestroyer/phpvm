use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::profile::Profile;

/// PHPVM configuration, loaded from ~/.phpvm/config.toml or project-local .phpvm.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Default PHP version constraint (e.g. ">=8.1")
    pub php_constraint: Option<String>,

    /// Default profile (wordpress, laravel, minimal, or a custom profile name)
    pub profile: Option<String>,

    /// Matrix of PHP versions to test against
    pub matrix: Option<Vec<String>>,

    /// Remote manifest URL
    pub manifest_url: Option<String>,

    /// Custom extension profiles defined by the user
    #[serde(default)]
    pub profiles: Vec<Profile>,

    /// The version activated by `phpvm use` (persists the active runtime
    /// across shell sessions and terminals, similar to fnm defaults).
    #[serde(default)]
    pub current_version: Option<String>,
}

/// Returns the PHPVM data directory.
///
/// Defaults to `~/.phpvm/` (or the platform equivalent of the user's home
/// directory + `/.phpvm`), which matches documented examples and provides
/// reproducibility. Users can override with the `PHPVM_HOME` environment
/// variable (e.g. for containers or custom locations).
pub fn data_dir() -> Result<Utf8PathBuf> {
    if let Ok(custom) = std::env::var("PHPVM_HOME") {
        return Ok(Utf8PathBuf::from(custom));
    }

    let home = BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
        .home_dir()
        .to_path_buf();
    let home_utf8 = Utf8PathBuf::from_path_buf(home)
        .map_err(|p| anyhow::anyhow!("home directory is not valid UTF-8: {:?}", p))?;
    Ok(home_utf8.join(".phpvm"))
}

/// Returns the current project directory (CWD) as a `Utf8PathBuf`.
/// This is the single helper for current_dir + camino conversion + consistent
/// error context (addresses repeated boilerplate and enforces Utf8 paths).
pub fn current_project_dir() -> Result<Utf8PathBuf> {
    let p = std::env::current_dir().context("Failed to get current directory")?;
    Utf8PathBuf::from_path_buf(p)
        .map_err(|p| anyhow::anyhow!("Current directory is not valid UTF-8: {:?}", p))
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

/// Returns the default matrix when no config specifies one.
#[allow(dead_code)]
pub fn default_matrix() -> Vec<String> {
    vec![
        "8.1.latest".to_string(),
        "8.2.latest".to_string(),
        "8.3.latest".to_string(),
        "8.4.latest".to_string(),
    ]
}

/// Returns the matrix from config if specified, otherwise the default matrix.
#[allow(dead_code)]
pub fn resolve_matrix(config: &Config) -> Vec<String> {
    config.matrix.clone().unwrap_or_else(default_matrix)
}

/// Persist the given version as the globally active one (written to the
/// global `~/.phpvm/config.toml` under `current_version`).
///
/// This is what makes `phpvm use X` affect future terminals/sessions.
pub fn set_current_version(version: &str) -> Result<()> {
    let dir = data_dir()?;
    let path = dir.join("config.toml");

    // Load existing global config (or defaults) so we don't clobber other settings.
    let mut config: Config = if path.as_std_path().exists() {
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read global config at {}", path))?;
        toml::from_str(&contents).unwrap_or_default()
    } else {
        Config::default()
    };

    config.current_version = Some(version.to_string());

    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create config directory {}", dir))?;

    let serialized = toml::to_string_pretty(&config)
        .with_context(|| "Failed to serialize updated global config")?;
    std::fs::write(&path, serialized)
        .with_context(|| format!("Failed to write global config to {}", path))?;

    Ok(())
}

/// Return the persisted current version (from `phpvm use`), if any.
/// This is the global one stored in the user's `~/.phpvm/config.toml`.
pub fn get_current_version() -> Option<String> {
    let path = match data_dir() {
        Ok(d) => d.join("config.toml"),
        Err(_) => return None,
    };
    if !path.as_std_path().exists() {
        return None;
    }
    let contents = std::fs::read_to_string(&path).ok()?;
    let cfg: Config = toml::from_str(&contents).ok()?;
    cfg.current_version
}

/// Load config from a project-local .phpvm.toml (or global), falling back to defaults.
/// `project_dir` should be a valid UTF-8 path (use `current_project_dir()`).
pub fn load_config(project_dir: &Utf8Path) -> Result<Config> {
    let local_config = project_dir.join(".phpvm.toml");
    if local_config.as_std_path().exists() {
        let contents = std::fs::read_to_string(local_config.as_std_path())?;
        let config: Config = toml::from_str(&contents)?;
        return Ok(config);
    }

    let global_config = data_dir()?.join("config.toml");
    if global_config.as_std_path().exists() {
        let contents = std::fs::read_to_string(global_config.as_std_path())?;
        let config: Config = toml::from_str(&contents)?;
        return Ok(config);
    }

    Ok(Config::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &TempDir, name: &str, contents: &str) {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn parse_complete_config() {
        let toml = r#"
php_constraint = ">=8.1"
profile = "wordpress"
matrix = ["8.1.latest", "8.2.latest", "8.3.latest"]
manifest_url = "https://example.com/manifest.json"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.php_constraint.as_deref(), Some(">=8.1"));
        assert_eq!(config.profile.as_deref(), Some("wordpress"));
        assert_eq!(
            config.matrix.as_deref(),
            Some(
                &[
                    "8.1.latest".to_string(),
                    "8.2.latest".to_string(),
                    "8.3.latest".to_string()
                ][..]
            )
        );
        assert_eq!(
            config.manifest_url.as_deref(),
            Some("https://example.com/manifest.json")
        );
    }

    #[test]
    fn parse_partial_config() {
        let toml = r#"
php_constraint = "^8.2"
profile = "laravel"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.php_constraint.as_deref(), Some("^8.2"));
        assert_eq!(config.profile.as_deref(), Some("laravel"));
        assert!(config.matrix.is_none());
        assert!(config.manifest_url.is_none());
    }

    #[test]
    fn parse_empty_config() {
        let toml = "";
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.php_constraint.is_none());
        assert!(config.profile.is_none());
        assert!(config.matrix.is_none());
        assert!(config.manifest_url.is_none());
        assert!(config.profiles.is_empty());
    }

    #[test]
    fn config_with_matrix_specified() {
        let toml = r#"
matrix = ["8.0.latest", "8.1.latest"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        let resolved = resolve_matrix(&config);
        assert_eq!(resolved, vec!["8.0.latest", "8.1.latest"]);
    }

    #[test]
    fn config_without_matrix_uses_default() {
        let config = Config::default();
        let resolved = resolve_matrix(&config);
        assert_eq!(
            resolved,
            vec!["8.1.latest", "8.2.latest", "8.3.latest", "8.4.latest",]
        );
    }

    #[test]
    fn invalid_toml_returns_error() {
        let toml = "php_constraint = [invalid";
        let result: Result<Config, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    fn utf8_project_dir(dir: &TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(dir.path().to_path_buf())
            .expect("temporary directory paths are valid UTF-8 in tests")
    }

    #[test]
    fn load_config_with_project_local() {
        let dir = TempDir::new().unwrap();
        write_file(
            &dir,
            ".phpvm.toml",
            r#"
php_constraint = ">=8.1"
profile = "wordpress"
"#,
        );
        let config = load_config(&utf8_project_dir(&dir)).unwrap();
        assert_eq!(config.php_constraint.as_deref(), Some(">=8.1"));
        assert_eq!(config.profile.as_deref(), Some("wordpress"));
    }

    #[test]
    fn load_config_with_no_files_returns_default() {
        let dir = TempDir::new().unwrap();
        let config = load_config(&utf8_project_dir(&dir)).unwrap();
        assert!(config.php_constraint.is_none());
        assert!(config.profile.is_none());
        assert!(config.matrix.is_none());
        assert!(config.manifest_url.is_none());
    }

    #[test]
    fn load_config_with_invalid_local_returns_error() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, ".phpvm.toml", "php_constraint = [bogus");
        let result = load_config(&utf8_project_dir(&dir));
        assert!(result.is_err());
    }

    #[test]
    fn default_matrix_returns_expected_versions() {
        let matrix = default_matrix();
        assert_eq!(matrix.len(), 4);
        assert_eq!(matrix[0], "8.1.latest");
        assert_eq!(matrix[1], "8.2.latest");
        assert_eq!(matrix[2], "8.3.latest");
        assert_eq!(matrix[3], "8.4.latest");
    }

    #[test]
    fn resolve_matrix_with_explicit_matrix() {
        let config = Config {
            matrix: Some(vec!["7.4.latest".to_string(), "8.0.latest".to_string()]),
            ..Default::default()
        };
        let resolved = resolve_matrix(&config);
        assert_eq!(resolved, vec!["7.4.latest", "8.0.latest"]);
    }

    #[test]
    fn resolve_matrix_with_none_uses_default() {
        let config = Config {
            matrix: None,
            ..Default::default()
        };
        let resolved = resolve_matrix(&config);
        assert_eq!(resolved, default_matrix());
    }

    // -- Custom profiles in config --

    #[test]
    fn parse_config_with_custom_profiles() {
        let toml = r#"
profile = "drupal"

[[profiles]]
name = "drupal"
extensions = ["curl", "dom", "gd", "mbstring", "mysqli", "pdo_mysql", "xml", "zip"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.profile.as_deref(), Some("drupal"));
        assert_eq!(config.profiles.len(), 1);
        assert_eq!(config.profiles[0].name, "drupal");
        assert_eq!(config.profiles[0].extensions.len(), 8);
        assert!(config.profiles[0].extensions.contains(&"curl".to_string()));
    }

    #[test]
    fn parse_config_with_multiple_custom_profiles() {
        let toml = r#"
profile = "api"

[[profiles]]
name = "api"
extensions = ["curl", "json", "mbstring", "openssl"]

[[profiles]]
name = "drupal"
extensions = ["curl", "dom", "gd", "mbstring", "mysql", "pdo_mysql", "xml", "zip"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.profiles.len(), 2);
        assert_eq!(config.profiles[0].name, "api");
        assert_eq!(config.profiles[0].extensions.len(), 4);
        assert_eq!(config.profiles[1].name, "drupal");
        assert_eq!(config.profiles[1].extensions.len(), 8);
    }

    #[test]
    fn config_without_profiles_defaults_to_empty() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.profiles.is_empty());
    }
}
