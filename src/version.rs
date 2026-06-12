use std::fmt;

use anyhow::{bail, Result};

use crate::config;
use crate::output;

// ---------------------------------------------------------------------------
// VersionSpecifier — how the user describes which version they want
// ---------------------------------------------------------------------------

/// Ways a user can specify a PHP version.
///
/// Supported formats:
///   - `8.3.12`        → Exact { major: 8, minor: 3, patch: 12 }
///   - `8.3`           → LatestMinor { major: 8, minor: 3 }
///   - `8.3.latest`    → LatestMinor { major: 8, minor: 3 }
///   - `8.3.min`       → MinMinor { major: 8, minor: 3 }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionSpecifier {
    /// A fully-qualified version: MAJOR.MINOR.PATCH
    Exact { major: u32, minor: u32, patch: u32 },
    /// The latest patch for a given major.minor series.
    LatestMinor { major: u32, minor: u32 },
    /// The earliest (minimum) patch for a given major.minor series.
    MinMinor { major: u32, minor: u32 },
}

// ---------------------------------------------------------------------------
// PhpVersion — a concrete, resolved version
// ---------------------------------------------------------------------------

/// A resolved PHP version with its numeric components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhpVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl fmt::Display for PhpVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PhpVersion {
    /// Parse a `MAJOR.MINOR.PATCH` string into a `PhpVersion`.
    pub fn parse(version: &str) -> Result<Self> {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            bail!(
                "Invalid version string '{}': expected MAJOR.MINOR.PATCH format",
                version
            );
        }
        let major = parse_u32(parts[0], "major", version)?;
        let minor = parse_u32(parts[1], "minor", version)?;
        let patch = parse_u32(parts[2], "patch", version)?;
        Ok(Self {
            major,
            minor,
            patch,
        })
    }

    /// Convert to the `MAJOR.MINOR.PATCH` string representation.
    #[allow(dead_code)]
    pub fn to_version_string(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ---------------------------------------------------------------------------
// parse — turn user input into a VersionSpecifier
// ---------------------------------------------------------------------------

/// Parse a user-supplied version specifier string into a `VersionSpecifier`.
///
/// # Supported inputs
/// | Input        | Result                              |
/// |-------------|-------------------------------------|
/// | `8.3.12`    | `Exact { 8, 3, 12 }`               |
/// | `8.3`       | `LatestMinor { 8, 3 }`             |
/// | `8.3.latest`| `LatestMinor { 8, 3 }`             |
/// | `8.3.min`   | `MinMinor { 8, 3 }`                |
///
/// # Errors
/// Returns an error if the input is malformed or contains non-numeric version
/// components.
pub fn parse(specifier: &str) -> Result<VersionSpecifier> {
    // Handle .latest suffix
    if let Some(stripped) = specifier.strip_suffix(".latest") {
        let (major, minor) = parse_major_minor(stripped, specifier)?;
        return Ok(VersionSpecifier::LatestMinor { major, minor });
    }

    // Handle .min suffix
    if let Some(stripped) = specifier.strip_suffix(".min") {
        let (major, minor) = parse_major_minor(stripped, specifier)?;
        return Ok(VersionSpecifier::MinMinor { major, minor });
    }

    // Split on dots and dispatch by count
    let parts: Vec<&str> = specifier.split('.').collect();
    match parts.len() {
        2 => {
            // Bare major.minor → treat as LatestMinor
            let major = parse_u32(parts[0], "major", specifier)?;
            let minor = parse_u32(parts[1], "minor", specifier)?;
            Ok(VersionSpecifier::LatestMinor { major, minor })
        }
        3 => {
            // Attempt exact version (all three must be numeric)
            let major = parse_u32(parts[0], "major", specifier)?;
            let minor = parse_u32(parts[1], "minor", specifier)?;
            let patch = parse_u32(parts[2], "patch", specifier)?;
            Ok(VersionSpecifier::Exact {
                major,
                minor,
                patch,
            })
        }
        _ => bail!(
            "Invalid version specifier '{}'. Expected 'MAJOR.MINOR', \
             'MAJOR.MINOR.PATCH', 'MAJOR.MINOR.latest', or 'MAJOR.MINOR.min'",
            specifier
        ),
    }
}

// ---------------------------------------------------------------------------
// resolve — map a VersionSpecifier to a concrete version string
// ---------------------------------------------------------------------------

/// Resolve a `VersionSpecifier` against a list of available version strings.
///
/// Available version strings must be in `MAJOR.MINOR.PATCH` format (e.g.
/// `"8.3.12"`). Versions that don't parse are silently skipped.
///
/// # Resolution rules
/// - `Exact` → verify the version exists in `available`; return it.
/// - `LatestMinor` → pick the **highest** patch for that `major.minor` series.
/// - `MinMinor` → pick the **lowest** patch for that `major.minor` series.
pub fn resolve(specifier: &VersionSpecifier, available: &[String]) -> Result<String> {
    match specifier {
        VersionSpecifier::Exact {
            major,
            minor,
            patch,
        } => {
            let target = format!("{}.{}.{}", major, minor, patch);
            if available.contains(&target) {
                Ok(target)
            } else {
                bail!("Version {} is not available", target)
            }
        }
        VersionSpecifier::LatestMinor { major, minor } => {
            let candidates = filter_matching(available, *major, *minor);
            if candidates.is_empty() {
                bail!("No available versions found for {}.{}", major, minor);
            }
            let selected = candidates
                .into_iter()
                .max_by_key(|v| v.patch)
                .expect("candidates is non-empty");
            Ok(selected.to_version_string())
        }
        VersionSpecifier::MinMinor { major, minor } => {
            let candidates = filter_matching(available, *major, *minor);
            if candidates.is_empty() {
                bail!("No available versions found for {}.{}", major, minor);
            }
            let selected = candidates
                .into_iter()
                .min_by_key(|v| v.patch)
                .expect("candidates is non-empty");
            Ok(selected.to_version_string())
        }
    }
}

// ---------------------------------------------------------------------------
// resolve_specifier — parse + resolve in one call
// ---------------------------------------------------------------------------

/// Convenience function: parse a specifier string and resolve it against the
/// given list of available version strings.
pub fn resolve_specifier(specifier: &str, available: &[String]) -> Result<String> {
    let spec = parse(specifier)?;
    resolve(&spec, available)
}

// ---------------------------------------------------------------------------
// list_installed — show the user what they have locally
// ---------------------------------------------------------------------------

/// List all installed PHP runtimes.
pub fn list_installed() -> Result<()> {
    let runtimes_dir = config::runtimes_dir()?;

    if !runtimes_dir.exists() {
        output::info("No runtimes installed.");
        return Ok(());
    }

    // Collect directory entries; parse as PhpVersion for correct numeric
    // ordering (so "8.3.10" sorts after "8.3.9", not before).
    let mut versions: Vec<PhpVersion> = std::fs::read_dir(&runtimes_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| PhpVersion::parse(&name).ok())
        .collect();

    if versions.is_empty() {
        output::info("No runtimes installed.");
    } else {
        versions.sort();
        output::info("Installed runtimes:");
        for v in &versions {
            println!("  {}", v);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// private helpers
// ---------------------------------------------------------------------------

/// Parse a "major.minor" string (exactly two dot-separated numbers).
fn parse_major_minor(input: &str, full_spec: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = input.split('.').collect();
    if parts.len() != 2 {
        bail!(
            "Invalid version specifier '{}': expected MAJOR.MINOR before suffix",
            full_spec
        );
    }
    let major = parse_u32(parts[0], "major", full_spec)?;
    let minor = parse_u32(parts[1], "minor", full_spec)?;
    Ok((major, minor))
}

/// Parse a single numeric component, wrapping the error with context.
fn parse_u32(s: &str, field: &str, full_spec: &str) -> Result<u32> {
    s.parse::<u32>().map_err(|_| {
        anyhow::anyhow!(
            "Invalid {} version in '{}': '{}' is not a number",
            field,
            full_spec,
            s
        )
    })
}

/// Filter available version strings to those matching a given major.minor.
/// Returns parsed `PhpVersion` values (malformed entries are skipped).
fn filter_matching(available: &[String], major: u32, minor: u32) -> Vec<PhpVersion> {
    available
        .iter()
        .filter_map(|v| {
            let parsed = PhpVersion::parse(v).ok()?;
            if parsed.major == major && parsed.minor == minor {
                Some(parsed)
            } else {
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // parse
    // -----------------------------------------------------------------------

    #[test]
    fn parse_exact() {
        let spec = parse("8.3.12").unwrap();
        assert_eq!(
            spec,
            VersionSpecifier::Exact {
                major: 8,
                minor: 3,
                patch: 12
            }
        );
    }

    #[test]
    fn parse_bare_major_minor() {
        let spec = parse("8.3").unwrap();
        assert_eq!(spec, VersionSpecifier::LatestMinor { major: 8, minor: 3 });
    }

    #[test]
    fn parse_latest_suffix() {
        let spec = parse("8.3.latest").unwrap();
        assert_eq!(spec, VersionSpecifier::LatestMinor { major: 8, minor: 3 });
    }

    #[test]
    fn parse_min_suffix() {
        let spec = parse("8.3.min").unwrap();
        assert_eq!(spec, VersionSpecifier::MinMinor { major: 8, minor: 3 });
    }

    #[test]
    fn parse_large_numbers() {
        let spec = parse("99.88.77").unwrap();
        assert_eq!(
            spec,
            VersionSpecifier::Exact {
                major: 99,
                minor: 88,
                patch: 77,
            }
        );
    }

    #[test]
    fn parse_zero_patch() {
        let spec = parse("8.1.0").unwrap();
        assert_eq!(
            spec,
            VersionSpecifier::Exact {
                major: 8,
                minor: 1,
                patch: 0
            }
        );
    }

    #[test]
    fn parse_bare_zero_minor() {
        let spec = parse("8.0").unwrap();
        assert_eq!(spec, VersionSpecifier::LatestMinor { major: 8, minor: 0 });
    }

    // -----------------------------------------------------------------------
    // parse errors
    // -----------------------------------------------------------------------

    #[test]
    fn parse_empty_string() {
        assert!(parse("").is_err());
    }

    #[test]
    fn parse_garbage() {
        assert!(parse("not-a-version").is_err());
    }

    #[test]
    fn parse_too_many_parts() {
        assert!(parse("8.3.12.1").is_err());
    }

    #[test]
    fn parse_only_major() {
        assert!(parse("8").is_err());
    }

    #[test]
    fn parse_letters_in_major() {
        assert!(parse("abc.3.12").is_err());
    }

    #[test]
    fn parse_letters_in_minor() {
        assert!(parse("8.xyz.12").is_err());
    }

    #[test]
    fn parse_letters_in_patch() {
        assert!(parse("8.3.abc").is_err());
    }

    #[test]
    fn parse_bad_latest_suffix() {
        // "8.latest" is missing the minor component
        assert!(parse("8.latest").is_err());
    }

    #[test]
    fn parse_bad_min_suffix() {
        assert!(parse("8.min").is_err());
    }

    #[test]
    fn parse_non_numeric_with_latest() {
        assert!(parse("abc.xyz.latest").is_err());
    }

    #[test]
    fn parse_non_numeric_with_min() {
        assert!(parse("abc.xyz.min").is_err());
    }

    // -----------------------------------------------------------------------
    // resolve — Exact
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_exact_found() {
        let available = vers(&["8.3.12", "8.3.11", "8.2.0"]);
        let spec = VersionSpecifier::Exact {
            major: 8,
            minor: 3,
            patch: 12,
        };
        let result = resolve(&spec, &available).unwrap();
        assert_eq!(result, "8.3.12");
    }

    #[test]
    fn resolve_exact_not_found() {
        let available = vers(&["8.3.11", "8.2.0"]);
        let spec = VersionSpecifier::Exact {
            major: 8,
            minor: 3,
            patch: 12,
        };
        let result = resolve(&spec, &available);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not available"));
    }

    // -----------------------------------------------------------------------
    // resolve — LatestMinor
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_latest_picks_highest_patch() {
        let available = vers(&["8.3.12", "8.3.1", "8.3.9", "8.2.99"]);
        let spec = VersionSpecifier::LatestMinor { major: 8, minor: 3 };
        let result = resolve(&spec, &available).unwrap();
        assert_eq!(result, "8.3.12");
    }

    #[test]
    fn resolve_latest_single_candidate() {
        let available = vers(&["8.3.5"]);
        let spec = VersionSpecifier::LatestMinor { major: 8, minor: 3 };
        let result = resolve(&spec, &available).unwrap();
        assert_eq!(result, "8.3.5");
    }

    #[test]
    fn resolve_latest_no_matching_major_minor() {
        let available = vers(&["8.2.0", "8.4.1"]);
        let spec = VersionSpecifier::LatestMinor { major: 8, minor: 3 };
        let result = resolve(&spec, &available);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No available versions found for 8.3"));
    }

    // -----------------------------------------------------------------------
    // resolve — MinMinor
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_min_picks_lowest_patch() {
        let available = vers(&["8.3.12", "8.3.1", "8.3.9", "8.2.99"]);
        let spec = VersionSpecifier::MinMinor { major: 8, minor: 3 };
        let result = resolve(&spec, &available).unwrap();
        assert_eq!(result, "8.3.1");
    }

    #[test]
    fn resolve_min_single_candidate() {
        let available = vers(&["8.3.5"]);
        let spec = VersionSpecifier::MinMinor { major: 8, minor: 3 };
        let result = resolve(&spec, &available).unwrap();
        assert_eq!(result, "8.3.5");
    }

    #[test]
    fn resolve_min_no_matching_major_minor() {
        let available = vers(&["8.2.0", "8.4.1"]);
        let spec = VersionSpecifier::MinMinor { major: 8, minor: 3 };
        let result = resolve(&spec, &available);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No available versions found for 8.3"));
    }

    // -----------------------------------------------------------------------
    // resolve — edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_with_empty_available() {
        let available: Vec<String> = vec![];
        let spec = VersionSpecifier::LatestMinor { major: 8, minor: 3 };
        let result = resolve(&spec, &available);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_skips_malformed_available() {
        // "8.3.abc" cannot be parsed as a version and should be skipped
        let available = vers(&["8.3.abc", "8.3.5", "8.3.x"]);
        let spec = VersionSpecifier::LatestMinor { major: 8, minor: 3 };
        let result = resolve(&spec, &available).unwrap();
        assert_eq!(result, "8.3.5");
    }

    #[test]
    fn resolve_min_among_many() {
        let mut avail = vec![];
        for p in 0..50 {
            avail.push(format!("8.3.{}", p));
        }
        let spec = VersionSpecifier::MinMinor { major: 8, minor: 3 };
        let result = resolve(&spec, &avail).unwrap();
        assert_eq!(result, "8.3.0");
    }

    #[test]
    fn resolve_latest_among_many() {
        let mut avail = vec![];
        for p in 0..50 {
            avail.push(format!("8.3.{}", p));
        }
        let spec = VersionSpecifier::LatestMinor { major: 8, minor: 3 };
        let result = resolve(&spec, &avail).unwrap();
        assert_eq!(result, "8.3.49");
    }

    // -----------------------------------------------------------------------
    // resolve_specifier convenience
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_specifier_convenience() {
        let available = vers(&["8.3.12", "8.3.1", "8.2.0"]);
        let result = resolve_specifier("8.3.latest", &available).unwrap();
        assert_eq!(result, "8.3.12");

        let result = resolve_specifier("8.3.min", &available).unwrap();
        assert_eq!(result, "8.3.1");

        let result = resolve_specifier("8.2.0", &available).unwrap();
        assert_eq!(result, "8.2.0");
    }

    #[test]
    fn resolve_specifier_parse_error() {
        let available = vers(&["8.3.12"]);
        let result = resolve_specifier("garbage", &available);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // PhpVersion
    // -----------------------------------------------------------------------

    #[test]
    fn phpversion_parse_valid() {
        let v = PhpVersion::parse("8.3.12").unwrap();
        assert_eq!(v.major, 8);
        assert_eq!(v.minor, 3);
        assert_eq!(v.patch, 12);
    }

    #[test]
    fn phpversion_parse_invalid() {
        assert!(PhpVersion::parse("8.3").is_err());
        assert!(PhpVersion::parse("not.ver.sion").is_err());
        assert!(PhpVersion::parse("").is_err());
    }

    #[test]
    fn phpversion_display() {
        let v = PhpVersion {
            major: 7,
            minor: 4,
            patch: 33,
        };
        assert_eq!(v.to_version_string(), "7.4.33");
        assert_eq!(format!("{}", v), "7.4.33");
    }

    #[test]
    fn phpversion_ordering() {
        let mut versions = [
            PhpVersion {
                major: 8,
                minor: 3,
                patch: 10,
            },
            PhpVersion {
                major: 8,
                minor: 3,
                patch: 2,
            },
            PhpVersion {
                major: 8,
                minor: 1,
                patch: 99,
            },
            PhpVersion {
                major: 7,
                minor: 4,
                patch: 33,
            },
        ];
        versions.sort();
        assert_eq!(versions[0].to_version_string(), "7.4.33");
        assert_eq!(versions[1].to_version_string(), "8.1.99");
        assert_eq!(versions[2].to_version_string(), "8.3.2");
        assert_eq!(versions[3].to_version_string(), "8.3.10");
    }

    #[test]
    fn phpversion_copy_trait() {
        let v1 = PhpVersion {
            major: 8,
            minor: 3,
            patch: 12,
        };
        let v2 = v1; // Copy
        assert_eq!(v1, v2);
    }

    // -----------------------------------------------------------------------
    // helpers
    // -----------------------------------------------------------------------

    /// Convert string slices to owned Strings for test convenience.
    fn vers(slice: &[&str]) -> Vec<String> {
        slice.iter().map(|s| s.to_string()).collect()
    }
}
