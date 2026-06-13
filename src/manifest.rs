use std::collections::BTreeMap;
use std::fs;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config;
use crate::profile::{self, ProfileTemplate};

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

    /// Extensions compiled into the full binary (manifest v2).
    #[serde(default)]
    pub extensions: Vec<String>,

    /// Download URL for the runtime archive
    pub url: String,

    /// SHA-256 checksum of the archive
    pub sha256: String,
}

/// The full manifest: profile presets and available runtimes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Named extension presets for starter ini seeding (not user config).
    #[serde(default)]
    pub profiles: Vec<ProfileTemplate>,

    pub runtimes: Vec<ManifestEntry>,
}

/// Default manifest URL.
const DEFAULT_MANIFEST_URL: &str = "https://phpvm.com/manifest.json";

// ── Manifest methods ────────────────────────────────────────────────────

impl ManifestEntry {
    /// Extensions available in the installed binary for this runtime.
    pub fn extension_catalog(&self) -> Vec<String> {
        if !self.extensions.is_empty() {
            return self.extensions.clone();
        }

        // v1 fallback: union builtins when the manifest has not published catalogs yet.
        self.profile
            .as_deref()
            .and_then(profile::builtin_template)
            .map(|p| p.extensions)
            .unwrap_or_default()
    }
}

impl Manifest {
    /// Find an entry by exact PHP version string.
    pub fn find(&self, php_version: &str) -> Option<&ManifestEntry> {
        self.runtimes.iter().find(|e| e.php == php_version)
    }

    /// Resolve a manifest profile template by name, falling back to built-ins.
    pub fn resolve_profile_template(&self, name: &str) -> Option<ProfileTemplate> {
        if let Some(p) = self.profiles.iter().find(|p| p.name == name) {
            return Some(p.clone());
        }
        profile::builtin_template(name)
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
    if manifest.profiles.is_empty() {
        manifest.profiles = profile::builtin_templates();
    }

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
        for entry in &entries {
            urls.insert(entry.url.as_str());
            checksums.insert(entry.sha256.as_str());
        }
        if urls.len() > 1 || checksums.len() > 1 {
            anyhow::bail!(
                "Manifest has conflicting runtime artifacts for PHP {}. \
                 Publish one full binary per version (manifest v2).",
                php
            );
        }
    }

    let mut extensions: Vec<String> = Vec::new();
    for entry in &entries {
        if !entry.extensions.is_empty() {
            for ext in &entry.extensions {
                if !extensions.contains(ext) {
                    extensions.push(ext.clone());
                }
            }
        } else if let Some(profile_name) = entry.profile.as_deref() {
            if let Some(p) = profile::builtin_template(profile_name) {
                for ext in p.extensions {
                    if !extensions.contains(&ext) {
                        extensions.push(ext);
                    }
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
        extensions,
        url: base.url,
        sha256: base.sha256,
    })
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    for (i, entry) in manifest.runtimes.iter().enumerate() {
        if entry.php.is_empty() {
            anyhow::bail!("Manifest entry {} has an empty PHP version", i);
        }
        if entry.composer.is_empty() {
            anyhow::bail!("Manifest entry {} has an empty Composer version", i);
        }
        if entry.url.is_empty() {
            anyhow::bail!("Manifest entry {} has an empty URL", i);
        }
        if entry.sha256.is_empty() {
            anyhow::bail!("Manifest entry {} has an empty sha256", i);
        }
        let normalized = entry.sha256.trim();
        if normalized.len() != 64 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!(
                "Manifest entry {} has invalid sha256 (expected 64 hex chars): '{}'",
                i,
                entry.sha256
            );
        }
        if !entry.url.starts_with("https://") {
            anyhow::bail!(
                "Manifest entry {} must use an https:// download URL, got '{}'",
                i,
                entry.url
            );
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

    #[test]
    fn parse_v1_normalizes_to_one_entry_per_php() {
        let m = fixture_v1();
        assert_eq!(m.runtimes.len(), 4);
        assert_eq!(m.profiles.len(), 3);
    }

    #[test]
    fn parse_v2_manifest_has_profiles_and_extensions() {
        let m = fixture_v2();
        assert_eq!(m.runtimes.len(), 1);
        assert_eq!(m.profiles.len(), 3);
        let entry = m.find("8.3.23").unwrap();
        assert!(entry.extensions.contains(&"tokenizer".to_string()));
    }

    #[test]
    fn v1_wordpress_entry_gains_extension_catalog() {
        let m = fixture_v1();
        let entry = m.find("8.2.0").unwrap();
        assert!(entry.extensions.contains(&"mysqli".to_string()));
    }

    #[test]
    fn resolve_profile_template_prefers_manifest_preset() {
        let m = fixture_v2();
        let resolved = m.resolve_profile_template("wordpress").unwrap();
        assert_eq!(resolved.extensions.len(), 10);
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
        assert_eq!(m.profiles.len(), 3);
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
    fn default_manifest_url_is_https() {
        assert!(DEFAULT_MANIFEST_URL.starts_with("https://"));
    }
}
