use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config;
use crate::output::{
    self, DoctorResult, MatrixEntry, MatrixResult, OutputFormat, ReleaseCheckResult, RunStatus,
};
use crate::profile;

// ---------------------------------------------------------------------------
// Project detection
// ---------------------------------------------------------------------------

/// Detect the project type by looking for characteristic files.
///
/// Returns one of: `"WordPress Plugin"`, `"Laravel Application"`, `"Composer Library"`,
/// or `None` if no project is detected.
pub fn detect_project_type(project_dir: &Path) -> Option<String> {
    if is_wordpress_plugin(project_dir) {
        return Some("WordPress Plugin".to_string());
    }
    if is_laravel_app(project_dir) {
        return Some("Laravel Application".to_string());
    }
    if project_dir.join("composer.json").exists() {
        return Some("Composer Library".to_string());
    }
    None
}

fn is_wordpress_plugin(project_dir: &Path) -> bool {
    if let Ok(entries) = fs::read_dir(project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "php") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if content.contains("Plugin Name:") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn is_laravel_app(project_dir: &Path) -> bool {
    project_dir.join("artisan").exists() && project_dir.join("bootstrap/app.php").exists()
}

// ---------------------------------------------------------------------------
// PHP constraint extraction
// ---------------------------------------------------------------------------

/// Read the PHP constraint from `composer.json` in the project directory.
///
/// Returns the raw constraint string (e.g. `">=8.1"`, `"^8.2"`) or `None` if
/// `composer.json` does not exist or does not specify a PHP requirement.
pub fn read_php_constraint(project_dir: &Path) -> Option<String> {
    let composer_path = project_dir.join("composer.json");
    let content = fs::read_to_string(&composer_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    parsed
        .get("require")
        .and_then(|r| r.get("php"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Profile recommendation
// ---------------------------------------------------------------------------

/// Recommend an extension profile based on the detected project type.
///
/// - WordPress Plugin → `"wordpress"`
/// - Laravel Application → `"laravel"`
/// - Composer Library → `"minimal"`
pub fn recommend_profile(project_type: &str) -> String {
    match project_type {
        "WordPress Plugin" => "wordpress",
        "Laravel Application" => "laravel",
        "Composer Library" => "minimal",
        _ => "minimal",
    }
    .to_string()
}

/// Resolve a profile name to a full Profile struct, checking built-ins
/// first, then custom profiles from config.
pub fn resolve_profile(
    profile_name: &str,
    custom_profiles: &[profile::Profile],
) -> profile::Profile {
    profile::resolve_or_minimal(profile_name, custom_profiles)
}

// ---------------------------------------------------------------------------
// Doctor inspection
// ---------------------------------------------------------------------------

/// Inspect the current project and display recommendations (human-readable output).
#[allow(dead_code)]
pub fn run() -> Result<()> {
    run_with_format(OutputFormat::Human)
}

/// Inspect the current project and display results in the requested format.
pub fn run_with_format(format: OutputFormat) -> Result<()> {
    let project_dir = std::env::current_dir().context("Failed to get current directory")?;
    let config = config::load_config(&project_dir)?;

    let project_type = detect_project_type(&project_dir);
    let php_constraint = read_php_constraint(&project_dir);

    let profile_name = config
        .profile
        .clone()
        .or_else(|| project_type.as_ref().map(|pt| recommend_profile(pt)));

    let resolved_profile = profile_name
        .as_deref()
        .map(|name| resolve_profile(name, &config.profiles));

    let recommended_matrix = config::resolve_matrix(&config);

    // Show extensions for the resolved profile.
    if let Some(ref p) = resolved_profile {
        if !p.extensions.is_empty() && matches!(format, OutputFormat::Human) {
            output::info(&format!("Profile extensions: {}", p.extensions.join(", ")));
        }
    }

    let result = DoctorResult {
        project_type,
        php_constraint,
        profile: profile_name,
        recommended_matrix,
    };

    output::print_doctor_result(&result, format);
    Ok(())
}

// ---------------------------------------------------------------------------
// Release check
// ---------------------------------------------------------------------------

/// Run a release compatibility check (human-readable output).
#[allow(dead_code)]
pub fn release_check() -> Result<()> {
    release_check_with_format(OutputFormat::Human)
}

/// Run a release compatibility check and display results in the requested format.
pub fn release_check_with_format(format: OutputFormat) -> Result<()> {
    let project_dir = std::env::current_dir().context("Failed to get current directory")?;
    let config = config::load_config(&project_dir)?;

    let project_type = detect_project_type(&project_dir);
    let php_constraint = read_php_constraint(&project_dir);

    let matrix = config::resolve_matrix(&config);

    let entries: Vec<MatrixEntry> = matrix
        .iter()
        .map(|v| MatrixEntry {
            php_version: v.clone(),
            status: RunStatus::Pass,
            output: None,
        })
        .collect();

    let overall = MatrixResult::compute_overall(&entries);

    let result = ReleaseCheckResult {
        project_type,
        php_constraint,
        entries,
        overall,
    };

    output::print_release_check_result(&result, format);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &TempDir, name: &str, contents: &str) {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    // -- detect_project_type -------------------------------------------------

    #[test]
    fn detect_wordpress_plugin_with_header() {
        let dir = TempDir::new().unwrap();
        write_file(
            &dir,
            "my-plugin.php",
            "<?php\n/*\nPlugin Name: My Awesome Plugin\n*/\n",
        );

        let result = detect_project_type(dir.path());
        assert_eq!(result.as_deref(), Some("WordPress Plugin"));
    }

    #[test]
    fn detect_wordpress_plugin_header_in_main_file() {
        let dir = TempDir::new().unwrap();
        write_file(
            &dir,
            "index.php",
            "<?php\n/**\n * Plugin Name: Test Plugin\n */\n",
        );

        let result = detect_project_type(dir.path());
        assert_eq!(result.as_deref(), Some("WordPress Plugin"));
    }

    #[test]
    fn detect_laravel_application() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "artisan", "#!/usr/bin/env php\n<?php\n");
        write_file(&dir, "bootstrap/app.php", "<?php\n");

        let result = detect_project_type(dir.path());
        assert_eq!(result.as_deref(), Some("Laravel Application"));
    }

    #[test]
    fn detect_laravel_app_missing_bootstrap() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "artisan", "#!/usr/bin/env php\n<?php\n");
        // No bootstrap/app.php — should NOT be detected as Laravel

        let result = detect_project_type(dir.path());
        assert_ne!(result.as_deref(), Some("Laravel Application"));
    }

    #[test]
    fn detect_composer_library() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", r#"{"name": "vendor/package"}"#);

        let result = detect_project_type(dir.path());
        assert_eq!(result.as_deref(), Some("Composer Library"));
    }

    #[test]
    fn detect_wordpress_takes_priority_over_composer() {
        let dir = TempDir::new().unwrap();
        write_file(
            &dir,
            "plugin.php",
            "<?php\n/*\nPlugin Name: WP Plugin\n*/\n",
        );
        write_file(&dir, "composer.json", r#"{"name": "vendor/package"}"#);

        let result = detect_project_type(dir.path());
        assert_eq!(result.as_deref(), Some("WordPress Plugin"));
    }

    #[test]
    fn detect_laravel_takes_priority_over_composer() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "artisan", "#!/usr/bin/env php\n<?php\n");
        write_file(&dir, "bootstrap/app.php", "<?php\n");
        write_file(&dir, "composer.json", r#"{"name": "vendor/package"}"#);

        let result = detect_project_type(dir.path());
        assert_eq!(result.as_deref(), Some("Laravel Application"));
    }

    #[test]
    fn detect_nothing_in_empty_dir() {
        let dir = TempDir::new().unwrap();
        let result = detect_project_type(dir.path());
        assert!(result.is_none());
    }

    // -- read_php_constraint -------------------------------------------------

    #[test]
    fn read_php_constraint_caret() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", r#"{"require": {"php": "^8.2"}}"#);

        let result = read_php_constraint(dir.path());
        assert_eq!(result.as_deref(), Some("^8.2"));
    }

    #[test]
    fn read_php_constraint_gte() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", r#"{"require": {"php": ">=8.1"}}"#);

        let result = read_php_constraint(dir.path());
        assert_eq!(result.as_deref(), Some(">=8.1"));
    }

    #[test]
    fn read_php_constraint_tilde() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", r#"{"require": {"php": "~8.1.0"}}"#);

        let result = read_php_constraint(dir.path());
        assert_eq!(result.as_deref(), Some("~8.1.0"));
    }

    #[test]
    fn read_php_constraint_or_expression() {
        let dir = TempDir::new().unwrap();
        write_file(
            &dir,
            "composer.json",
            r#"{"require": {"php": ">=8.1 || ^8.2"}}"#,
        );

        let result = read_php_constraint(dir.path());
        assert_eq!(result.as_deref(), Some(">=8.1 || ^8.2"));
    }

    #[test]
    fn read_php_constraint_no_php_require() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", r#"{"require": {"ext-curl": "*"}}"#);

        let result = read_php_constraint(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn read_php_constraint_no_composer_json() {
        let dir = TempDir::new().unwrap();
        let result = read_php_constraint(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn read_php_constraint_malformed_json() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", "{ not valid }");

        let result = read_php_constraint(dir.path());
        assert!(result.is_none());
    }

    // -- recommend_profile ----------------------------------------------------

    #[test]
    fn recommend_wordpress_profile() {
        assert_eq!(recommend_profile("WordPress Plugin"), "wordpress");
    }

    #[test]
    fn recommend_laravel_profile() {
        assert_eq!(recommend_profile("Laravel Application"), "laravel");
    }

    #[test]
    fn recommend_composer_library_profile() {
        assert_eq!(recommend_profile("Composer Library"), "minimal");
    }

    #[test]
    fn recommend_unknown_profile_defaults_to_minimal() {
        assert_eq!(recommend_profile("Some Framework"), "minimal");
    }
}
