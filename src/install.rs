use anyhow::Result;

use crate::config;
use crate::manifest;
use crate::output;
use crate::providers;
use crate::runtime_metadata::RuntimeMetadata;

/// Install a PHP runtime.
///
/// Flow: Resolve version against manifest → Download full binary once →
///       Apply initial profile ini preset
pub fn run(version: &str, profile_name: Option<&str>) -> Result<()> {
    output::info(&format!("Installing PHP runtime: {}", version));

    let project_dir = config::current_project_dir()?;
    let config = config::load_config(&project_dir)?;
    let mf = manifest::fetch_from_config(&config)?;

    let profile_label =
        profile_name.unwrap_or_else(|| config.profile.as_deref().unwrap_or("minimal"));

    output::info(&format!("Profile: {}", profile_label));

    let available = mf.available_versions();
    if available.is_empty() {
        anyhow::bail!("Manifest has no runtime artifacts available.");
    }

    let resolved = crate::version::resolve_specifier(version, &available)?;
    output::info(&format!("Resolved to: {}", resolved));

    let manifest_entry = mf
        .find(&resolved)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("PHP version {} not found in manifest", resolved))?;

    let catalog = manifest_entry.extension_catalog();
    if catalog.is_empty() {
        output::warn(
            "Manifest entry has no extension catalog; profile switching may be limited \
             until full-binary manifests are published.",
        );
    }

    let runtimes_dir = config::runtimes_dir()?;
    let runtime_path = runtimes_dir.join(&resolved);
    let runtime_installed = runtime_path
        .join("bin")
        .join(if cfg!(windows) { "php.exe" } else { "php" })
        .exists();

    if runtime_installed {
        if let Some(active) = RuntimeMetadata::read_active_profile(&resolved)? {
            if active == profile_label {
                output::info(&format!(
                    "Runtime {} already installed (profile: {})",
                    resolved, profile_label
                ));
                return Ok(());
            }
        }

        output::info(&format!(
            "Runtime {} already installed; switching to profile '{}'",
            resolved, profile_label
        ));
        providers::apply_preset(
            &runtime_path,
            profile_label,
            &project_dir,
            Some(&mf),
            &catalog,
            &manifest_entry,
        )?;
        output::success(&format!(
            "Switched PHP {} to profile '{}'",
            resolved, profile_label
        ));
        return Ok(());
    }

    let provider = providers::default_provider();
    provider.install(
        &manifest_entry,
        &runtime_path,
        profile_label,
        &project_dir,
        Some(&mf),
        &catalog,
    )?;

    output::success(&format!("Installed PHP {} ({})", resolved, profile_label));
    Ok(())
}
