use anyhow::Result;

use crate::config;
use crate::manifest;
use crate::output::{self, StepList};
use crate::providers;
use crate::runtime_metadata::RuntimeMetadata;

/// Install a PHP runtime.
///
/// Flow: Resolve version against manifest → Download full binary once →
///       Apply initial profile ini preset
pub fn run(version: &str, profile_name: Option<&str>) -> Result<()> {
    output::heading(&format!("Installing PHP {version}"));

    let project_dir = config::current_project_dir()?;
    let config = config::load_config(&project_dir)?;
    let mf = manifest::fetch_from_config(&config)?;

    let profile_label =
        profile_name.unwrap_or_else(|| config.profile.as_deref().unwrap_or("minimal"));

    let available = mf.available_versions();
    if available.is_empty() {
        anyhow::bail!("Manifest has no runtime artifacts available.");
    }

    let resolved = crate::version::resolve_specifier(version, &available)?;

    output::label("Profile:", profile_label);
    output::label("Resolved:", &resolved);
    output::blank();

    let manifest_entry = mf
        .find(&resolved)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("PHP version {} not found in manifest", resolved))?;

    let runtimes_dir = config::runtimes_dir()?;
    let runtime_path = runtimes_dir.join(&resolved);
    let runtime_installed = runtime_is_complete(&runtime_path);

    if runtime_installed {
        if let Some(active) = RuntimeMetadata::read_active_profile(&resolved)? {
            if active == profile_label {
                output::success(&format!(
                    "Runtime {} already installed (profile: {})",
                    resolved, profile_label
                ));
                return Ok(());
            }
        }

        let mut steps = StepList::new();
        steps.done("Runtime present");
        let step_label = format!("Apply profile '{profile_label}'");
        steps.start(&step_label);
        if let Err(e) = providers::apply_preset(
            &runtime_path,
            profile_label,
            &project_dir,
            Some(&mf),
            &manifest_entry,
        ) {
            steps.fail(&step_label, &e.to_string());
            steps.finish();
            return Err(e);
        }
        steps.done(&format!("Applied profile '{profile_label}'"));
        steps.finish();

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
    )?;

    output::blank();
    output::success(&format!("Installed PHP {} ({})", resolved, profile_label));
    Ok(())
}

fn runtime_is_complete(runtime_path: &camino::Utf8Path) -> bool {
    let php = runtime_path
        .join("bin")
        .join(if cfg!(windows) { "php.exe" } else { "php" });
    let composer = runtime_path.join("bin").join(if cfg!(windows) {
        "composer.exe"
    } else {
        "composer"
    });
    php.exists() && composer.exists()
}
