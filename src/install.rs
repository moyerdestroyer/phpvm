use anyhow::Result;

use crate::config;
use crate::manifest;
use crate::output;
use crate::providers;

/// Install a PHP runtime.
///
/// Flow: Resolve version → Download manifest → Verify checksum →
///       Download runtime → Extract → Cache
pub fn run(version: &str) -> Result<()> {
    output::info(&format!("Installing PHP runtime: {}", version));

    let resolved = crate::version::resolve(version)?;
    output::info(&format!("Resolved to: {}", resolved));

    let runtimes_dir = config::runtimes_dir()?;
    let runtime_path = runtimes_dir.join(&resolved);

    if runtime_path.exists() {
        output::info(&format!("Runtime {} already installed", resolved));
        return Ok(());
    }

    // Fetch manifest entry for this version
    let manifest_entry = manifest::fetch_entry(&resolved)?;

    // Download and verify the runtime
    let provider = providers::default_provider();
    provider.install(&manifest_entry, &runtime_path)?;

    output::success(&format!("Installed PHP {}", resolved));
    Ok(())
}
