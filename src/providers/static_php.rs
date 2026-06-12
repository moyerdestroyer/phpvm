use anyhow::Result;
use camino::Utf8PathBuf;

use super::Provider;
use crate::manifest::ManifestEntry;

/// Provider that downloads prebuilt/static PHP binaries.
///
/// This is the primary provider for V1. It:
/// 1. Downloads a prebuilt runtime archive from the manifest URL
/// 2. Verifies the SHA-256 checksum
/// 3. Extracts the archive into the runtime directory
/// 4. Verifies the runtime is functional
pub struct StaticPhpProvider;

impl Provider for StaticPhpProvider {
    fn name(&self) -> &str {
        "static_php"
    }

    fn install(&self, _entry: &ManifestEntry, _target: &Utf8PathBuf) -> Result<()> {
        // TODO: Download archive from entry.url
        // TODO: Verify SHA-256 checksum matches entry.sha256
        // TODO: Extract archive to target directory
        // TODO: Verify php binary is executable
        // TODO: Verify composer binary is executable
        // TODO: Write metadata.json to target directory
        Ok(())
    }
}
