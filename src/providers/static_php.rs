use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use super::Provider;
use crate::manifest::ManifestEntry;
use crate::profile::Profile;

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
        profile: &Profile,
    ) -> Result<()> {
        // ── 1. Download archive to a temporary file ────────────────────
        let tmp_file =
            download_archive(&entry.url).context("Failed to download runtime archive")?;

        // ── 2. Verify SHA-256 checksum ────────────────────────────────
        verify_checksum(&tmp_file, &entry.sha256)
            .context("SHA-256 checksum verification failed")?;

        // ── 3. Extract archive to target directory ────────────────────
        extract_archive(&tmp_file, target).context("Failed to extract runtime archive")?;

        // Clean up the temporary file after extraction.
        let _ = fs::remove_file(&tmp_file);

        // ── 4. Verify the extracted runtime has bin/php ───────────────
        let bin_php = target.join("bin").join(runtime_binary_name("php"));
        if !bin_php.exists() {
            anyhow::bail!(
                "Runtime directory {} does not contain bin/php after extraction",
                target
            );
        }
        let bin_composer = target.join("bin").join(runtime_binary_name("composer"));
        if !bin_composer.exists() {
            anyhow::bail!(
                "Runtime directory {} does not contain bin/composer after extraction",
                target
            );
        }

        // ── 5. Write metadata.json ────────────────────────────────────
        write_metadata(entry, target, profile).context("Failed to write metadata.json")?;

        Ok(())
    }
}

// ── Download ──────────────────────────────────────────────────────────────

/// Download the runtime archive from `url` to a temporary file.
///
/// Returns the path to the temporary file. The caller is responsible for
/// deleting it after use.
fn download_archive(url: &str) -> Result<Utf8PathBuf> {
    // Create a temporary file. We use `.keep()` to persist the file so we can
    // reopen it later; the OS will clean up when phpvm exits.
    let (tmp_file, tmp_path) = tempfile::Builder::new()
        .suffix(&archive_suffix(url))
        .tempfile()
        .context("Failed to create temporary file for download")?
        .keep()
        .context("Failed to persist temporary file")?;

    // Drop the handle so we can read the file later for checksum verification.
    drop(tmp_file);

    let response = reqwest::blocking::get(url)
        .with_context(|| format!("Failed to connect to {}", url))?
        .error_for_status()
        .with_context(|| format!("Download returned error status from {}", url))?;

    let total_size = response.content_length();

    // Create progress bar. If we know the total size, use a bar; otherwise
    // show a spinner with byte count.
    let progress = indicatif::ProgressBar::new(total_size.unwrap_or(0));
    if total_size.is_some() {
        progress.set_style(
            indicatif::ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] \
                     {bytes}/{total_bytes} ({eta})",
                )
                .expect("hardcoded progress bar template is valid")
                .progress_chars("#>-"),
        );
    } else {
        progress.set_style(
            indicatif::ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {bytes} downloaded")
                .expect("hardcoded progress spinner template is valid"),
        );
    }

    let mut file = fs::File::create(&tmp_path)
        .with_context(|| format!("Failed to create download file {}", tmp_path.display()))?;

    let mut downloaded: u64 = 0;
    let mut buffer = [0u8; 8192];
    let mut reader = response;

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .with_context(|| format!("Failed to read response from {}", url))?;
        if bytes_read == 0 {
            break;
        }
        io::Write::write_all(&mut file, &buffer[..bytes_read])
            .with_context(|| format!("Failed to write to {}", tmp_path.display()))?;
        downloaded += bytes_read as u64;
        progress.set_position(downloaded);
    }

    progress.finish_with_message("Download complete");

    Utf8PathBuf::from_path_buf(tmp_path)
        .map_err(|p| anyhow::anyhow!("Temporary path is not valid UTF-8: {:?}", p))
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

    if actual != expected {
        anyhow::bail!(
            "SHA-256 checksum mismatch: expected {}, got {}",
            expected,
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

// ── Metadata ──────────────────────────────────────────────────────────────

/// Metadata written alongside an installed runtime.
#[derive(serde::Serialize)]
struct RuntimeMetadata {
    php: String,
    composer: String,
    profile: String,
    extensions: Vec<String>,
    manifest_profile: String,
    installed_at: String,
}

/// Write a `metadata.json` file in the target directory.
fn write_metadata(entry: &ManifestEntry, target: &Utf8PathBuf, profile: &Profile) -> Result<()> {
    let metadata = RuntimeMetadata {
        php: entry.php.clone(),
        composer: entry.composer.clone(),
        profile: profile.name.clone(),
        extensions: profile.extensions.clone(),
        manifest_profile: entry.profile.clone(),
        installed_at: iso8601_now(),
    };

    let json =
        serde_json::to_string_pretty(&metadata).with_context(|| "Failed to serialize metadata")?;

    let meta_path = target.join("metadata.json");
    fs::write(&meta_path, json)
        .with_context(|| format!("Failed to write metadata to {}", meta_path))?;

    Ok(())
}

/// Return the current UTC time as an ISO 8601 string.
///
/// Uses a pure-Rust Gregorian calendar conversion (no chrono crate needed). If
/// obtaining the system time fails (extremely unlikely), falls back to the epoch.
fn iso8601_now() -> String {
    use std::time::SystemTime;

    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(dur) => {
            let secs = dur.as_secs();
            let days = secs / 86400;
            let time_of_day = secs % 86400;
            let hours = time_of_day / 3600;
            let minutes = (time_of_day % 3600) / 60;
            let seconds = time_of_day % 60;

            let (y, m, d) = civil_from_days(days as i64);

            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                y, m, d, hours, minutes, seconds
            )
        }
        Err(_) => "1970-01-01T00:00:00Z".to_string(),
    }
}

/// Convert days since Unix epoch (1970-01-01) to a (year, month, day) triple.
///
/// Uses the Howard Hinnant algorithm:
/// http://howardhinnant.github.io/date_algorithms.html#civil_from_days
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── write_metadata ─────────────────────────────────────────────────

    #[test]
    fn write_metadata_creates_file() -> Result<()> {
        let target = tempfile::TempDir::new()?;
        let target_path = Utf8PathBuf::from_path_buf(target.path().to_path_buf())
            .map_err(|p| anyhow::anyhow!("{:?}", p))?;

        let entry = ManifestEntry {
            php: "8.3.23".into(),
            composer: "2.9.2".into(),
            profile: "wordpress".into(),
            url: "https://example.com/php-8.3.23.tar.gz".into(),
            sha256: "abc123".into(),
        };

        let profile = Profile {
            name: "wordpress".into(),
            extensions: vec!["curl".into(), "mbstring".into()],
        };

        write_metadata(&entry, &target_path, &profile)?;

        let meta_path = target_path.join("metadata.json");
        assert!(meta_path.exists());

        let content = fs::read_to_string(&meta_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(parsed["php"], "8.3.23");
        assert_eq!(parsed["composer"], "2.9.2");
        assert_eq!(parsed["profile"], "wordpress");
        assert_eq!(parsed["extensions"][0], "curl");
        assert_eq!(parsed["manifest_profile"], "wordpress");
        assert!(parsed["installed_at"].is_string());

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
