use anyhow::{Context, Result};

use crate::config;
use crate::manifest::{self, Manifest};
use crate::output::{self, OutputFormat};
use crate::profile_preset::{self, ListedPreset};
use crate::providers;
use crate::runtime_metadata::RuntimeMetadata;

// ---------------------------------------------------------------------------
// ProfileTemplate — manifest starter seeding only (not user config)
// ---------------------------------------------------------------------------

/// Extension list from the manifest used to seed a starter ini when no bundled file exists.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ProfileTemplate {
    pub name: String,
    pub extensions: Vec<String>,
}

/// Return the built-in wordpress template (offline manifest fallback).
pub fn wordpress_template() -> ProfileTemplate {
    ProfileTemplate {
        name: "wordpress".to_string(),
        extensions: vec![
            "curl".to_string(),
            "dom".to_string(),
            "gd".to_string(),
            "intl".to_string(),
            "mbstring".to_string(),
            "mysqli".to_string(),
            "openssl".to_string(),
            "pdo_mysql".to_string(),
            "xml".to_string(),
            "zip".to_string(),
        ],
    }
}

/// Return the built-in laravel template.
pub fn laravel_template() -> ProfileTemplate {
    ProfileTemplate {
        name: "laravel".to_string(),
        extensions: vec![
            "curl".to_string(),
            "intl".to_string(),
            "mbstring".to_string(),
            "openssl".to_string(),
            "pdo_mysql".to_string(),
            "tokenizer".to_string(),
            "xml".to_string(),
            "zip".to_string(),
        ],
    }
}

/// Return the built-in minimal template.
pub fn minimal_template() -> ProfileTemplate {
    ProfileTemplate {
        name: "minimal".to_string(),
        extensions: vec![],
    }
}

/// Return all built-in manifest templates.
pub fn builtin_templates() -> Vec<ProfileTemplate> {
    vec![wordpress_template(), laravel_template(), minimal_template()]
}

/// Look up a built-in template by name.
pub fn builtin_template(name: &str) -> Option<ProfileTemplate> {
    match name {
        "wordpress" => Some(wordpress_template()),
        "laravel" => Some(laravel_template()),
        "minimal" => Some(minimal_template()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Profile switching
// ---------------------------------------------------------------------------

/// Switch the active ini preset for an installed runtime.
pub fn use_profile(profile_name: &str, version_spec: Option<&str>) -> Result<()> {
    let project_dir = config::current_project_dir()?;
    let cfg = config::load_config(&project_dir)?;
    let mf = manifest::fetch_from_config(&cfg).ok();

    let resolved = resolve_target_runtime(version_spec)?;
    let runtime_path = config::runtimes_dir()?.join(&resolved);
    if !runtime_path.exists() {
        anyhow::bail!(
            "PHP runtime {} is not installed. Run `phpvm install {}` first.",
            resolved,
            version_spec.unwrap_or(&resolved)
        );
    }

    let catalog = runtime_extension_catalog(&resolved, mf.as_ref())?;
    providers::apply_preset(
        &runtime_path,
        profile_name,
        &project_dir,
        mf.as_ref(),
        &catalog,
        &manifest_entry_for_runtime(&resolved, mf.as_ref(), &catalog)?,
    )?;

    let enabled = profile_preset::parse_enabled_extensions_from_file(
        &profile_preset::resolve_preset(
            profile_name,
            &project_dir,
            &runtime_path,
            mf.as_ref(),
            &catalog,
        )?
        .path,
    )?;

    output::success_stderr(&format!(
        "Switched PHP {} to profile '{}' ({} extensions enabled)",
        resolved,
        profile_name,
        enabled.len()
    ));

    Ok(())
}

fn runtime_extension_catalog(resolved: &str, mf: Option<&Manifest>) -> Result<Vec<String>> {
    if let Some(metadata) = RuntimeMetadata::read(resolved)? {
        if !metadata.available_extensions.is_empty() {
            return Ok(metadata.available_extensions);
        }
    }

    if let Some(mf) = mf {
        if let Some(entry) = mf.find(resolved) {
            let catalog = entry.extension_catalog();
            if !catalog.is_empty() {
                return Ok(catalog);
            }
        }
    }

    anyhow::bail!(
        "Runtime {} has no extension catalog. Reinstall with `phpvm install {}`.",
        resolved,
        resolved
    )
}

fn manifest_entry_for_runtime(
    resolved: &str,
    mf: Option<&Manifest>,
    catalog: &[String],
) -> Result<manifest::ManifestEntry> {
    if let Some(mf) = mf {
        if let Some(entry) = mf.find(resolved) {
            return Ok(entry.clone());
        }
    }

    let metadata = RuntimeMetadata::read(resolved)?
        .with_context(|| format!("Runtime {} is missing metadata.json", resolved))?;

    Ok(manifest::ManifestEntry {
        php: metadata.php,
        composer: metadata.composer,
        profile: None,
        extensions: catalog.to_vec(),
        url: String::new(),
        sha256: String::new(),
    })
}

fn resolve_target_runtime(version_spec: Option<&str>) -> Result<String> {
    let installed = crate::runner::installed_versions()?;
    if installed.is_empty() {
        anyhow::bail!("No runtimes installed. Run `phpvm install <version>` first.");
    }

    if let Some(spec) = version_spec {
        return crate::version::resolve_specifier(spec, &installed);
    }

    if let Ok(active) = std::env::var("PHPVM_VERSION") {
        if !active.is_empty() && installed.iter().any(|v| v == &active) {
            return Ok(active);
        }
    }

    if let Some(active) = config::get_current_version() {
        if installed.iter().any(|v| v == &active) {
            return Ok(active);
        }
    }

    anyhow::bail!("No active PHP runtime. Pass --version or run `phpvm use <version>` first.")
}

// ---------------------------------------------------------------------------
// Listing and managing presets
// ---------------------------------------------------------------------------

/// List all known profile presets.
pub fn list_profiles(format: OutputFormat) -> Result<()> {
    let project_dir = config::current_project_dir()?;
    let runtime_dir = active_runtime_dir()?;
    let presets = profile_preset::discover_presets(&project_dir, runtime_dir.as_deref())?;

    match format {
        OutputFormat::Human => print_presets_human(&presets),
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&presets)
                .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
            println!("{}", json);
        }
    }

    Ok(())
}

fn print_presets_human(presets: &[ListedPreset]) {
    output::info("Available Profile Presets");
    output::info("========================");
    for preset in presets {
        output::list_item(&format!(
            "{} [{}] {}",
            preset.name, preset.source, preset.path
        ));
    }
}

/// Print the resolved path for a profile preset.
pub fn preset_path(name: Option<&str>, version_spec: Option<&str>) -> Result<()> {
    let project_dir = config::current_project_dir()?;
    let cfg = config::load_config(&project_dir)?;
    let mf = manifest::fetch_from_config(&cfg).ok();

    let preset_name = match name {
        Some(n) => n.to_string(),
        None => default_preset_name(&project_dir, version_spec)?,
    };

    let runtime_dir = resolve_runtime_dir(version_spec)?;
    let catalog = runtime_dir
        .as_ref()
        .and_then(|dir| {
            dir.file_name()
                .map(|n| n.to_string())
                .and_then(|v| runtime_extension_catalog(&v, mf.as_ref()).ok())
        })
        .unwrap_or_default();

    let resolved = profile_preset::resolve_preset(
        &preset_name,
        &project_dir,
        runtime_dir
            .as_deref()
            .unwrap_or_else(|| camino::Utf8Path::new("/nonexistent")),
        mf.as_ref(),
        &catalog,
    )?;

    println!("{}", resolved.path);
    Ok(())
}

/// Open a profile preset in the user's editor.
pub fn edit_preset(name: Option<&str>, version_spec: Option<&str>) -> Result<()> {
    let project_dir = config::current_project_dir()?;
    let cfg = config::load_config(&project_dir)?;
    let mf = manifest::fetch_from_config(&cfg).ok();

    let preset_name = match name {
        Some(n) => n.to_string(),
        None => default_preset_name(&project_dir, version_spec)?,
    };

    let runtime_dir = resolve_runtime_dir(version_spec)?;
    let catalog = runtime_dir
        .as_ref()
        .and_then(|dir| {
            dir.file_name()
                .map(|n| n.to_string())
                .and_then(|v| runtime_extension_catalog(&v, mf.as_ref()).ok())
        })
        .unwrap_or_default();

    let resolved = profile_preset::resolve_preset(
        &preset_name,
        &project_dir,
        runtime_dir
            .as_deref()
            .unwrap_or_else(|| camino::Utf8Path::new("/nonexistent")),
        mf.as_ref(),
        &catalog,
    )?;

    profile_preset::edit_preset(&resolved.path)
}

/// Create a new profile preset file.
pub fn new_preset(
    name: &str,
    global: bool,
    from_template: Option<&str>,
    version_spec: Option<&str>,
) -> Result<()> {
    let project_dir = config::current_project_dir()?;
    let cfg = config::load_config(&project_dir)?;
    let mf = manifest::fetch_from_config(&cfg).ok();

    let runtime_dir = resolve_runtime_dir(version_spec)?;
    let catalog = runtime_dir
        .as_ref()
        .and_then(|dir| {
            dir.file_name()
                .map(|n| n.to_string())
                .and_then(|v| runtime_extension_catalog(&v, mf.as_ref()).ok())
        })
        .unwrap_or_default();

    let path = profile_preset::create_preset(
        name,
        &project_dir,
        global,
        from_template,
        mf.as_ref(),
        &catalog,
    )?;

    output::success(&format!("Created profile preset: {}", path));
    Ok(())
}

/// Fork an existing preset into the project profiles directory.
pub fn fork_preset(src: &str, dst: &str, version_spec: Option<&str>) -> Result<()> {
    let project_dir = config::current_project_dir()?;
    let cfg = config::load_config(&project_dir)?;
    let mf = manifest::fetch_from_config(&cfg).ok();
    let runtime_dir = resolve_runtime_dir(version_spec)?;
    let catalog = runtime_dir
        .as_ref()
        .and_then(|dir| {
            dir.file_name()
                .map(|n| n.to_string())
                .and_then(|v| runtime_extension_catalog(&v, mf.as_ref()).ok())
        })
        .unwrap_or_default();

    let path = profile_preset::fork_preset(
        src,
        dst,
        &project_dir,
        runtime_dir.as_deref(),
        mf.as_ref(),
        &catalog,
    )?;

    output::success(&format!("Forked profile preset to: {}", path));
    Ok(())
}

/// Resolve enabled extensions for a named preset (for doctor/info; read-only).
pub fn enabled_extensions_for_preset(
    name: &str,
    project_dir: &camino::Utf8Path,
    mf: Option<&Manifest>,
) -> Result<Vec<String>> {
    let runtime_dir = active_runtime_dir()?;
    let catalog = runtime_dir
        .as_ref()
        .and_then(|dir| {
            dir.file_name()
                .map(|n| n.to_string())
                .and_then(|v| runtime_extension_catalog(&v, mf).ok())
        })
        .unwrap_or_default();

    let runtime_lookup = runtime_dir
        .as_deref()
        .unwrap_or_else(|| camino::Utf8Path::new("/nonexistent"));

    if let Some(preset) = profile_preset::find_existing_preset(name, project_dir, runtime_lookup)? {
        return profile_preset::parse_enabled_extensions_from_file(&preset.path);
    }

    let content = profile_preset::starter_content(name, mf, &catalog)?;
    Ok(profile_preset::parse_enabled_extensions(&content))
}

fn default_preset_name(
    project_dir: &camino::Utf8Path,
    version_spec: Option<&str>,
) -> Result<String> {
    if let Some(dir) = resolve_runtime_dir(version_spec)? {
        if let Some(name) = dir.file_name().and_then(|n| {
            RuntimeMetadata::read(n)
                .ok()
                .flatten()
                .map(|m| m.active_profile)
        }) {
            return Ok(name);
        }
    }

    let cfg = config::load_config(project_dir)?;
    Ok(cfg.profile.unwrap_or_else(|| "minimal".to_string()))
}

fn active_runtime_dir() -> Result<Option<camino::Utf8PathBuf>> {
    resolve_runtime_dir(None)
}

fn resolve_runtime_dir(version_spec: Option<&str>) -> Result<Option<camino::Utf8PathBuf>> {
    let installed = crate::runner::installed_versions()?;
    if installed.is_empty() {
        return Ok(None);
    }

    let resolved = if let Some(spec) = version_spec {
        Some(crate::version::resolve_specifier(spec, &installed)?)
    } else if let Ok(active) = std::env::var("PHPVM_VERSION") {
        if !active.is_empty() && installed.iter().any(|v| v == &active) {
            Some(active)
        } else {
            config::get_current_version().filter(|v| installed.iter().any(|i| i == v))
        }
    } else {
        config::get_current_version().filter(|v| installed.iter().any(|i| i == v))
    };

    let runtimes_dir = config::runtimes_dir()?;
    Ok(resolved.map(|v| runtimes_dir.join(v)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordpress_template_has_expected_extensions() {
        let p = wordpress_template();
        assert_eq!(p.name, "wordpress");
        assert!(p.extensions.contains(&"mysqli".to_string()));
        assert_eq!(p.extensions.len(), 10);
    }

    #[test]
    fn profile_template_serialization_roundtrip() {
        let p = wordpress_template();
        let json = serde_json::to_string(&p).unwrap();
        let deserialized: ProfileTemplate = serde_json::from_str(&json).unwrap();
        assert_eq!(p, deserialized);
    }
}
