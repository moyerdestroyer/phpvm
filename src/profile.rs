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

/// A named starter preset advertised by the manifest.
///
/// Manifests may include extension recommendations alongside a profile name. PHPVM
/// deliberately ignores them: static runtime extensions are compiled into the binary
/// and profiles only tune INI settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ProfileTemplate {
    pub name: String,
}

// ---------------------------------------------------------------------------
// Profile switching
// ---------------------------------------------------------------------------

/// Switch the active profile preset for an installed runtime (updates metadata
/// and, for static runtimes, the phpvm-managed user ini outside the runtime tree).
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

    providers::apply_preset(
        &runtime_path,
        profile_name,
        &project_dir,
        mf.as_ref(),
        &manifest_entry_for_runtime(&resolved, mf.as_ref())?,
    )?;

    output::success_stderr(&format!(
        "Switched PHP {} to profile '{}'",
        resolved, profile_name
    ));

    Ok(())
}

fn manifest_entry_for_runtime(
    resolved: &str,
    mf: Option<&Manifest>,
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
        runtime_type: metadata.runtime_type,
        abi: metadata.abi,
        thread_safety: metadata.thread_safety,
        extension_api: metadata.extension_api,
        extensions: metadata.extension_catalog,
        url: String::new(),
        sha256: String::new(),
        artifacts: None,
    })
}

fn resolve_target_runtime(version_spec: Option<&str>) -> Result<String> {
    crate::version::resolve_active_runtime(version_spec)
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
    output::heading("Available Profile Presets");
    for preset in presets {
        output::list_item(&format!(
            "{} {} {}",
            output::bold(&preset.name),
            output::dim(&format!("[{}]", preset.source)),
            output::dim(&preset.path)
        ));
    }
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

    let resolved = profile_preset::resolve_preset(
        &preset_name,
        &project_dir,
        runtime_dir
            .as_deref()
            .unwrap_or_else(|| camino::Utf8Path::new("/nonexistent")),
        mf.as_ref(),
    )?;

    profile_preset::edit_preset(&resolved.path)
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
    } else {
        crate::version::resolve_active_version(&installed)
    };

    let runtimes_dir = config::runtimes_dir()?;
    Ok(resolved.map(|v| runtimes_dir.join(v)))
}
