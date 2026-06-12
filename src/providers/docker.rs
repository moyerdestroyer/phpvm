use anyhow::Result;
use camino::Utf8PathBuf;

use super::Provider;
use crate::manifest::ManifestEntry;

/// Provider that uses Docker containers for PHP runtimes.
///
/// This is a future provider. V1 does not implement Docker support,
/// but the trait is defined here so the architecture is ready.
#[allow(dead_code)]
pub struct DockerProvider;

impl Provider for DockerProvider {
    fn name(&self) -> &str {
        "docker"
    }

    fn install(&self, _entry: &ManifestEntry, _target: &Utf8PathBuf) -> Result<()> {
        anyhow::bail!("Docker provider is not yet implemented")
    }
}
