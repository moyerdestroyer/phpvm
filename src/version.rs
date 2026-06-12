use anyhow::Result;
use semver::Version;

use crate::config;
use crate::output;

/// A resolved PHP version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub struct PhpVersion {
    /// The major.minor.patch version string.
    pub version: String,
}

impl std::fmt::Display for PhpVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.version)
    }
}

/// Resolve a version specifier to a concrete PHP version.
///
/// Supported formats:
///   "8.3"       → latest available 8.3.x
///   "8.3.latest" → latest available 8.3.x
///   "8.3.min"   → minimum available 8.3.x
///   "8.3.12"    → exact version 8.3.12
pub fn resolve(specifier: &str) -> Result<String> {
    // Handle exact versions (e.g. "8.3.12")
    if let Ok(_version) = Version::parse(specifier) {
        // TODO: Verify this version exists in the manifest
        return Ok(specifier.to_string());
    }

    // Handle special suffixes
    if let Some(stripped) = specifier.strip_suffix(".latest") {
        // TODO: Look up latest patch version from manifest
        return Ok(format!("{}.latest", stripped));
    }

    if let Some(stripped) = specifier.strip_suffix(".min") {
        // TODO: Look up minimum patch version from manifest
        return Ok(format!("{}.min", stripped));
    }

    // Handle bare major.minor (e.g. "8.3") → treat as .latest
    if specifier.split('.').count() == 2 {
        // TODO: Look up latest patch version from manifest
        return Ok(format!("{}.latest", specifier));
    }

    anyhow::bail!("Invalid PHP version specifier: {}", specifier)
}

/// List all installed runtimes.
pub fn list_installed() -> Result<()> {
    let runtimes_dir = config::runtimes_dir()?;

    if !runtimes_dir.exists() {
        output::info("No runtimes installed.");
        return Ok(());
    }

    let mut versions: Vec<String> = std::fs::read_dir(&runtimes_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();

    versions.sort();

    if versions.is_empty() {
        output::info("No runtimes installed.");
    } else {
        output::info("Installed runtimes:");
        for v in &versions {
            println!("  {}", v);
        }
    }

    Ok(())
}
