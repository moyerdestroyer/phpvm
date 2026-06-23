pub mod static_php;

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};

use crate::manifest::{Manifest, ManifestEntry};

/// A runtime provider knows how to install and set up a PHP runtime.
pub trait Provider {
    /// Human-readable name for this provider.
    #[allow(dead_code)]
    fn name(&self) -> &str;

    /// Install a runtime described by the manifest entry into the target directory.
    fn install(
        &self,
        entry: &ManifestEntry,
        target: &Utf8PathBuf,
        profile_name: &str,
        project_dir: &Utf8Path,
        manifest: Option<&Manifest>,
    ) -> Result<()>;
}

/// Apply a profile ini preset to an installed runtime (no download).
pub fn apply_preset(
    target: &Utf8PathBuf,
    profile_name: &str,
    project_dir: &Utf8Path,
    manifest: Option<&Manifest>,
    entry: &ManifestEntry,
) -> Result<()> {
    static_php::apply_preset(target, profile_name, project_dir, manifest, entry)
}

/// Return the default provider (static_php for V1).
pub fn default_provider() -> Box<dyn Provider> {
    Box::new(static_php::StaticPhpProvider)
}
