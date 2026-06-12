use std::fs;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::config;

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
const DEFAULT_MANIFEST_URL: &str = "https://phpvm.com/manifest.json";

// ── Manifest methods ────────────────────────────────────────────────────

impl Manifest {
    /// Find an entry by exact PHP version string.
    pub fn find(&self, php_version: &str) -> Option<&ManifestEntry> {
        self.runtimes.iter().find(|e| e.php == php_version)
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
    #[allow(dead_code)]
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
///
/// Uses `reqwest::blocking` to perform a synchronous HTTP GET and parse the
/// JSON body.  The caller is responsible for caching decisions.
pub fn fetch(url: &str) -> Result<Manifest> {
    let resp = reqwest::blocking::get(url)
        .with_context(|| format!("Failed to fetch manifest from {}", url))?
        .error_for_status()
        .with_context(|| format!("Manifest fetch returned error status from {}", url))?;

    let body = resp
        .text()
        .with_context(|| "Failed to read manifest response body")?;

    parse_manifest(&body)
}

/// Fetch the manifest with local caching.
///
/// If a cached manifest exists at `cache_dir / manifest.json` and is less than
/// one hour old (by file modification time), the cached copy is used.
/// Otherwise the URL is fetched, the result parsed, and the JSON is written
/// into `cache_dir / manifest.json` for future use.
pub fn fetch_cached(url: &str, cache_dir: &Utf8PathBuf) -> Result<Manifest> {
    let cache_path = cache_dir.join("manifest.json");
    let max_age = Duration::from_secs(3600); // 1 hour

    // Try to use the cached copy if it exists and is fresh.
    if let Ok(meta) = fs::metadata(&cache_path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                if elapsed < max_age {
                    match parse_file(&cache_path) {
                        Ok(m) => return Ok(m),
                        Err(e) => {
                            // Cache file corrupt?  Warn and re-fetch.
                            eprintln!("Warning: cached manifest is invalid ({}) — re-fetching", e);
                        }
                    }
                }
            }
        }
    }

    // Fetch from network.
    let manifest = fetch(url)?;

    // Persist to cache directory.
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("Failed to create cache directory {}", cache_dir))?;

    let json = serde_json::to_string_pretty(&manifest)
        .with_context(|| "Failed to serialize manifest for caching")?;

    fs::write(&cache_path, json)
        .with_context(|| format!("Failed to write cached manifest to {}", cache_path))?;

    Ok(manifest)
}

/// Fetch the manifest from the default URL, using the default cache directory.
///
/// This is the main entry-point for most callers.
pub fn fetch_default() -> Result<Manifest> {
    let cache_dir = config::cache_dir()?;
    fetch_cached(DEFAULT_MANIFEST_URL, &cache_dir)
}

/// Parse a JSON string into a `Manifest`.
///
/// This is a pure function with no side-effects — ideal for testing.
pub fn parse_manifest(json: &str) -> Result<Manifest> {
    let manifest: Manifest =
        serde_json::from_str(json).with_context(|| "Failed to parse manifest JSON")?;

    // Validate that every entry has the required fields (serde already
    // enforces this for missing fields, but a field could be empty).
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
        // Validate the PHP version looks like a semver string.
        semver::Version::parse(&entry.php).with_context(|| {
            format!(
                "Manifest entry {} has invalid PHP version: '{}'",
                i, entry.php
            )
        })?;
    }

    Ok(manifest)
}

/// Fetch a single manifest entry for a specific PHP version.
///
/// This is a convenience wrapper around `fetch_default` + `Manifest::find`.
#[allow(dead_code)]
pub fn fetch_entry(version: &str) -> Result<ManifestEntry> {
    let manifest = fetch_default()?;
    manifest
        .find(version)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("PHP version {} not found in manifest", version))
}

// ── Internal helpers ────────────────────────────────────────────────────

/// Parse a cached manifest file.
fn parse_file(path: &camino::Utf8Path) -> Result<Manifest> {
    let json = fs::read_to_string(path)
        .with_context(|| format!("Failed to read cached manifest from {}", path))?;
    parse_manifest(&json)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const FIXTURE_JSON: &str = r#"{
  "runtimes": [
    {
      "php": "8.1.0",
      "composer": "2.6.0",
      "profile": "minimal",
      "url": "https://example.com/php-8.1.0.tar.gz",
      "sha256": "abc123"
    },
    {
      "php": "8.2.0",
      "composer": "2.8.0",
      "profile": "wordpress",
      "url": "https://example.com/php-8.2.0.tar.gz",
      "sha256": "def456"
    },
    {
      "php": "8.3.12",
      "composer": "2.9.2",
      "profile": "laravel",
      "url": "https://example.com/php-8.3.12.tar.gz",
      "sha256": "ghi789"
    },
    {
      "php": "8.3.23",
      "composer": "2.9.2",
      "profile": "wordpress",
      "url": "https://example.com/php-8.3.23.tar.gz",
      "sha256": "jkl012"
    }
  ]
}"#;

    fn fixture() -> Manifest {
        parse_manifest(FIXTURE_JSON).unwrap()
    }

    // ── parse_manifest ────────────────────────────────────────────────

    #[test]
    fn parse_valid_manifest() {
        let m = parse_manifest(FIXTURE_JSON).unwrap();
        assert_eq!(m.runtimes.len(), 4);
    }

    #[test]
    fn parse_manifest_with_multiple_entries() {
        let m = fixture();
        let versions: Vec<&str> = m.runtimes.iter().map(|e| e.php.as_str()).collect();
        assert!(versions.contains(&"8.1.0"));
        assert!(versions.contains(&"8.2.0"));
        assert!(versions.contains(&"8.3.12"));
        assert!(versions.contains(&"8.3.23"));
    }

    #[test]
    fn parse_invalid_json_returns_error() {
        let result = parse_manifest("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn parse_manifest_missing_required_fields_returns_error() {
        let json = r#"{
  "runtimes": [
    {
      "php": "8.1.0",
      "composer": "2.6.0",
      "profile": "minimal",
      "sha256": "abc123"
    }
  ]
}"#;
        // Missing `url` field — serde should reject it.
        let result = parse_manifest(json);
        assert!(result.is_err());
    }

    #[test]
    fn parse_manifest_empty_fields_returns_error() {
        let json = r#"{
  "runtimes": [
    {
      "php": "",
      "composer": "2.6.0",
      "profile": "minimal",
      "url": "https://example.com/php-8.1.0.tar.gz",
      "sha256": "abc123"
    }
  ]
}"#;
        let result = parse_manifest(json);
        assert!(result.is_err());
    }

    #[test]
    fn parse_manifest_invalid_php_version_returns_error() {
        let json = r#"{
  "runtimes": [
    {
      "php": "8-point-what",
      "composer": "2.6.0",
      "profile": "minimal",
      "url": "https://example.com/php-8.1.0.tar.gz",
      "sha256": "abc123"
    }
  ]
}"#;
        let result = parse_manifest(json);
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_runtimes_is_ok() {
        let json = r#"{"runtimes": []}"#;
        let m = parse_manifest(json).unwrap();
        assert!(m.runtimes.is_empty());
    }

    #[test]
    fn parse_missing_runtimes_field_returns_error() {
        let json = r#"{}"#;
        let result = parse_manifest(json);
        assert!(result.is_err());
    }

    // ── Manifest::find ────────────────────────────────────────────────

    #[test]
    fn find_returns_correct_entry() {
        let m = fixture();
        let entry = m.find("8.3.12").unwrap();
        assert_eq!(entry.php, "8.3.12");
        assert_eq!(entry.composer, "2.9.2");
        assert_eq!(entry.profile, "laravel");
        assert_eq!(entry.url, "https://example.com/php-8.3.12.tar.gz");
        assert_eq!(entry.sha256, "ghi789");
    }

    #[test]
    fn find_returns_none_for_missing_version() {
        let m = fixture();
        assert!(m.find("9.0.0").is_none());
    }

    #[test]
    fn find_returns_none_for_partial_version() {
        let m = fixture();
        // "8.3" is not an exact version match.
        assert!(m.find("8.3").is_none());
    }

    // ── Manifest::latest_patch ────────────────────────────────────────

    #[test]
    fn latest_patch_returns_highest() {
        let m = fixture();
        let entry = m.latest_patch(8, 3).unwrap();
        assert_eq!(entry.php, "8.3.23");
    }

    #[test]
    fn latest_patch_single_entry() {
        let m = fixture();
        let entry = m.latest_patch(8, 1).unwrap();
        assert_eq!(entry.php, "8.1.0");
    }

    #[test]
    fn latest_patch_missing_major_minor() {
        let m = fixture();
        assert!(m.latest_patch(9, 0).is_none());
    }

    // ── Manifest::min_patch ───────────────────────────────────────────

    #[test]
    fn min_patch_returns_lowest() {
        let m = fixture();
        let entry = m.min_patch(8, 3).unwrap();
        assert_eq!(entry.php, "8.3.12");
    }

    #[test]
    fn min_patch_single_entry() {
        let m = fixture();
        let entry = m.min_patch(8, 1).unwrap();
        assert_eq!(entry.php, "8.1.0");
    }

    #[test]
    fn min_patch_missing_major_minor() {
        let m = fixture();
        assert!(m.min_patch(9, 0).is_none());
    }

    // ── Manifest::available_versions ──────────────────────────────────

    #[test]
    fn available_versions_returns_sorted() {
        let m = fixture();
        let versions = m.available_versions();
        assert_eq!(versions, vec!["8.1.0", "8.2.0", "8.3.12", "8.3.23"]);
    }

    #[test]
    fn available_versions_empty_manifest() {
        let m = Manifest {
            runtimes: Vec::new(),
        };
        assert!(m.available_versions().is_empty());
    }

    // ── Structure ─────────────────────────────────────────────────────

    #[test]
    fn manifest_entry_fields_are_accessible() {
        let entry = ManifestEntry {
            php: "8.3.23".into(),
            composer: "2.9.2".into(),
            profile: "wordpress".into(),
            url: "https://example.com/php-8.3.23.tar.gz".into(),
            sha256: "jkl012".into(),
        };
        assert_eq!(entry.php, "8.3.23");
        assert_eq!(entry.composer, "2.9.2");
        assert_eq!(entry.profile, "wordpress");
        assert!(entry.url.starts_with("https://"));
        assert!(!entry.sha256.is_empty());
    }

    #[test]
    fn default_manifest_url_is_https() {
        assert!(DEFAULT_MANIFEST_URL.starts_with("https://"));
    }
}
