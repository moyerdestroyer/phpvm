use std::fmt;

use anyhow::{bail, Result};
use serde::Deserialize;

use crate::config;
use crate::manifest;
use crate::output;
use crate::profile;
use crate::runner;

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
///   - `latest`        → Latest (highest version in the available list)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionSpecifier {
    /// A fully-qualified version: MAJOR.MINOR.PATCH
    Exact { major: u32, minor: u32, patch: u32 },
    /// The latest patch for a given major.minor series.
    LatestMinor { major: u32, minor: u32 },
    /// The earliest (minimum) patch for a given major.minor series.
    MinMinor { major: u32, minor: u32 },
    /// The single highest version present in the list of available/installed versions.
    Latest,
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
    // Bare "latest" (case-insensitive) means the single highest in the list.
    if specifier.eq_ignore_ascii_case("latest") {
        return Ok(VersionSpecifier::Latest);
    }

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
             'MAJOR.MINOR.PATCH', 'MAJOR.MINOR.latest', 'MAJOR.MINOR.min', or 'latest'",
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
                .ok_or_else(|| {
                    anyhow::anyhow!("No candidates after filter (should be impossible)")
                })?;
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
                .ok_or_else(|| {
                    anyhow::anyhow!("No candidates after filter (should be impossible)")
                })?;
            Ok(selected.to_version_string())
        }
        VersionSpecifier::Latest => {
            if available.is_empty() {
                bail!("No versions available");
            }
            let candidates: Vec<PhpVersion> = available
                .iter()
                .filter_map(|v| PhpVersion::parse(v).ok())
                .collect();
            let selected = candidates
                .into_iter()
                .max()
                .ok_or_else(|| anyhow::anyhow!("No parseable candidates"))?;
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
///
/// The currently "active" runtime (resolved from project config's php_constraint,
/// or the highest installed version if no constraint) is marked with a leading `*`.
pub fn list_installed() -> Result<()> {
    let raw = runner::installed_versions()?;
    let mut versions: Vec<PhpVersion> = raw
        .iter()
        .filter_map(|name| PhpVersion::parse(name).ok())
        .collect();

    if versions.is_empty() {
        output::info("No runtimes installed.");
        return Ok(());
    }

    versions.sort();
    let installed_strs: Vec<String> = versions.iter().map(|v| v.to_string()).collect();
    let current = compute_current_version(&installed_strs);

    for v in &versions {
        let s = v.to_string();
        if current.as_deref() == Some(s.as_str()) {
            println!("* {}", s);
        } else {
            output::list_item(&s);
        }
    }

    Ok(())
}

/// Build the shell export snippet for activating a specific resolved runtime.
/// Used by both `activate` (for `use`) and the `env` command.
///
/// Note: composer globals (COMPOSER_HOME) are shared across all patch versions
/// of the same minor series (all 8.3.x share one `composer-homes/8.3` bucket).
fn build_activation_snippet(resolved: &str, runtime_path: &camino::Utf8Path) -> String {
    let bin_dir = runtime_path.join("bin");
    let composer_home = composer_home_for(resolved).expect("valid resolved version");
    let global_bin = composer_home.join("vendor").join("bin");

    let separator = if cfg!(windows) { ";" } else { ":" };
    let path_value = format!(
        "{}{}{}{}{}",
        bin_dir, separator, global_bin, separator, "$PATH"
    );

    format!(
        r#"export PHPVM_VERSION="{}"
export COMPOSER_HOME="{}"
export PATH="{}"
"#,
        resolved, composer_home, path_value
    )
}

/// Activate a runtime for the current shell by printing an eval-able snippet.
///
/// Intended usage:
///   eval "$(phpvm use 8.3)"
///
/// This sets:
/// - PHPVM_VERSION (so `phpvm ls` can mark it with *)
/// - COMPOSER_HOME (isolates `composer global` packages per runtime)
/// - PATH (so bare `php`, `composer`, and global tools from that runtime work)
///
/// The per-runtime globals live under `~/.phpvm/runtimes/<resolved>/composer-home/`.
pub fn activate(spec: &str) -> Result<()> {
    let resolved = match runner::resolve_version(spec) {
        Ok(r) => r,
        Err(_) => {
            // Give a clear, actionable message for the common case.
            anyhow::bail!(
                "PHP runtime matching '{}' is not installed. \
                 Run `phpvm install {}` first (or `phpvm ls` to see installed runtimes).",
                spec,
                spec
            );
        }
    };

    // Verify it is actually on disk.
    let runtimes_dir = config::runtimes_dir()?;
    let runtime_path = runtimes_dir.join(&resolved);
    if !runtime_path.exists() {
        anyhow::bail!(
            "PHP runtime {} is not installed. Run `phpvm install {}` first.",
            resolved,
            spec
        );
    }

    // Persist so that `phpvm use` affects future terminals/sessions.
    // "use" is the single command that determines the active version (no
    // separate "default" setter is needed).
    config::set_current_version(&resolved)?;

    // TODO (per-project "use"): Support a project-local declaration so that
    // `phpvm use` (no argument) and/or shell integration can pick the right
    // runtime + profile automatically inside a project.
    //
    // Options to consider:
    //   - A lightweight `.phpvm-version` file containing just a specifier
    //     (e.g. "8.3", "latest", "8.4.11") — analogous to .nvmrc / .node-version.
    //   - Or (better for richness) reading from the existing project
    //     `.phpvm.toml`, which can already express `php_constraint` + `profile`
    //     (built-in or custom) + other settings.
    //
    // Important: any per-project mechanism must be able to specify the
    // extension profile, not just the PHP version, because the user noted
    // that "it would also require extension settings, not just php version."
    //
    // This would primarily affect `print_env` / activation and the no-arg
    // case of `activate`. Global `phpvm use <ver>` should probably still
    // override for the current user/session.

    // Ensure the minor-series composer home exists for globals isolation.
    // All 8.3.x patches share `~/.phpvm/composer-homes/8.3/`.
    if let Ok(composer_home) = composer_home_for(&resolved) {
        let _ = std::fs::create_dir_all(&composer_home);
    }

    // Emit the activation snippet (for immediate effect in *this* shell via eval).
    // Informational message on stderr; pure exports on stdout.
    eprintln!(
        "Using PHP {} from {}\n\
         (This is now active here. For new terminals/sessions, put \
         `eval \"$(phpvm env)\"` in your shell rc once.)",
        resolved, runtime_path
    );
    let snippet = build_activation_snippet(&resolved, &runtime_path);
    print!("{}", snippet);

    Ok(())
}

/// Returns the directory that should be used as COMPOSER_HOME for global
/// packages for a given resolved version.
///
/// Globals are shared across patch versions in the same minor series
/// (i.e. all 8.3.x runtimes share the same composer home).
/// This lives under `~/.phpvm/composer-homes/8.3/`.
pub fn composer_home_for(resolved: &str) -> Result<camino::Utf8PathBuf> {
    let v = PhpVersion::parse(resolved)?;
    let homes_dir = crate::config::data_dir()?.join("composer-homes");
    Ok(homes_dir.join(format!("{}.{}", v.major, v.minor)))
}

/// Show the currently active PHP version.
///
/// Priority: live $PHPVM_VERSION env (current shell) > persisted from
/// `phpvm use` > "none".
pub fn show_current() -> Result<()> {
    if let Ok(v) = std::env::var("PHPVM_VERSION") {
        if !v.is_empty() {
            println!("{}", v);
            return Ok(());
        }
    }

    if let Some(v) = config::get_current_version() {
        // Only report it if the runtime is still present on disk.
        if let Ok(installed) = runner::installed_versions() {
            if installed.iter().any(|i| i == &v) {
                println!("{}", v);
                return Ok(());
            }
        }
    }

    println!("none");
    Ok(())
}

/// Print shell integration for `phpvm env`.
///
/// Output is designed to be eval'ed, typically once from your shell rc:
///   eval "$(phpvm env)"
///
/// This sets up (modeled after fnm):
/// - A `phpvm` shell function wrapper. After this, plain `phpvm use <ver>`
///   will immediately update PATH / COMPOSER_HOME / PHPVM_VERSION in the
///   *current* shell (no extra manual eval needed for each `use`).
/// - Activation of the persisted current version (or one passed with --version)
///   so new shells start with the last `phpvm use`d version.
///
/// See the "Daily development" section in the README for the recommended
/// one-time setup.
pub fn print_env(version: Option<&str>) -> Result<()> {
    // Shell function wrapper. This is the key to removing the per-`use` eval step.
    // Once installed via the rc, `phpvm use 8.2` (the function) will run the
    // binary (for persistence) and then eval its export output in the current shell.
    let wrapper = r#"phpvm() {
  if [ "$1" = "use" ]; then
    eval "$(command phpvm "$@")"
  else
    command phpvm "$@"
  fi
}
"#;
    print!("{}", wrapper);

    // Determine what (if anything) to activate in *this* shell right now.
    let target_spec: Option<String> = match version {
        Some(v) => Some(v.to_string()),
        None => config::get_current_version(),
    };

    if let Some(spec) = target_spec {
        match runner::resolve_version(&spec) {
            Ok(resolved) => {
                let runtimes_dir = config::runtimes_dir()?;
                let runtime_path = runtimes_dir.join(&resolved);

                if runtime_path.exists() {
                    if let Ok(composer_home) = composer_home_for(&resolved) {
                        let _ = std::fs::create_dir_all(&composer_home);
                    }

                    let snippet = build_activation_snippet(&resolved, &runtime_path);
                    print!("{}", snippet);
                } else {
                    eprintln!(
                        "phpvm env: runtime {} not found on disk (wrapper installed anyway)",
                        resolved
                    );
                }
            }
            Err(_) => {
                eprintln!(
                    "phpvm env: could not resolve '{}' (wrapper installed anyway). Run `phpvm use` to pick a version.",
                    spec
                );
            }
        }
    }
    // No target spec at all (first time ever) → we still emitted the wrapper
    // so the user can immediately run `phpvm use <something>` and have it apply.

    Ok(())
}

/// List remote versions available for install (from the manifest).
///
/// Versions are printed one per line, newest first.
pub fn list_remote() -> Result<()> {
    let project_dir = config::current_project_dir()?;
    let cfg = config::load_config(&project_dir)?;
    let mf = manifest::fetch_from_config(&cfg)?;
    let mut versions = mf.available_versions();
    versions.sort_by(|a, b| {
        // Sort descending by semver (newest first); fall back to string cmp.
        let va = semver::Version::parse(a);
        let vb = semver::Version::parse(b);
        match (&va, &vb) {
            (Ok(va), Ok(vb)) => vb.cmp(va).then(b.cmp(a)),
            (Ok(_), Err(_)) => std::cmp::Ordering::Less,
            (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
            (Err(_), Err(_)) => b.cmp(a),
        }
    });
    versions.dedup();
    for v in versions {
        println!("{}", v);
    }
    Ok(())
}

/// Show runtime metadata for a version specifier (resolved via installed or manifest).
///
/// Output includes PHP version, bundled Composer, profile, and extension list.
pub fn show_info(spec: &str) -> Result<()> {
    // Prefer resolving against locally installed runtimes (works offline).
    let installed = runner::installed_versions().unwrap_or_default();
    let resolved = if let Ok(r) = resolve_specifier(spec, &installed) {
        r
    } else {
        // Fall back to manifest (supports discovery of not-yet-installed versions).
        let project_dir = config::current_project_dir()?;
        let cfg = config::load_config(&project_dir)?;
        let mf = manifest::fetch_from_config(&cfg)?;
        resolve_specifier(spec, &mf.available_versions())?
    };

    if let Some(metadata) = read_installed_metadata(&resolved) {
        println!("{:<12}{}", "PHP:", metadata.php);
        println!("{:<12}{}", "Composer:", metadata.composer);
        println!("{:<12}{}", "Profile:", metadata.profile);
        if let Some(manifest_profile) = metadata.manifest_profile {
            if manifest_profile != metadata.profile {
                println!("{:<12}{}", "Artifact:", manifest_profile);
            }
        }
        if !metadata.extensions.is_empty() {
            println!();
            println!("Extensions:");
            for ext in &metadata.extensions {
                println!("  {}", ext);
            }
        }
        return Ok(());
    }

    // Best-effort metadata lookup from manifest (may be cached or fail offline).
    let entry = {
        config::current_project_dir()
            .ok()
            .and_then(|pd| config::load_config(&pd).ok())
            .and_then(|c| manifest::fetch_from_config(&c).ok())
            .and_then(|m| m.find(&resolved).cloned())
    };

    if let Some(e) = entry {
        let exts = profile::builtin(&e.profile)
            .map(|p| p.extensions)
            .unwrap_or_default();

        println!("{:<12}{}", "PHP:", e.php);
        println!("{:<12}{}", "Composer:", e.composer);
        println!("{:<12}{}", "Profile:", e.profile);
        if !exts.is_empty() {
            println!();
            println!("Extensions:");
            for ext in &exts {
                println!("  {}", ext);
            }
        }
    } else {
        // No manifest data available (stale/offline); report what we know.
        println!("{:<12}{}", "PHP:", resolved);
        println!("{:<12}unknown", "Composer:");
        println!("{:<12}unknown", "Profile:");
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

#[derive(Deserialize)]
struct InstalledRuntimeMetadata {
    php: String,
    composer: String,
    profile: String,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    manifest_profile: Option<String>,
}

fn read_installed_metadata(resolved: &str) -> Option<InstalledRuntimeMetadata> {
    let metadata_path = config::runtimes_dir()
        .ok()?
        .join(resolved)
        .join("metadata.json");
    let contents = std::fs::read_to_string(metadata_path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Determine which installed version should be considered "current" for display in `ls`.
///
/// Resolution order (highest priority first):
/// 1. The PHPVM_VERSION environment variable (set by `eval "$(phpvm use X)"`).
///    This lets `phpvm ls` show the actively "used" runtime in the current shell.
/// 2. If the project/global config specifies `php_constraint`, resolve it against the
///    installed list (picks best matching installed version).
/// 3. Otherwise, pick the highest (latest) installed version.
fn compute_current_version(installed: &[String]) -> Option<String> {
    if installed.is_empty() {
        return None;
    }

    // Highest priority: explicit activation via `phpvm use` in *this* shell (env var set by eval).
    if let Ok(active) = std::env::var("PHPVM_VERSION") {
        if installed.iter().any(|v| v == &active) {
            return Some(active);
        }
    }

    // Next: the persisted value written by `phpvm use` in any previous session.
    // This makes the last `use`d version active in new terminals.
    if let Some(active) = config::get_current_version() {
        if installed.iter().any(|v| v == &active) {
            return Some(active);
        }
    }

    // Then: project/global config php_constraint.
    if let Ok(project_dir) = config::current_project_dir() {
        if let Ok(cfg) = config::load_config(&project_dir) {
            if let Some(constr) = &cfg.php_constraint {
                if let Ok(res) = resolve_specifier(constr, installed) {
                    return Some(res);
                }
            }
        }
    }
    // Fallback: highest installed version (for display purposes).
    let mut parsed: Vec<PhpVersion> = installed
        .iter()
        .filter_map(|s| PhpVersion::parse(s).ok())
        .collect();
    parsed.sort();
    parsed.last().map(|v| v.to_string())
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

    #[test]
    fn parse_latest_bare() {
        let spec = parse("latest").unwrap();
        assert_eq!(spec, VersionSpecifier::Latest);
        let spec = parse("Latest").unwrap();
        assert_eq!(spec, VersionSpecifier::Latest);
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

    #[test]
    fn resolve_latest_picks_highest_overall() {
        let available = vers(&["8.3.12", "8.4.5", "8.2.99", "7.4.33"]);
        let result = resolve_specifier("latest", &available).unwrap();
        assert_eq!(result, "8.4.5");
    }

    #[test]
    fn resolve_latest_single() {
        let available = vers(&["8.1.0"]);
        let result = resolve_specifier("latest", &available).unwrap();
        assert_eq!(result, "8.1.0");
    }

    #[test]
    fn resolve_latest_empty() {
        let available: Vec<String> = vec![];
        let result = resolve_specifier("latest", &available);
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
