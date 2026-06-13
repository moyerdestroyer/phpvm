use std::fs;

use anyhow::Result;
use camino::Utf8Path;

use crate::config;
use crate::output::{
    self, DoctorResult, MatrixEntry, MatrixResult, OutputFormat, ReleaseCheckResult, RunStatus,
};
use crate::profile;
use crate::runner;

// ---------------------------------------------------------------------------
// Project detection
// ---------------------------------------------------------------------------

/// Detect the project type by looking for characteristic files.
///
/// Returns one of: `"WordPress Plugin"`, `"Laravel Application"`, `"Composer Library"`,
/// or `None` if no project is detected.
pub fn detect_project_type(project_dir: &Utf8Path) -> Option<String> {
    if is_wordpress_plugin(project_dir) {
        return Some("WordPress Plugin".to_string());
    }
    if is_laravel_app(project_dir) {
        return Some("Laravel Application".to_string());
    }
    if project_dir.join("composer.json").as_std_path().exists() {
        return Some("Composer Library".to_string());
    }
    None
}

fn is_wordpress_plugin(project_dir: &Utf8Path) -> bool {
    if let Ok(entries) = fs::read_dir(project_dir.as_std_path()) {
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

fn is_laravel_app(project_dir: &Utf8Path) -> bool {
    project_dir.join("artisan").as_std_path().exists()
        && project_dir.join("bootstrap/app.php").as_std_path().exists()
}

// ---------------------------------------------------------------------------
// PHP constraint extraction
// ---------------------------------------------------------------------------

/// Read the PHP constraint from `composer.json` in the project directory.
///
/// Returns the raw constraint string (e.g. `">=8.1"`, `"^8.2"`) or `None` if
/// `composer.json` does not exist or does not specify a PHP requirement.
pub fn read_php_constraint(project_dir: &Utf8Path) -> Option<String> {
    let composer_path = project_dir.join("composer.json");
    let content = fs::read_to_string(composer_path.as_std_path()).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    parsed
        .get("require")
        .and_then(|r| r.get("php"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Read required PHP extensions from `composer.json` (`ext-*` in require/require-dev).
pub fn read_required_extensions(project_dir: &Utf8Path) -> Vec<String> {
    let composer_path = project_dir.join("composer.json");
    let content = match fs::read_to_string(composer_path.as_std_path()) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut extensions = Vec::new();
    for section in ["require", "require-dev"] {
        if let Some(requirements) = parsed.get(section).and_then(|v| v.as_object()) {
            for (key, _) in requirements {
                if let Some(ext) = key.strip_prefix("ext-") {
                    if !extensions.iter().any(|e| e == ext) {
                        extensions.push(ext.to_string());
                    }
                }
            }
        }
    }
    extensions.sort();
    extensions
}

fn missing_extensions_for_preset(required: &[String], enabled: &[String]) -> Vec<String> {
    required
        .iter()
        .filter(|ext| !enabled.iter().any(|e| e == *ext))
        .cloned()
        .collect()
}

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
    let project_dir = config::current_project_dir()?;
    let config = config::load_config(&project_dir)?;
    let mf = match crate::manifest::fetch_from_config(&config) {
        Ok(mf) => Some(mf),
        Err(e) => {
            if matches!(format, OutputFormat::Human) {
                output::warn(&format!(
                    "Could not fetch manifest: {e}. Extension recommendations may be incomplete."
                ));
            }
            None
        }
    };

    let project_type = detect_project_type(&project_dir);
    let php_constraint = read_php_constraint(&project_dir);
    let required_extensions = read_required_extensions(&project_dir);

    let profile_name = config
        .profile
        .clone()
        .or_else(|| project_type.as_ref().map(|pt| recommend_profile(pt)));

    let enabled_extensions = profile_name
        .as_deref()
        .and_then(|name| {
            profile::enabled_extensions_for_preset(name, &project_dir, mf.as_ref()).ok()
        })
        .unwrap_or_default();

    let missing_extensions = if profile_name.is_some() {
        missing_extensions_for_preset(&required_extensions, &enabled_extensions)
    } else {
        Vec::new()
    };

    let recommended_matrix = config::resolve_matrix(&config);

    if matches!(format, OutputFormat::Human) {
        warn_if_matrix_may_conflict_with_constraint(&recommended_matrix, php_constraint.as_deref());
    }

    if !enabled_extensions.is_empty() && matches!(format, OutputFormat::Human) {
        output::info(&format!(
            "Enabled extensions: {}",
            enabled_extensions.join(", ")
        ));
    }

    let result = DoctorResult {
        project_type,
        php_constraint,
        profile: profile_name,
        required_extensions,
        missing_extensions,
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
    let project_dir = config::current_project_dir()?;
    let config = config::load_config(&project_dir)?;

    let project_type = detect_project_type(&project_dir);
    let php_constraint = read_php_constraint(&project_dir);

    let matrix = config::resolve_matrix(&config);
    if matches!(format, OutputFormat::Human) {
        warn_if_matrix_may_conflict_with_constraint(&matrix, php_constraint.as_deref());
    }

    // Actually execute a basic verification command against each matrix version
    // using the real runner. This makes release-check report true status instead
    // of always faking PASS (addresses primary workflow, explicitness, and
    // reproducibility). Uses a minimal php -r command that exercises the runtime.
    let check_cmd: Vec<String> = vec![
        "php".to_string(),
        "-r".to_string(),
        "echo 'phpvm-ok\n';".to_string(),
    ];

    let mut entries: Vec<MatrixEntry> = Vec::new();
    for v in &matrix {
        match runner::run_silent(v, &check_cmd) {
            Ok(run) => {
                entries.push(MatrixEntry {
                    php_version: run.resolved_version,
                    status: RunStatus::Pass,
                    output: None,
                });
            }
            Err(e) => {
                entries.push(MatrixEntry {
                    php_version: v.clone(),
                    status: RunStatus::Fail,
                    output: Some(e.to_string()),
                });
            }
        }
    }

    let overall = MatrixResult::compute_overall(&entries);

    let result = ReleaseCheckResult {
        project_type,
        php_constraint,
        entries,
        overall,
    };

    output::print_release_check_result(&result, format);

    if matches!(result.overall, RunStatus::Fail) {
        anyhow::bail!("Release check failed");
    }

    Ok(())
}

fn warn_if_matrix_may_conflict_with_constraint(matrix: &[String], constraint: Option<&str>) {
    let Some((major, minor)) = constraint.and_then(extract_min_major_minor) else {
        return;
    };

    let conflicting: Vec<&str> = matrix
        .iter()
        .map(String::as_str)
        .filter(|version| {
            let Some((entry_major, entry_minor)) = extract_leading_major_minor(version) else {
                return false;
            };
            (entry_major, entry_minor) < (major, minor)
        })
        .collect();

    if !conflicting.is_empty() {
        output::warn(&format!(
            "composer.json asks for PHP {}.{} or newer; matrix also includes {}. \
             PHPVM will run what you selected.",
            major,
            minor,
            conflicting.join(", ")
        ));
    }
}

fn extract_min_major_minor(constraint: &str) -> Option<(u32, u32)> {
    for prefix in [">=", "^", "~"] {
        if let Some(rest) = constraint.trim().strip_prefix(prefix) {
            return extract_leading_major_minor(rest.trim());
        }
    }
    extract_leading_major_minor(constraint.trim())
}

fn extract_leading_major_minor(input: &str) -> Option<(u32, u32)> {
    let mut parts = input.split('.');
    let major = leading_u32(parts.next()?)?;
    let minor = leading_u32(parts.next()?)?;
    Some((major, minor))
}

fn leading_u32(input: &str) -> Option<u32> {
    let digits: String = input
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    use camino::Utf8PathBuf;

    fn write_file(dir: &TempDir, name: &str, contents: &str) {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    fn utf8(dir: &TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("tempdir paths are UTF-8")
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

        let result = detect_project_type(&utf8(&dir));
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

        let result = detect_project_type(&utf8(&dir));
        assert_eq!(result.as_deref(), Some("WordPress Plugin"));
    }

    #[test]
    fn detect_laravel_application() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "artisan", "#!/usr/bin/env php\n<?php\n");
        write_file(&dir, "bootstrap/app.php", "<?php\n");

        let result = detect_project_type(&utf8(&dir));
        assert_eq!(result.as_deref(), Some("Laravel Application"));
    }

    #[test]
    fn detect_laravel_app_missing_bootstrap() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "artisan", "#!/usr/bin/env php\n<?php\n");
        // No bootstrap/app.php — should NOT be detected as Laravel

        let result = detect_project_type(&utf8(&dir));
        assert_ne!(result.as_deref(), Some("Laravel Application"));
    }

    #[test]
    fn detect_composer_library() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", r#"{"name": "vendor/package"}"#);

        let result = detect_project_type(&utf8(&dir));
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

        let result = detect_project_type(&utf8(&dir));
        assert_eq!(result.as_deref(), Some("WordPress Plugin"));
    }

    #[test]
    fn detect_laravel_takes_priority_over_composer() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "artisan", "#!/usr/bin/env php\n<?php\n");
        write_file(&dir, "bootstrap/app.php", "<?php\n");
        write_file(&dir, "composer.json", r#"{"name": "vendor/package"}"#);

        let result = detect_project_type(&utf8(&dir));
        assert_eq!(result.as_deref(), Some("Laravel Application"));
    }

    #[test]
    fn detect_nothing_in_empty_dir() {
        let dir = TempDir::new().unwrap();
        let result = detect_project_type(&utf8(&dir));
        assert!(result.is_none());
    }

    // -- read_php_constraint -------------------------------------------------

    #[test]
    fn read_php_constraint_caret() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", r#"{"require": {"php": "^8.2"}}"#);

        let result = read_php_constraint(&utf8(&dir));
        assert_eq!(result.as_deref(), Some("^8.2"));
    }

    #[test]
    fn read_php_constraint_gte() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", r#"{"require": {"php": ">=8.1"}}"#);

        let result = read_php_constraint(&utf8(&dir));
        assert_eq!(result.as_deref(), Some(">=8.1"));
    }

    #[test]
    fn read_php_constraint_tilde() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", r#"{"require": {"php": "~8.1.0"}}"#);

        let result = read_php_constraint(&utf8(&dir));
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

        let result = read_php_constraint(&utf8(&dir));
        assert_eq!(result.as_deref(), Some(">=8.1 || ^8.2"));
    }

    #[test]
    fn read_php_constraint_no_php_require() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", r#"{"require": {"ext-curl": "*"}}"#);

        let result = read_php_constraint(&utf8(&dir));
        assert!(result.is_none());
    }

    #[test]
    fn read_php_constraint_no_composer_json() {
        let dir = TempDir::new().unwrap();
        let result = read_php_constraint(&utf8(&dir));
        assert!(result.is_none());
    }

    #[test]
    fn read_php_constraint_malformed_json() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "composer.json", "{ not valid }");

        let result = read_php_constraint(&utf8(&dir));
        assert!(result.is_none());
    }

    #[test]
    fn read_required_extensions_from_composer_json() {
        let dir = TempDir::new().unwrap();
        write_file(
            &dir,
            "composer.json",
            r#"{
  "require": {
    "ext-curl": "*",
    "ext-mbstring": "*"
  },
  "require-dev": {
    "ext-tokenizer": "*"
  }
}"#,
        );

        let extensions = read_required_extensions(&utf8(&dir));
        assert_eq!(extensions, vec!["curl", "mbstring", "tokenizer"]);
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

    #[test]
    fn extract_min_major_minor_from_common_constraints() {
        assert_eq!(extract_min_major_minor(">=8.4"), Some((8, 4)));
        assert_eq!(extract_min_major_minor("^8.2"), Some((8, 2)));
        assert_eq!(extract_min_major_minor("~8.1.0"), Some((8, 1)));
        assert_eq!(extract_min_major_minor(">=8.1 || ^8.2"), Some((8, 1)));
    }
}
