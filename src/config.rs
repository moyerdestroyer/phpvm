use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

/// PHPVM configuration, loaded from ~/.phpvm/config.toml merged with project-local .phpvm.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Default PHP version constraint (e.g. ">=8.1")
    pub php_constraint: Option<String>,

    /// Project-local PHP version pin (e.g. "8.3", "8.3.latest") from `.phpvm.toml`.
    pub version: Option<String>,

    /// Default profile preset name (wordpress, laravel, minimal, or custom)
    pub profile: Option<String>,

    /// Matrix of PHP versions to test against
    pub matrix: Option<Vec<String>>,

    /// Remote manifest URL
    pub manifest_url: Option<String>,

    /// The version activated by `phpvm use` (persists the active runtime
    /// across shell sessions and terminals, similar to fnm defaults).
    #[serde(default)]
    pub current_version: Option<String>,

    /// When true, shell integration (`phpvm env`) emits a directory-change hook
    /// that runs `phpvm use` in projects with `.phpvm-version` / `.phpvm.toml`.
    /// Global-only — set in `~/.phpvm/config.toml`.
    #[serde(default)]
    pub use_on_cd: bool,
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

/// Load global config from `~/.phpvm/config.toml`, or defaults.
pub fn load_global_config() -> Result<Config> {
    let global_config = data_dir()?.join("config.toml");
    if global_config.as_std_path().exists() {
        let contents = std::fs::read_to_string(global_config.as_std_path())
            .with_context(|| format!("Failed to read global config at {}", global_config))?;
        let config: Config = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse global config at {}", global_config))?;
        return Ok(config);
    }
    Ok(Config::default())
}

/// Persist the given version as the globally active one (written to the
/// global `~/.phpvm/config.toml` under `current_version`).
pub fn set_current_version(version: &str) -> Result<()> {
    let dir = data_dir()?;
    let path = dir.join("config.toml");

    let mut config = load_global_config()?;
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
pub fn get_current_version() -> Option<String> {
    load_global_config().ok()?.current_version
}

/// Whether per-project auto-switch on directory change is enabled (global config).
pub fn use_on_cd_enabled() -> bool {
    load_global_config().ok().is_some_and(|cfg| cfg.use_on_cd)
}

/// Clear the persisted active version (used by `phpvm deactivate --persist`).
pub fn clear_current_version() -> Result<()> {
    let dir = data_dir()?;
    let path = dir.join("config.toml");
    let mut cfg = load_global_config()?;
    cfg.current_version = None;

    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create config directory {}", dir))?;

    let serialized =
        toml::to_string_pretty(&cfg).with_context(|| "Failed to serialize global config")?;
    std::fs::write(&path, serialized)
        .with_context(|| format!("Failed to write global config to {}", path))?;
    Ok(())
}

/// Load config: global `~/.phpvm/config.toml` overlaid by project `.phpvm.toml`.
///
/// Project wins on `version`, `profile`, `php_constraint`, `matrix`, and `manifest_url`.
/// `current_version` and `use_on_cd` are global-only.
pub fn load_config(project_dir: &Utf8Path) -> Result<Config> {
    let mut config = load_global_config()?;

    let local_config = project_dir.join(".phpvm.toml");
    if local_config.as_std_path().exists() {
        let contents = std::fs::read_to_string(local_config.as_std_path())
            .with_context(|| format!("Failed to read project config at {}", local_config))?;
        let project: Config = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse project config at {}", local_config))?;
        merge_project_config(&mut config, &project);
    }

    Ok(config)
}

fn merge_project_config(base: &mut Config, project: &Config) {
    if project.php_constraint.is_some() {
        base.php_constraint = project.php_constraint.clone();
    }
    if project.version.is_some() {
        base.version = project.version.clone();
    }
    if project.profile.is_some() {
        base.profile = project.profile.clone();
    }
    if project.matrix.is_some() {
        base.matrix = project.matrix.clone();
    }
    if project.manifest_url.is_some() {
        base.manifest_url = project.manifest_url.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::env_lock::LOCK as ENV_LOCK;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &TempDir, name: &str, contents: &str) {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn use_on_cd_defaults_to_false() {
        let config: Config = toml::from_str("").unwrap();
        assert!(!config.use_on_cd);
    }

    #[test]
    fn use_on_cd_enabled_reads_global_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        let prev = std::env::var("PHPVM_HOME").ok();
        std::env::set_var("PHPVM_HOME", dir.path());
        write_file(&dir, "config.toml", "use_on_cd = true\n");

        assert!(use_on_cd_enabled());

        match prev {
            Some(v) => std::env::set_var("PHPVM_HOME", v),
            None => std::env::remove_var("PHPVM_HOME"),
        }
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

    #[test]
    fn load_config_merges_project_over_global() {
        let _guard = ENV_LOCK.lock().unwrap();
        let home = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();

        let phpvm_home = home.path().join(".phpvm");
        std::fs::create_dir_all(&phpvm_home).unwrap();
        write_file(
            &home,
            ".phpvm/config.toml",
            r#"
profile = "minimal"
php_constraint = ">=8.0"
matrix = ["8.0.latest"]
"#,
        );

        write_file(
            &project,
            ".phpvm.toml",
            r#"
profile = "wordpress"
"#,
        );

        let prev = std::env::var("PHPVM_HOME").ok();
        std::env::set_var("PHPVM_HOME", phpvm_home.to_string_lossy().to_string());

        let config = load_config(&utf8_project_dir(&project)).unwrap();
        assert_eq!(config.profile.as_deref(), Some("wordpress"));
        assert_eq!(config.php_constraint.as_deref(), Some(">=8.0"));
        assert_eq!(
            config.matrix.as_deref(),
            Some(&["8.0.latest".to_string()][..])
        );

        match prev {
            Some(v) => std::env::set_var("PHPVM_HOME", v),
            None => std::env::remove_var("PHPVM_HOME"),
        }
    }
}
