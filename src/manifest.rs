use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A single runtime entry from the remote manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// PHP version (e.g. "8.3.23")
    pub php: String,

    /// Bundled Composer version (e.g. "2.9.2")
    pub composer: String,

    /// Extension profile name (e.g. "wordpress", "laravel", "minimal")
    pub profile: String,

    /// Download URL for the runtime archive
    pub url: String,

    /// SHA-256 checksum of the archive
    pub sha256: String,
}

/// The full manifest: a collection of available runtimes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub runtimes: Vec<ManifestEntry>,
}

/// Default manifest URL.
#[allow(dead_code)]
const DEFAULT_MANIFEST_URL: &str = "https://phpvm.com/manifest.json";

/// Fetch the full manifest from the remote source.
pub fn fetch() -> Result<Manifest> {
    // TODO: Implement manifest fetching with caching
    // TODO: Use config.manifest_url if set, otherwise DEFAULT_MANIFEST_URL
    // TODO: Cache the manifest locally
    anyhow::bail!("Manifest fetching not yet implemented")
}

/// Fetch a single manifest entry for a specific PHP version.
pub fn fetch_entry(version: &str) -> Result<ManifestEntry> {
    let manifest = fetch()?;
    manifest
        .runtimes
        .into_iter()
        .find(|e| e.php == version)
        .ok_or_else(|| anyhow::anyhow!("PHP version {} not found in manifest", version))
}
