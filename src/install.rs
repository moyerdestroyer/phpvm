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
    let profile_versions: Vec<String> = mf
        .runtimes
        .iter()
        .filter(|entry| entry.profile == resolved_profile.name)
        .map(|entry| entry.php.clone())
        .collect();
    let available = if profile_versions.is_empty() {
        output::warn(&format!(
            "Manifest has no artifact for profile '{}'; recording the selected profile \
             and using the available runtime for the PHP version.",
            resolved_profile.name
        ));
        mf.available_versions()
    } else {
        profile_versions
    };

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
        .find_with_profile(&resolved, &resolved_profile.name)
        .or_else(|| mf.find(&resolved))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("PHP version {} not found in manifest", resolved))?;
    if manifest_entry.profile != resolved_profile.name {
        output::warn(&format!(
            "Runtime artifact is tagged with manifest profile '{}'; PHPVM will record \
             your selected profile '{}' and its configured extensions.",
            manifest_entry.profile, resolved_profile.name
        ));
    }

    // Download and verify the runtime.
    let provider = providers::default_provider();
    provider.install(&manifest_entry, &runtime_path, &resolved_profile)?;

    output::success(&format!("Installed PHP {}", resolved));
    Ok(())
}
