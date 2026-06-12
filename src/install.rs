use anyhow::Result;

use crate::config;
use crate::manifest;
use crate::output;
use crate::profile;
use crate::providers;

/// Install a PHP runtime.
///
/// Flow: Resolve version against manifest → Download manifest entry →
///       Verify checksum → Download runtime → Extract → Cache
pub fn run(version: &str, profile_name: Option<&str>) -> Result<()> {
    output::info(&format!("Installing PHP runtime: {}", version));

    // Resolve the profile (and potentially manifest override).
    let project_dir = config::current_project_dir()?;
    let config = config::load_config(&project_dir)?;
    let resolved_profile = profile::resolve_or_minimal(
        profile_name.unwrap_or_else(|| config.profile.as_deref().unwrap_or("minimal")),
        &config.profiles,
    );
    output::info(&format!("Profile: {}", resolved_profile.name));
    if !resolved_profile.extensions.is_empty() {
        output::info(&format!(
            "Extensions: {}",
            resolved_profile.extensions.join(", ")
        ));
    }

    // Fetch the manifest to know which versions are available.
    // Respects config.manifest_url if set in .phpvm.toml or global config.
    let mf = manifest::fetch_from_config(&config)?;
    let available = mf.available_versions();

    // Resolve the user's specifier against the available versions.
    let resolved = crate::version::resolve_specifier(version, &available)?;
    output::info(&format!("Resolved to: {}", resolved));

    let runtimes_dir = config::runtimes_dir()?;
    let runtime_path = runtimes_dir.join(&resolved);

    if runtime_path.exists() {
        output::info(&format!("Runtime {} already installed", resolved));
        return Ok(());
    }

    // Fetch manifest entry for the resolved version.
    let manifest_entry = mf
        .find(&resolved)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("PHP version {} not found in manifest", resolved))?;

    // Download and verify the runtime.
    let provider = providers::default_provider();
    provider.install(&manifest_entry, &runtime_path)?;

    output::success(&format!("Installed PHP {}", resolved));
    Ok(())
}
