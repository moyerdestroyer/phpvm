use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};

use super::Provider;
use crate::manifest::{Manifest, ManifestEntry};

/// Provider that uses a locally-installed PHP runtime.
///
/// This provider wraps an existing PHP installation on the host machine.
/// It is primarily useful for development and testing, but violates the
/// host-independence principle and should not be relied upon for
/// compatibility verification.
#[allow(dead_code)]
pub struct LocalProvider;

impl Provider for LocalProvider {
    fn name(&self) -> &str {
        "local"
    }

    fn install(
        &self,
        _entry: &ManifestEntry,
        _target: &Utf8PathBuf,
        _profile_name: &str,
        _project_dir: &Utf8Path,
        _manifest: Option<&Manifest>,
        _catalog: &[String],
    ) -> Result<()> {
        anyhow::bail!(
            "Local provider creates a symlink to host PHP. \
             This violates host independence and is not recommended for compatibility testing."
        )
    }
}
