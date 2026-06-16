use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use super::Provider;
use crate::manifest::{Manifest, ManifestEntry};
use crate::output::{self, StepList};
use crate::profile_preset;
use crate::runtime_metadata::RuntimeMetadata;

/// Provider that downloads prebuilt/static PHP binaries.
///
/// This is the primary provider for V1. It:
/// 1. Downloads a prebuilt runtime archive from the manifest URL
/// 2. Verifies the SHA-256 checksum
/// 3. Extracts the archive into the runtime directory
/// 4. Verifies the runtime is functional
/// 5. Writes a metadata.json file
pub struct StaticPhpProvider;

impl Provider for StaticPhpProvider {
    fn name(&self) -> &str {
        "static_php"
    }

    fn install(
        &self,
        entry: &ManifestEntry,
        target: &Utf8PathBuf,
        profile_name: &str,
        project_dir: &camino::Utf8Path,
        manifest: Option<&Manifest>,
        catalog: &[String],
    ) -> Result<()> {
        let mut steps = StepList::new();
        let result = install_with_steps(
            entry,
            target,
            profile_name,
            project_dir,
            manifest,
            catalog,
            &mut steps,
        );
        steps.finish();
        result
    }
}

fn install_with_steps(
    entry: &ManifestEntry,
    target: &Utf8PathBuf,
    profile_name: &str,
    project_dir: &camino::Utf8Path,
    manifest: Option<&Manifest>,
    catalog: &[String],
    steps: &mut StepList,
) -> Result<()> {
    steps.start("Resolve artifact for host");
    let artifact = match entry.download_for_host() {
        Ok(artifact) => artifact,
        Err(e) => {
            steps.fail("Resolve artifact for host", &e.to_string());
            return Err(e).context("Failed to resolve runtime artifact for this host");
        }
    };
    let host_target = crate::manifest::host_target().unwrap_or_else(|_| "host".to_string());
    steps.done(&format!("Resolved artifact for {host_target}"));

    steps.start("Download archive");
    let archive_file = match download_archive(&artifact.url, steps) {
        Ok(file) => file,
        Err(e) => {
            steps.fail("Download archive", &e.to_string());
            return Err(e).context("Failed to download runtime archive");
        }
    };
    steps.done("Downloaded archive");

    let archive_path = match Utf8PathBuf::from_path_buf(archive_file.path().to_path_buf()) {
        Ok(path) => path,
        Err(p) => {
            let msg = format!("Temporary archive path is not valid UTF-8: {p:?}");
            steps.fail("Download archive", &msg);
            anyhow::bail!("{msg}");
        }
    };

    steps.start("Verify SHA-256 checksum");
    if let Err(e) = verify_checksum(&archive_path, &artifact.sha256) {
        steps.fail("Verify SHA-256 checksum", &e.to_string());
        return Err(e).context("SHA-256 checksum verification failed");
    }
    steps.done("Verified SHA-256 checksum");

    let staging_path = target
        .parent()
        .map(|p| {
            p.join(format!(
                ".staging-{}",
                target.file_name().unwrap_or("runtime")
            ))
        })
        .ok_or_else(|| anyhow::anyhow!("Invalid runtime target path {}", target))?;

    steps.start("Extract runtime");
    if staging_path.exists() {
        if let Err(e) = fs::remove_dir_all(&staging_path) {
            steps.fail("Extract runtime", &e.to_string());
            return Err(e)
                .with_context(|| format!("Failed to clear staging directory {staging_path}"));
        }
    }

    if let Err(e) = extract_archive(&archive_path, &staging_path) {
        steps.fail("Extract runtime", &e.to_string());
        return Err(e).context("Failed to extract runtime archive");
    }
    steps.done("Extracted runtime");

    steps.start("Verify runtime binaries");
    let bin_php = staging_path.join("bin").join(runtime_binary_name("php"));
    if !bin_php.exists() {
        let _ = fs::remove_dir_all(&staging_path);
        let msg =
            format!("Runtime directory {staging_path} does not contain bin/php after extraction");
        steps.fail("Verify runtime binaries", &msg);
        anyhow::bail!("{msg}");
    }
    let bin_composer = staging_path
        .join("bin")
        .join(runtime_binary_name("composer"));
    if !bin_composer.exists() {
        let _ = fs::remove_dir_all(&staging_path);
        let msg = format!(
            "Runtime directory {staging_path} does not contain bin/composer after extraction"
        );
        steps.fail("Verify runtime binaries", &msg);
        anyhow::bail!("{msg}");
    }
    steps.done("Verified bin/php and bin/composer");

    if target.exists() {
        fs::remove_dir_all(target)
            .with_context(|| format!("Failed to replace existing runtime {}", target))?;
    }

    fs::rename(&staging_path, target)
        .with_context(|| format!("Failed to move staged runtime into {}", target))?;

    let profile_label = format!("Apply profile '{profile_name}'");
    steps.start(&profile_label);
    if let Err(e) = apply_preset(target, profile_name, project_dir, manifest, catalog, entry) {
        steps.fail(&profile_label, &e.to_string());
        return Err(e).context("Failed to apply profile");
    }
    steps.done(&format!("Applied profile '{profile_name}'"));

    Ok(())
}

// ── Download ──────────────────────────────────────────────────────────────

/// Download the runtime archive from `url` to a temporary file.
///
/// The temporary file is deleted when the returned handle is dropped.
fn download_archive(url: &str, steps: &StepList) -> Result<tempfile::NamedTempFile> {
    let mut tmp_file = tempfile::Builder::new()
        .suffix(&archive_suffix(url))
        .tempfile()
        .context("Failed to create temporary file for download")?;

    let client = crate::net::blocking_client()?;
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("Failed to connect to {}", url))?
        .error_for_status()
        .with_context(|| format!("Download returned error status from {}", url))?;

    let total_size = response.content_length();
    let progress_bar = steps.add_download_bar(total_size);
    let file = tmp_file.as_file_mut();

    output::download_with_progress(response, file, &progress_bar)
        .with_context(|| format!("Failed to read response from {}", url))?;

    if !progress_bar.is_hidden() {
        progress_bar.finish_and_clear();
    }

    Ok(tmp_file)
}

/// Determine an appropriate temporary file suffix from the archive URL.
fn archive_suffix(url: &str) -> &str {
    let url_lower = url.to_lowercase();
    if url_lower.ends_with(".tar.gz") {
        ".tar.gz"
    } else if url_lower.ends_with(".tgz") {
        ".tgz"
    } else if url_lower.ends_with(".zip") {
        ".zip"
    } else {
        // Default to .tar.gz — the most common format.
        ".tar.gz"
    }
}

// ── Checksum verification ─────────────────────────────────────────────────

/// Verify that the SHA-256 checksum of the file at `path` matches `expected`.
fn verify_checksum(path: &Utf8PathBuf, expected: &str) -> Result<()> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("Failed to open file for checksum verification: {}", path))?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .with_context(|| format!("Failed to read file for checksum: {}", path))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let actual = format!("{:x}", hasher.finalize());
    let expected_norm = expected.trim().to_ascii_lowercase();

    if actual != expected_norm {
        anyhow::bail!(
            "SHA-256 checksum mismatch: expected {}, got {}",
            expected_norm,
            actual
        );
    }

    Ok(())
}

// ── Extraction ────────────────────────────────────────────────────────────

/// Extract the archive at `archive_path` to the `target` directory.
///
/// Dispatches to the appropriate extractor based on the file extension. Strips
/// the top-level directory so that `bin/php` ends up at `<target>/bin/php`.
fn extract_archive(archive_path: &Utf8PathBuf, target: &Utf8PathBuf) -> Result<()> {
    let archive_str = archive_path.as_str();

    if archive_str.ends_with(".tar.gz") || archive_str.ends_with(".tgz") {
        extract_tar_gz(archive_path, target)
            .with_context(|| format!("Failed to extract tar.gz archive to {}", target))
    } else if archive_str.ends_with(".zip") {
        extract_zip(archive_path, target)
            .with_context(|| format!("Failed to extract zip archive to {}", target))
    } else {
        anyhow::bail!(
            "Unsupported archive format: {}. Expected .tar.gz, .tgz, or .zip",
            archive_str
        )
    }
}

/// Extract a `.tar.gz` or `.tgz` archive, stripping the top-level directory.
fn extract_tar_gz(archive_path: &Utf8PathBuf, target: &Utf8PathBuf) -> Result<()> {
    fs::create_dir_all(target)
        .with_context(|| format!("Failed to create target directory {}", target))?;

    let file = fs::File::open(archive_path)
        .with_context(|| format!("Failed to open archive {}", archive_path))?;

    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive
        .entries()
        .with_context(|| "Failed to iterate archive entries")?
    {
        let mut entry = entry.with_context(|| "Failed to read archive entry")?;

        let entry_path = entry
            .path()
            .with_context(|| "Failed to get entry path")?
            .into_owned();

        // Strip the top-level directory from the entry path.
        let stripped = strip_top_dir_string(&entry_path);

        if stripped.is_empty() {
            continue;
        }

        let dest_path = safe_join_stripped(target, &stripped)?;

        // Ensure parent directories exist.
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent))?;
        }

        ensure_supported_tar_entry(&entry, &dest_path)?;

        entry
            .unpack(dest_path.as_str())
            .with_context(|| format!("Failed to extract {}", dest_path))?;
    }

    Ok(())
}

/// Extract a `.zip` archive, stripping the top-level directory.
fn extract_zip(archive_path: &Utf8PathBuf, target: &Utf8PathBuf) -> Result<()> {
    fs::create_dir_all(target)
        .with_context(|| format!("Failed to create target directory {}", target))?;

    let file = fs::File::open(archive_path)
        .with_context(|| format!("Failed to open archive {}", archive_path))?;

    let mut archive = zip::ZipArchive::new(file).with_context(|| "Failed to read zip archive")?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .with_context(|| format!("Failed to read zip entry at index {}", i))?;

        let entry_name = entry.name().to_string();

        let stripped = strip_top_dir_from_str(&entry_name);

        if stripped.is_empty() {
            continue;
        }

        if entry.is_dir() {
            let dir_path = safe_join_stripped(target, &stripped)?;
            fs::create_dir_all(&dir_path)
                .with_context(|| format!("Failed to create directory {}", stripped))?;
            continue;
        }

        let dest_path = safe_join_stripped(target, &stripped)?;

        // Ensure parent directories exist.
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent))?;
        }

        let mut out_file = fs::File::create(&dest_path)
            .with_context(|| format!("Failed to create file {}", dest_path))?;

        io::copy(&mut entry, &mut out_file)
            .with_context(|| format!("Failed to extract {}", dest_path))?;

        // Restore Unix permissions if available (zip entries may carry them).
        // Best-effort only: ignore failures (e.g. unusual filesystems, Windows
        // extraction of a unix-targeted archive). On Windows, executability for
        // prebuilt PHP/Composer is determined by .exe extension + PATHEXT rather
        // than mode bits; the manifest-supplied archives for the host are expected
        // to be correct.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                let _ = fs::set_permissions(&dest_path, fs::Permissions::from_mode(mode));
            }
        }
    }

    Ok(())
}

/// Strip the first path component from a `std::path::Path`, returning a String.
fn strip_top_dir_string(path: &Path) -> String {
    let mut components = path.components();
    // Skip the first component (the top-level directory).
    components.next();
    components.as_path().to_string_lossy().into_owned()
}

/// Strip the first path component from a `/`-delimited archive entry name.
fn strip_top_dir_from_str(name: &str) -> String {
    // Remove trailing slash for consistent handling of directory entries.
    let trimmed = name.trim_end_matches('/');
    if let Some(pos) = trimmed.find('/') {
        trimmed[pos + 1..].to_string()
    } else {
        String::new()
    }
}

fn safe_join_stripped(target: &Utf8PathBuf, stripped: &str) -> Result<Utf8PathBuf> {
    let stripped_path = Path::new(stripped);
    if stripped_path.is_absolute() {
        anyhow::bail!("Archive entry escapes runtime directory: {}", stripped);
    }

    let mut clean = PathBuf::new();
    for component in stripped_path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("Archive entry escapes runtime directory: {}", stripped);
            }
        }
    }

    if clean.as_os_str().is_empty() {
        anyhow::bail!("Archive entry has no safe destination path");
    }

    let clean_utf8 = Utf8PathBuf::from_path_buf(clean)
        .map_err(|p| anyhow::anyhow!("Archive entry path is not valid UTF-8: {:?}", p))?;
    Ok(target.join(clean_utf8))
}

fn ensure_supported_tar_entry(
    entry: &tar::Entry<'_, GzDecoder<fs::File>>,
    path: &Utf8PathBuf,
) -> Result<()> {
    let entry_type = entry.header().entry_type();
    if entry_type.is_symlink() || entry_type.is_hard_link() {
        anyhow::bail!("Archive entry {} is a link, which is not supported", path);
    }
    Ok(())
}

fn runtime_binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", name)
    } else {
        name.to_string()
    }
}

// ── Profile ini presets ───────────────────────────────────────────────────

/// Resolve, validate, activate, and persist metadata for a profile switch.
pub fn apply_preset(
    target: &Utf8PathBuf,
    profile_name: &str,
    project_dir: &camino::Utf8Path,
    manifest: Option<&Manifest>,
    catalog: &[String],
    entry: &ManifestEntry,
) -> Result<()> {
    let preset =
        profile_preset::resolve_preset(profile_name, project_dir, target, manifest, catalog)?;

    profile_preset::validate_preset_extensions(&preset.path, catalog)?;

    // Static model baseline: profiles are user-level .ini presets for tuning
    // (memory_limit, opcache, error_reporting, etc.). The binary has its
    // extension catalog compiled in; we never write etc/, conf.d/, or
    // extension load directives into the runtime tree.
    let enabled = profile_preset::parse_enabled_extensions_from_file(&preset.path)?;

    // Best-effort: materialize a sanitized copy of the active preset under
    // ~/.phpvm/ini/<ver>.ini . This drives PHPRC for `phpvm use` and bare
    // `php`/`composer` invocations so that profile settings take effect.
    if let Ok(managed) = crate::runtime_metadata::managed_ini_for_version(&entry.php) {
        if let Some(p) = managed.parent() {
            let _ = fs::create_dir_all(p);
        }
        if let Ok(raw) = fs::read_to_string(&preset.path) {
            let clean = profile_preset::filter_non_extension_ini_lines(&raw);
            let _ = fs::write(&managed, clean);
        }
    }

    let enabled_extensions = enabled;

    let mut metadata = RuntimeMetadata::read(&entry.php)?
        .unwrap_or_else(|| RuntimeMetadata::from_install(entry, profile_name, &preset, catalog));
    metadata.update_active_preset(profile_name, &preset);
    metadata.runtime_type = entry.runtime_type.clone();
    metadata.abi = entry.abi.clone();
    metadata.thread_safety = entry.thread_safety.clone();
    metadata.extension_api = entry.extension_api.clone();
    metadata.extension_catalog = entry.extensions.clone();
    metadata.enabled_extensions = enabled_extensions;
    if metadata.available_extensions.is_empty() {
        metadata.available_extensions = catalog.to_vec();
    }
    metadata.write(target)?;

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{RuntimeExtension, RuntimeType};

    // ── archive_suffix ─────────────────────────────────────────────────

    #[test]
    fn suffix_tar_gz() {
        assert_eq!(
            archive_suffix("https://example.com/php-8.3.23.tar.gz"),
            ".tar.gz"
        );
    }

    #[test]
    fn suffix_tgz() {
        assert_eq!(archive_suffix("https://example.com/php-8.3.23.tgz"), ".tgz");
    }

    #[test]
    fn suffix_zip() {
        assert_eq!(archive_suffix("https://example.com/php-8.3.23.zip"), ".zip");
    }

    #[test]
    fn suffix_unknown_defaults_to_tar_gz() {
        assert_eq!(
            archive_suffix("https://example.com/php-8.3.23.unknown"),
            ".tar.gz"
        );
    }

    // ── strip_top_dir_string ──────────────────────────────────────────

    #[test]
    fn strip_top_dir_string_simple() {
        let p = Path::new("php-8.3.23/bin/php");
        assert_eq!(strip_top_dir_string(p), "bin/php");
    }

    #[test]
    fn strip_top_dir_string_single_component() {
        let p = Path::new("php-8.3.23");
        assert_eq!(strip_top_dir_string(p), "");
    }

    #[test]
    fn strip_top_dir_string_empty() {
        let p = Path::new("");
        assert_eq!(strip_top_dir_string(p), "");
    }

    // ── strip_top_dir_from_str ────────────────────────────────────────

    #[test]
    fn strip_top_dir_from_str_simple() {
        assert_eq!(strip_top_dir_from_str("php-8.3.23/bin/php"), "bin/php");
    }

    #[test]
    fn strip_top_dir_from_str_directory() {
        assert_eq!(strip_top_dir_from_str("php-8.3.23/bin/"), "bin");
    }

    #[test]
    fn strip_top_dir_from_str_single_component() {
        assert_eq!(strip_top_dir_from_str("php-8.3.23"), "");
    }

    #[test]
    fn strip_top_dir_from_str_empty() {
        assert_eq!(strip_top_dir_from_str(""), "");
    }

    // ── verify_checksum ───────────────────────────────────────────────

    #[test]
    fn verify_checksum_matches() {
        // Create a temporary file with known content.
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        io::Write::write_all(&mut tmp, b"hello world").unwrap();
        let (_, tmp_path) = tmp.keep().unwrap();
        let path = Utf8PathBuf::from_path_buf(tmp_path).unwrap();

        // SHA-256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert!(verify_checksum(&path, expected).is_ok());
    }

    #[test]
    fn verify_checksum_mismatch() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        io::Write::write_all(&mut tmp, b"hello world").unwrap();
        let (_, tmp_path) = tmp.keep().unwrap();
        let path = Utf8PathBuf::from_path_buf(tmp_path).unwrap();

        let result = verify_checksum(&path, "deadbeef");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("checksum mismatch"));
        assert!(err.contains("expected deadbeef"));
    }

    // ── Extraction tests ──────────────────────────────────────────────

    #[test]
    fn extract_tar_gz_strips_top_dir() -> Result<()> {
        let target = tempfile::TempDir::new()?;
        let target_path = Utf8PathBuf::from_path_buf(target.path().to_path_buf())
            .map_err(|p| anyhow::anyhow!("{:?}", p))?;

        let archive_path = create_test_tar_gz()?;

        extract_tar_gz(&archive_path, &target_path)?;

        // After extraction, bin/php should exist directly in the target.
        let php_path = target_path.join("bin").join("php");
        assert!(php_path.exists(), "bin/php should exist after extraction");

        // Top-level directory should NOT exist as a subdirectory.
        let top_dir = target_path.join("php-0.0.0");
        assert!(
            !top_dir.exists(),
            "Top-level directory should be stripped, but {} exists",
            top_dir
        );

        Ok(())
    }

    #[test]
    fn extract_zip_strips_top_dir() -> Result<()> {
        let target = tempfile::TempDir::new()?;
        let target_path = Utf8PathBuf::from_path_buf(target.path().to_path_buf())
            .map_err(|p| anyhow::anyhow!("{:?}", p))?;

        let archive_path = create_test_zip()?;

        extract_zip(&archive_path, &target_path)?;

        let php_path = target_path.join("bin").join("php");
        assert!(php_path.exists(), "bin/php should exist after extraction");

        let top_dir = target_path.join("php-0.0.0");
        assert!(
            !top_dir.exists(),
            "Top-level directory should be stripped, but {} exists",
            top_dir
        );

        Ok(())
    }

    // ── profile ini presets ───────────────────────────────────────────

    #[test]
    fn apply_preset_static_writes_metadata_only_no_etc_tree() -> Result<()> {
        let target = tempfile::TempDir::new()?;
        let target_path = Utf8PathBuf::from_path_buf(target.path().to_path_buf())
            .map_err(|p| anyhow::anyhow!("{:?}", p))?;

        let project = tempfile::TempDir::new()?;
        let project_path = Utf8PathBuf::from_path_buf(project.path().to_path_buf())
            .map_err(|p| anyhow::anyhow!("{:?}", p))?;
        let preset_dir = project_path.join(".phpvm").join("profiles");
        fs::create_dir_all(&preset_dir)?;
        let preset_path = preset_dir.join("minimal.ini");
        fs::write(
            &preset_path,
            "; static-minimal\nmemory_limit=256M\ndisplay_errors=On\n",
        )?;

        // Use a temp PHPVM_HOME so the managed ini materialize has a predictable place
        let home = tempfile::TempDir::new()?;
        let prev = std::env::var("PHPVM_HOME").ok();
        std::env::set_var("PHPVM_HOME", home.path());

        let entry = ManifestEntry {
            php: "8.4.1".into(),
            composer: "2.9.2".into(),
            profile: None,
            runtime_type: RuntimeType::Static,
            abi: None,
            thread_safety: None,
            extension_api: None,
            extensions: vec![RuntimeExtension::from_name("curl".into())],
            url: "https://example.com/php-8.4.1.tar.gz".into(),
            sha256: "beefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeef".into(),
            artifacts: None,
        };
        let catalog = entry.extension_catalog();

        apply_preset(
            &target_path,
            "minimal",
            &project_path,
            None,
            &catalog,
            &entry,
        )?;

        // No etc/ tree for pure static
        let etc = target_path.join("etc");
        assert!(
            !etc.exists(),
            "static apply must not create etc/ inside runtime"
        );

        // Metadata still written (bookkeeping)
        let meta_path = target_path.join("metadata.json");
        assert!(meta_path.exists());
        let parsed: serde_json::Value = serde_json::from_str(&fs::read_to_string(&meta_path)?)?;
        assert_eq!(parsed["php"], "8.4.1");
        assert_eq!(parsed["active_profile"], "minimal");

        // Managed ini should have been materialized under the temp HOME/ini/
        // (PHPVM_HOME points at the temp dir itself; data_dir uses it verbatim.)
        let managed = home.path().join("ini").join("8.4.1.ini");
        assert!(
            managed.exists(),
            "static profile should materialize managed ini for PHPRC"
        );
        let mcontent = fs::read_to_string(&managed)?;
        assert!(mcontent.contains("memory_limit=256M"));
        assert!(!mcontent.contains("extension="));

        // restore env
        match prev {
            Some(v) => std::env::set_var("PHPVM_HOME", v),
            None => std::env::remove_var("PHPVM_HOME"),
        }

        Ok(())
    }

    #[test]
    fn safe_join_rejects_parent_directory() {
        let target = Utf8PathBuf::from("/tmp/phpvm-runtime");
        let result = safe_join_stripped(&target, "../outside");
        assert!(result.is_err());
    }

    // ── Helpers ────────────────────────────────────────────────────────

    /// Create a test tar.gz archive in a temp file.
    /// Archive structure: php-0.0.0/bin/php (a dummy file)
    fn create_test_tar_gz() -> Result<Utf8PathBuf> {
        let tmp = tempfile::Builder::new()
            .suffix(".tar.gz")
            .tempfile()
            .context("Failed to create temp file")?;
        let (tmp_file, tmp_path) = tmp.keep().context("Failed to persist temp file")?;

        let encoder = flate2::write::GzEncoder::new(tmp_file, flate2::Compression::default());
        let mut tar_builder = tar::Builder::new(encoder);

        // Add top-level directory entry.
        let mut dir_header = tar::Header::new_gnu();
        dir_header.set_path("php-0.0.0").unwrap();
        dir_header.set_entry_type(tar::EntryType::Directory);
        dir_header.set_size(0);
        dir_header.set_mode(0o755);
        dir_header.set_cksum();
        tar_builder
            .append(&dir_header, &mut io::empty())
            .context("Failed to add directory entry")?;

        // Add bin/ directory entry.
        let mut bindir_header = tar::Header::new_gnu();
        bindir_header.set_path("php-0.0.0/bin").unwrap();
        bindir_header.set_entry_type(tar::EntryType::Directory);
        bindir_header.set_size(0);
        bindir_header.set_mode(0o755);
        bindir_header.set_cksum();
        tar_builder
            .append(&bindir_header, &mut io::empty())
            .context("Failed to add bin directory entry")?;

        // Add bin/php file entry.
        let mut file_header = tar::Header::new_gnu();
        file_header.set_path("php-0.0.0/bin/php").unwrap();
        file_header.set_entry_type(tar::EntryType::Regular);
        file_header.set_size(6);
        file_header.set_mode(0o755);
        file_header.set_cksum();
        tar_builder
            .append(&file_header, "phpbin".as_bytes())
            .context("Failed to add file entry")?;

        // Add bin/composer file entry.
        let mut composer_header = tar::Header::new_gnu();
        composer_header.set_path("php-0.0.0/bin/composer").unwrap();
        composer_header.set_entry_type(tar::EntryType::Regular);
        composer_header.set_size(8);
        composer_header.set_mode(0o755);
        composer_header.set_cksum();
        tar_builder
            .append(&composer_header, "phpcmpbin".as_bytes())
            .context("Failed to add composer entry")?;

        // Finish writing.
        let encoder = tar_builder
            .into_inner()
            .context("Failed to finalize tar builder")?;
        encoder.finish().context("Failed to finalize gz encoder")?;

        Utf8PathBuf::from_path_buf(tmp_path)
            .map_err(|p| anyhow::anyhow!("Temporary path is not valid UTF-8: {:?}", p))
    }

    /// Create a test zip archive in a temp file.
    /// Archive structure: php-0.0.0/bin/php (a dummy file)
    fn create_test_zip() -> Result<Utf8PathBuf> {
        use std::io::Write as IoWrite;

        let tmp = tempfile::Builder::new()
            .suffix(".zip")
            .tempfile()
            .context("Failed to create temp file")?;
        let (tmp_file, tmp_path) = tmp.keep().context("Failed to persist temp file")?;

        let mut zip_writer = zip::ZipWriter::new(tmp_file);
        let opts = zip::write::SimpleFileOptions::default();

        // Add directory entry.
        zip_writer
            .add_directory("php-0.0.0/", opts)
            .context("Failed to add directory entry")?;

        // Add bin directory.
        zip_writer
            .add_directory("php-0.0.0/bin/", opts)
            .context("Failed to add bin directory entry")?;

        // Add bin/php file.
        zip_writer
            .start_file("php-0.0.0/bin/php", opts)
            .context("Failed to start zip file entry")?;
        zip_writer
            .write_all(b"phpbin")
            .context("Failed to write zip file content")?;

        // Add bin/composer file.
        zip_writer
            .start_file("php-0.0.0/bin/composer", opts)
            .context("Failed to start zip file entry")?;
        zip_writer
            .write_all(b"phpcmpbin")
            .context("Failed to write zip file content")?;

        zip_writer
            .finish()
            .context("Failed to finalize zip writer")?;

        Utf8PathBuf::from_path_buf(tmp_path)
            .map_err(|p| anyhow::anyhow!("Temporary path is not valid UTF-8: {:?}", p))
    }
}
