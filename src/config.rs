use anyhow::Result;
use camino::Utf8PathBuf;
use directories::ProjectDirs;
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
        let config = load_config(dir.path()).unwrap();
        assert_eq!(config.php_constraint.as_deref(), Some(">=8.1"));
        assert_eq!(config.profile.as_deref(), Some("wordpress"));
    }

    #[test]
    fn load_config_with_no_files_returns_default() {
        let dir = TempDir::new().unwrap();
        let config = load_config(dir.path()).unwrap();
        assert!(config.php_constraint.is_none());
        assert!(config.profile.is_none());
        assert!(config.matrix.is_none());
        assert!(config.manifest_url.is_none());
    }

    #[test]
    fn load_config_with_invalid_local_returns_error() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, ".phpvm.toml", "php_constraint = [bogus");
        let result = load_config(dir.path());
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
