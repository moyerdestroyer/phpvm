mod docker;
mod local;
mod static_php;

use anyhow::Result;
use camino::Utf8PathBuf;

use crate::manifest::ManifestEntry;
use crate::profile::Profile;

/// A runtime provider knows how to install and set up a PHP runtime.
pub trait Provider {
    /// Human-readable name for this provider.
    #[allow(dead_code)]
    fn name(&self) -> &str;

    /// Install a runtime described by the manifest entry into the target directory.
    fn install(&self, entry: &ManifestEntry, target: &Utf8PathBuf, profile: &Profile)
        -> Result<()>;
}

/// Return the default provider (static_php for V1).
pub fn default_provider() -> Box<dyn Provider> {
    Box::new(static_php::StaticPhpProvider)
}
