use std::collections::BTreeMap;
use std::fs;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::config;
use crate::profile::ProfileTemplate;

/// A downloadable runtime archive (manifest v2.1 per-platform artifacts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestArtifact {
    pub url: String,
    pub sha256: String,
}

/// Runtime packaging mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeType {
    #[default]
    Static,
    Dynamic,
}

impl RuntimeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Dynamic => "dynamic",
        }
    }
}

fn default_extension_type() -> String {
    "extension".to_string()
}

fn default_bundled() -> bool {
    true
}

/// A loadable or compiled extension advertised by a runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeExtension {
    pub name: String,
    #[serde(default = "default_extension_type", rename = "type")]
    pub extension_type: String,
    #[serde(default = "default_bundled")]
    pub bundled: bool,
    #[serde(default, rename = "default")]
    pub default_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

impl RuntimeExtension {
    pub fn from_name(name: String) -> Self {
        Self {
            name,
            extension_type: default_extension_type(),
            bundled: true,
            default_enabled: false,
            file: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RuntimeExtensionInput {
    Name(String),
    Detail(RuntimeExtension),
}

fn deserialize_extensions<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<RuntimeExtension>, D::Error>
where
    D: Deserializer<'de>,
{
    let inputs = Vec::<RuntimeExtensionInput>::deserialize(deserializer)?;
    Ok(inputs
        .into_iter()
        .map(|input| match input {
            RuntimeExtensionInput::Name(name) => RuntimeExtension::from_name(name),
            RuntimeExtensionInput::Detail(detail) => detail,
        })
        .collect())
}

/// A single runtime entry from the remote manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// PHP version (e.g. "8.3.23")
    pub php: String,

    /// Bundled Composer version (e.g. "2.9.2")
    pub composer: String,

    /// v1 manifest artifact profile tag (deprecated; normalized away on parse).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Runtime packaging model. v2/v2.1 static manifests default to Static (the
    /// long-term supported model). Dynamic is legacy/conditional.
    #[serde(default)]
    pub runtime_type: RuntimeType,

    /// PHP extension ABI metadata for dynamic runtimes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_safety: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_api: Option<String>,

    /// Extensions available in the runtime.
    #[serde(default, deserialize_with = "deserialize_extensions")]
    pub extensions: Vec<RuntimeExtension>,

    /// Download URL for the runtime archive (manifest v2; empty when using `artifacts`).
    #[serde(default)]
    pub url: String,

    /// SHA-256 checksum of the archive (manifest v2; empty when using `artifacts`).
    #[serde(default)]
    pub sha256: String,

    /// Per-platform artifacts (manifest v2.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<BTreeMap<String, ManifestArtifact>>,
}

/// The full manifest: profile presets and available runtimes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Manifest schema version. v2/v2.1 are static full binaries; v3 is dynamic-capable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Named INI starter presets (not runtime configuration).
    #[serde(default)]
    pub profiles: Vec<ProfileTemplate>,

    pub runtimes: Vec<ManifestEntry>,
}

/// Default manifest URL (phpvm-runtimes catalog on GitHub).
const DEFAULT_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/moyerdestroyer/phpvm-runtimes/master/manifest.json";

// ── Manifest methods ────────────────────────────────────────────────────

impl ManifestEntry {
    /// Resolve the download artifact for the current host triple.
    pub fn download_for_host(&self) -> Result<ManifestArtifact> {
        if let Some(artifacts) = &self.artifacts {
            let target = host_target()?;
            return artifacts.get(&target).cloned().ok_or_else(|| {
                anyhow::anyhow!(
                    "No runtime artifact published for host target '{}'. \
                     PHP {} is not available on this platform in the manifest.",
                    target,
                    self.php
                )
            });
        }

        if self.url.is_empty() || self.sha256.is_empty() {
            anyhow::bail!(
                "Manifest entry for PHP {} has no download URL or checksum",
                self.php
            );
        }

        Ok(ManifestArtifact {
            url: self.url.clone(),
            sha256: self.sha256.clone(),
        })
    }

    /// Extensions available in the installed binary for this runtime.
    pub fn extension_catalog(&self) -> Vec<String> {
        self.extensions.iter().map(|ext| ext.name.clone()).collect()
    }
}

impl Manifest {
    /// Find an entry by exact PHP version string.
    pub fn find(&self, php_version: &str) -> Option<&ManifestEntry> {
        self.runtimes.iter().find(|e| e.php == php_version)
    }

    /// Resolve a manifest profile template by name.
    pub fn resolve_profile_template(&self, name: &str) -> Option<ProfileTemplate> {
        self.profiles.iter().find(|p| p.name == name).cloned()
    }

    /// Find the latest (highest) patch version for a given `major.minor`.
    #[allow(dead_code)]
    pub fn latest_patch(&self, major: u32, minor: u32) -> Option<&ManifestEntry> {
        self.runtimes
            .iter()
            .filter(|e| {
                if let Ok(v) = semver::Version::parse(&e.php) {
                    v.major == major as u64 && v.minor == minor as u64
                } else {
                    false
                }
            })
            .max_by(|a, b| {
                let va = semver::Version::parse(&a.php);
                let vb = semver::Version::parse(&b.php);
                match (&va, &vb) {
                    (Ok(va), Ok(vb)) => va.cmp(vb),
                    (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
                    (Err(_), Ok(_)) => std::cmp::Ordering::Less,
                    (Err(_), Err(_)) => std::cmp::Ordering::Equal,
                }
            })
    }

    /// Find the minimum (lowest) patch version for a given `major.minor`.
    #[allow(dead_code)]
    pub fn min_patch(&self, major: u32, minor: u32) -> Option<&ManifestEntry> {
        self.runtimes
            .iter()
            .filter(|e| {
                if let Ok(v) = semver::Version::parse(&e.php) {
                    v.major == major as u64 && v.minor == minor as u64
                } else {
                    false
                }
            })
            .min_by(|a, b| {
                let va = semver::Version::parse(&a.php);
                let vb = semver::Version::parse(&b.php);
                match (&va, &vb) {
                    (Ok(va), Ok(vb)) => va.cmp(vb),
                    (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                    (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                    (Err(_), Err(_)) => std::cmp::Ordering::Equal,
                }
            })
    }

    /// Return a sorted list of all available PHP version strings.
    pub fn available_versions(&self) -> Vec<String> {
        let mut versions: Vec<String> = self.runtimes.iter().map(|e| e.php.clone()).collect();
        versions.sort_by(|a, b| {
            let va = semver::Version::parse(a);
            let vb = semver::Version::parse(b);
            match (&va, &vb) {
                (Ok(va), Ok(vb)) => va.cmp(vb).then(a.cmp(b)),
                (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                (Err(_), Err(_)) => a.cmp(b),
            }
        });
        versions
    }
}

// ── Public functions ────────────────────────────────────────────────────

/// Fetch the manifest from a URL.
pub fn fetch(url: &str) -> Result<Manifest> {
    let client = crate::net::blocking_client()?;
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("Failed to fetch manifest from {}", url))?
        .error_for_status()
        .with_context(|| format!("Manifest fetch returned error status from {}", url))?;

    let body = resp
        .text()
        .with_context(|| "Failed to read manifest response body")?;

    parse_manifest(&body)
}

/// Fetch the manifest with local caching.
pub fn fetch_cached(url: &str, cache_dir: &Utf8PathBuf) -> Result<Manifest> {
    let cache_path = manifest_cache_path(url, cache_dir);
    let max_age = Duration::from_secs(3600);

    if let Ok(meta) = fs::metadata(&cache_path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                if elapsed < max_age {
                    match parse_file(&cache_path) {
                        Ok(m) => return Ok(m),
                        Err(e) => {
                            crate::output::warn(&format!(
                                "cached manifest is invalid ({}) — re-fetching",
                                e
                            ));
                        }
                    }
                }
            }
        }
    }

    let manifest = fetch(url)?;

    fs::create_dir_all(cache_dir)
        .with_context(|| format!("Failed to create cache directory {}", cache_dir))?;

    let json = serde_json::to_string_pretty(&manifest)
        .with_context(|| "Failed to serialize manifest for caching")?;

    fs::write(&cache_path, json)
        .with_context(|| format!("Failed to write cached manifest to {}", cache_path))?;

    Ok(manifest)
}

/// Fetch the manifest from the default URL, using the default cache directory.
pub fn fetch_default() -> Result<Manifest> {
    let cache_dir = config::cache_dir()?;
    fetch_cached(DEFAULT_MANIFEST_URL, &cache_dir)
}

/// Fetch the manifest, using `config.manifest_url` if set, otherwise the default URL.
pub fn fetch_from_config(config: &config::Config) -> Result<Manifest> {
    let url = config
        .manifest_url
        .as_deref()
        .unwrap_or(DEFAULT_MANIFEST_URL);
    let cache_dir = config::cache_dir()?;
    fetch_cached(url, &cache_dir)
}

/// Detect the host target triple (matches `install.sh` resolution).
pub fn host_target() -> Result<String> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => {
            anyhow::bail!(
                "Unsupported architecture: {} (supported: x86_64, aarch64)",
                other
            );
        }
    };

    let target = match std::env::consts::OS {
        "macos" => format!("{arch}-apple-darwin"),
        "linux" => format!("{arch}-unknown-linux-gnu"),
        other => anyhow::bail!("Unsupported OS: {other} (supported: linux, macos)"),
    };

    Ok(target)
}

/// Parse a JSON string into a `Manifest`.
pub fn parse_manifest(json: &str) -> Result<Manifest> {
    let raw: Manifest =
        serde_json::from_str(json).with_context(|| "Failed to parse manifest JSON")?;
    normalize_manifest(raw)
}

/// Fetch a single manifest entry for a specific PHP version.
#[allow(dead_code)]
pub fn fetch_entry(version: &str) -> Result<ManifestEntry> {
    let manifest = fetch_default()?;
    manifest
        .find(version)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("PHP version {} not found in manifest", version))
}

// ── Internal helpers ────────────────────────────────────────────────────

fn normalize_manifest(mut manifest: Manifest) -> Result<Manifest> {
    let mut grouped: BTreeMap<String, Vec<ManifestEntry>> = BTreeMap::new();
    for entry in manifest.runtimes {
        grouped.entry(entry.php.clone()).or_default().push(entry);
    }

    let mut runtimes = Vec::new();
    for (php, entries) in grouped {
        let merged = merge_runtime_entries(&php, entries)?;
        runtimes.push(merged);
    }

    runtimes.sort_by(|a, b| {
        let va = semver::Version::parse(&a.php);
        let vb = semver::Version::parse(&b.php);
        match (&va, &vb) {
            (Ok(va), Ok(vb)) => va.cmp(vb),
            (Ok(_), Err(_)) => std::cmp::Ordering::Less,
            (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
            (Err(_), Err(_)) => a.php.cmp(&b.php),
        }
    });

    manifest.runtimes = runtimes;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn merge_runtime_entries(php: &str, entries: Vec<ManifestEntry>) -> Result<ManifestEntry> {
    if entries.len() > 1 {
        let mut urls = std::collections::BTreeSet::new();
        let mut checksums = std::collections::BTreeSet::new();
        let mut artifact_maps = 0usize;
        for entry in &entries {
            if entry.artifacts.is_some() {
                artifact_maps += 1;
            }
            if !entry.url.is_empty() {
                urls.insert(entry.url.as_str());
            }
            if !entry.sha256.is_empty() {
                checksums.insert(entry.sha256.as_str());
            }
        }
        if artifact_maps > 1 {
            anyhow::bail!(
                "Manifest has conflicting per-platform artifacts for PHP {}. \
                 Publish one runtime row per version (manifest v2.1).",
                php
            );
        }
        if urls.len() > 1 || checksums.len() > 1 {
            anyhow::bail!(
                "Manifest has conflicting runtime artifacts for PHP {}. \
                 Publish one full binary per version (manifest v2).",
                php
            );
        }
    }

    let mut extensions: Vec<RuntimeExtension> = Vec::new();
    for entry in &entries {
        if !entry.extensions.is_empty() {
            for ext in &entry.extensions {
                if !extensions.iter().any(|existing| existing.name == ext.name) {
                    extensions.push(ext.clone());
                }
            }
        }
    }

    let base = entries
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("No manifest entries for PHP version {}", php))?;

    Ok(ManifestEntry {
        php: php.to_string(),
        composer: base.composer,
        profile: None,
        runtime_type: base.runtime_type,
        abi: base.abi,
        thread_safety: base.thread_safety,
        extension_api: base.extension_api,
        extensions,
        url: base.url,
        sha256: base.sha256,
        artifacts: base.artifacts,
    })
}

fn validate_artifact(entry_index: usize, label: &str, artifact: &ManifestArtifact) -> Result<()> {
    if artifact.url.is_empty() {
        anyhow::bail!("Manifest entry {} {} has an empty URL", entry_index, label);
    }
    if artifact.sha256.is_empty() {
        anyhow::bail!(
            "Manifest entry {} {} has an empty sha256",
            entry_index,
            label
        );
    }
    let normalized = artifact.sha256.trim();
    if normalized.len() != 64 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!(
            "Manifest entry {} {} has invalid sha256 (expected 64 hex chars): '{}'",
            entry_index,
            label,
            artifact.sha256
        );
    }
    if !artifact.url.starts_with("https://") {
        anyhow::bail!(
            "Manifest entry {} {} must use an https:// download URL, got '{}'",
            entry_index,
            label,
            artifact.url
        );
    }
    Ok(())
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    for (i, entry) in manifest.runtimes.iter().enumerate() {
        if entry.php.is_empty() {
            anyhow::bail!("Manifest entry {} has an empty PHP version", i);
        }
        if entry.composer.is_empty() {
            anyhow::bail!("Manifest entry {} has an empty Composer version", i);
        }

        if let Some(artifacts) = &entry.artifacts {
            if artifacts.is_empty() {
                anyhow::bail!("Manifest entry {} has an empty artifacts map", i);
            }
            for (target, artifact) in artifacts {
                validate_artifact(i, &format!("artifacts[{target}]"), artifact)?;
            }
        } else {
            validate_artifact(
                i,
                "runtime",
                &ManifestArtifact {
                    url: entry.url.clone(),
                    sha256: entry.sha256.clone(),
                },
            )?;
        }

        semver::Version::parse(&entry.php).with_context(|| {
            format!(
                "Manifest entry {} has invalid PHP version: '{}'",
                i, entry.php
            )
        })?;
    }

    for profile in &manifest.profiles {
        if profile.name.is_empty() {
            anyhow::bail!("Manifest profile has an empty name");
        }
    }

    Ok(())
}

fn parse_file(path: &camino::Utf8Path) -> Result<Manifest> {
    let json = fs::read_to_string(path)
        .with_context(|| format!("Failed to read cached manifest from {}", path))?;
    parse_manifest(&json)
}

fn manifest_cache_path(url: &str, cache_dir: &Utf8PathBuf) -> Utf8PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    cache_dir.join(format!("manifest-{}.json", &digest[..16]))
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const FIXTURE_V1_JSON: &str = r#"{
  "runtimes": [
    {
      "php": "8.1.0",
      "composer": "2.6.0",
      "profile": "minimal",
      "url": "https://example.com/php-8.1.0.tar.gz",
      "sha256": "0000000000000000000000000000000000000000000000000000000000000001"
    },
    {
      "php": "8.2.0",
      "composer": "2.8.0",
      "profile": "wordpress",
      "url": "https://example.com/php-8.2.0.tar.gz",
      "sha256": "0000000000000000000000000000000000000000000000000000000000000002"
    },
    {
      "php": "8.3.12",
      "composer": "2.9.2",
      "profile": "laravel",
      "url": "https://example.com/php-8.3.12.tar.gz",
      "sha256": "0000000000000000000000000000000000000000000000000000000000000003"
    },
    {
      "php": "8.3.23",
      "composer": "2.9.2",
      "profile": "wordpress",
      "url": "https://example.com/php-8.3.23.tar.gz",
      "sha256": "0000000000000000000000000000000000000000000000000000000000000004"
    }
  ]
}"#;

    const FIXTURE_V21_JSON: &str = r#"{
  "schema": "2.1",
  "profiles": [
    {
      "name": "wordpress",
      "extensions": ["curl", "dom", "gd", "intl", "mbstring", "mysqli", "openssl", "pdo_mysql", "xml", "zip"]
    },
    {
      "name": "laravel",
      "extensions": ["curl", "intl", "mbstring", "openssl", "pdo_mysql", "tokenizer", "xml", "zip"]
    },
    {
      "name": "minimal",
      "extensions": []
    }
  ],
  "runtimes": [
    {
      "php": "8.3.31",
      "composer": "2.9.2",
      "extensions": ["curl", "dom", "gd", "intl", "mbstring", "mysqli", "openssl", "pdo_mysql", "tokenizer", "xml", "zip"],
      "artifacts": {
        "x86_64-unknown-linux-gnu": {
          "url": "https://example.com/php-8.3.31-x86_64-unknown-linux-gnu.tar.gz",
          "sha256": "00000000000000000000000000000000000000000000000000000000000000aa"
        },
        "x86_64-apple-darwin": {
          "url": "https://example.com/php-8.3.31-x86_64-apple-darwin.tar.gz",
          "sha256": "00000000000000000000000000000000000000000000000000000000000000bb"
        },
        "aarch64-apple-darwin": {
          "url": "https://example.com/php-8.3.31-aarch64-apple-darwin.tar.gz",
          "sha256": "00000000000000000000000000000000000000000000000000000000000000cc"
        }
      }
    }
  ]
}"#;

    /// Pure static v2.1 entry with *no* runtime_type key (and no legacy url/sha).
    /// Confirms serde default + is_dynamic() + catalog work for the new minimal static model.
    const FIXTURE_PURE_STATIC_V21_JSON: &str = r#"{
  "schema": "2.1",
  "profiles": [{"name": "minimal", "extensions": []}],
  "runtimes": [
    {
      "php": "8.4.5",
      "composer": "2.9.2",
      "extensions": ["curl", "mbstring", "openssl", "pdo_mysql", "tokenizer", "zip"],
      "artifacts": {
        "x86_64-unknown-linux-gnu": {
          "url": "https://example.com/php-8.4.5-x86_64-unknown-linux-gnu.tar.gz",
          "sha256": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
        }
      }
    }
  ]
}"#;

    const FIXTURE_V2_JSON: &str = r#"{
  "profiles": [
    {
      "name": "wordpress",
      "extensions": ["curl", "dom", "gd", "intl", "mbstring", "mysqli", "openssl", "pdo_mysql", "xml", "zip"]
    },
    {
      "name": "laravel",
      "extensions": ["curl", "intl", "mbstring", "openssl", "pdo_mysql", "tokenizer", "xml", "zip"]
    },
    {
      "name": "minimal",
      "extensions": []
    }
  ],
  "runtimes": [
    {
      "php": "8.3.23",
      "composer": "2.9.2",
      "extensions": ["curl", "dom", "gd", "intl", "mbstring", "mysqli", "openssl", "pdo_mysql", "tokenizer", "xml", "zip"],
      "url": "https://example.com/php-8.3.23-full.tar.gz",
      "sha256": "00000000000000000000000000000000000000000000000000000000000000ab"
    }
  ]
}"#;

    fn fixture_v1() -> Manifest {
        parse_manifest(FIXTURE_V1_JSON).unwrap()
    }

    fn fixture_v2() -> Manifest {
        parse_manifest(FIXTURE_V2_JSON).unwrap()
    }

    fn fixture_v21() -> Manifest {
        parse_manifest(FIXTURE_V21_JSON).unwrap()
    }

    #[test]
    fn parse_v1_normalizes_to_one_entry_per_php() {
        let m = fixture_v1();
        assert_eq!(m.runtimes.len(), 4);
        assert!(m.profiles.is_empty());
    }

    #[test]
    fn parse_v2_manifest_has_profiles_and_extensions() {
        let m = fixture_v2();
        assert_eq!(m.runtimes.len(), 1);
        assert_eq!(m.profiles.len(), 3);
        let entry = m.find("8.3.23").unwrap();
        assert!(entry.extension_catalog().contains(&"tokenizer".to_string()));
    }

    #[test]
    fn v1_wordpress_entry_keeps_its_explicit_extension_catalog() {
        let m = fixture_v1();
        let entry = m.find("8.2.0").unwrap();
        assert!(entry.extension_catalog().is_empty());
    }

    #[test]
    fn resolve_profile_template_prefers_manifest_preset() {
        let m = fixture_v2();
        let resolved = m.resolve_profile_template("wordpress").unwrap();
        assert_eq!(resolved.name, "wordpress");
    }

    #[test]
    fn parse_invalid_json_returns_error() {
        assert!(parse_manifest("not valid json").is_err());
    }

    #[test]
    fn parse_manifest_missing_required_fields_returns_error() {
        let json = r#"{
  "runtimes": [
    {
      "php": "8.1.0",
      "composer": "2.6.0",
      "sha256": "0000000000000000000000000000000000000000000000000000000000000001"
    }
  ]
}"#;
        assert!(parse_manifest(json).is_err());
    }

    #[test]
    fn parse_manifest_empty_fields_returns_error() {
        let json = r#"{
  "runtimes": [
    {
      "php": "",
      "composer": "2.6.0",
      "url": "https://example.com/php-8.1.0.tar.gz",
      "sha256": "0000000000000000000000000000000000000000000000000000000000000001"
    }
  ]
}"#;
        assert!(parse_manifest(json).is_err());
    }

    #[test]
    fn parse_manifest_invalid_php_version_returns_error() {
        let json = r#"{
  "runtimes": [
    {
      "php": "8-point-what",
      "composer": "2.6.0",
      "url": "https://example.com/php-8.1.0.tar.gz",
      "sha256": "0000000000000000000000000000000000000000000000000000000000000001"
    }
  ]
}"#;
        assert!(parse_manifest(json).is_err());
    }

    #[test]
    fn parse_empty_runtimes_is_ok() {
        let json = r#"{"runtimes": []}"#;
        let m = parse_manifest(json).unwrap();
        assert!(m.runtimes.is_empty());
        assert!(m.profiles.is_empty());
    }

    #[test]
    fn find_returns_correct_v2_entry() {
        let m = fixture_v2();
        let entry = m.find("8.3.23").unwrap();
        assert_eq!(entry.composer, "2.9.2");
        assert_eq!(entry.url, "https://example.com/php-8.3.23-full.tar.gz");
    }

    #[test]
    fn find_returns_none_for_missing_version() {
        assert!(fixture_v2().find("9.0.0").is_none());
    }

    #[test]
    fn latest_patch_returns_highest() {
        let m = fixture_v1();
        let entry = m.latest_patch(8, 3).unwrap();
        assert_eq!(entry.php, "8.3.23");
    }

    #[test]
    fn available_versions_returns_sorted() {
        let m = fixture_v1();
        assert_eq!(
            m.available_versions(),
            vec!["8.1.0", "8.2.0", "8.3.12", "8.3.23"]
        );
    }

    #[test]
    fn default_manifest_url_points_at_phpvm_runtimes_catalog() {
        assert!(DEFAULT_MANIFEST_URL.starts_with("https://"));
        assert!(DEFAULT_MANIFEST_URL.contains("phpvm-runtimes"));
        assert!(DEFAULT_MANIFEST_URL.ends_with("/manifest.json"));
    }

    #[test]
    fn parse_v21_manifest_with_artifacts() {
        let m = fixture_v21();
        assert_eq!(m.runtimes.len(), 1);
        let entry = m.find("8.3.31").unwrap();
        assert!(entry.url.is_empty());
        assert!(entry.sha256.is_empty());
        assert_eq!(entry.artifacts.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn download_for_host_selects_current_target() {
        let manifest = fixture_v21();
        let entry = manifest.find("8.3.31").unwrap();
        let target = host_target().unwrap();
        let artifact = entry.download_for_host().unwrap();
        assert!(artifact.url.contains(&target));
        assert_eq!(artifact.sha256.len(), 64);
    }

    #[test]
    fn download_for_host_fails_when_target_missing() {
        let mut entry = fixture_v21().find("8.3.31").unwrap().clone();
        let target = host_target().unwrap();
        entry.artifacts.as_mut().unwrap().remove(&target);
        let err = entry.download_for_host().unwrap_err().to_string();
        assert!(err.contains(&target));
        assert!(err.contains("not available on this platform"));
    }

    #[test]
    fn parse_v21_without_top_level_url_is_ok() {
        assert!(parse_manifest(FIXTURE_V21_JSON).is_ok());
    }

    #[test]
    fn parse_pure_static_v21_no_runtime_type_key_defaults_to_static() {
        let m = parse_manifest(FIXTURE_PURE_STATIC_V21_JSON).unwrap();
        let entry = m.find("8.4.5").unwrap();
        assert!(
            entry.runtime_type == RuntimeType::Static,
            "absent runtime_type must default to Static"
        );
        let catalog = entry.extension_catalog();
        assert!(catalog.contains(&"pdo_mysql".to_string()));
        assert!(catalog.contains(&"tokenizer".to_string()));
        assert_eq!(catalog.len(), 6);
    }

    #[test]
    fn host_target_returns_supported_triple() {
        let target = host_target().unwrap();
        assert!(
            target == "x86_64-unknown-linux-gnu"
                || target == "aarch64-unknown-linux-gnu"
                || target == "x86_64-apple-darwin"
                || target == "aarch64-apple-darwin"
        );
    }

    #[test]
    fn parse_companion_runtimes_manifest_if_present() {
        let path = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../phpvm-runtimes/manifest.json");
        if !path.exists() {
            return;
        }
        let json = fs::read_to_string(&path).unwrap();
        let manifest = parse_manifest(&json).unwrap();
        assert_eq!(manifest.runtimes.len(), 4);
        let entry = manifest.find("8.3.31").unwrap();
        let artifact = entry.download_for_host().unwrap();
        assert!(artifact
            .url
            .contains("php-8.3.31-x86_64-unknown-linux-gnu.tar.gz"));
    }
}
