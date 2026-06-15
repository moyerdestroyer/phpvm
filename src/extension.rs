use std::fs;
use std::io::Write;
use std::process::Command;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};

use crate::config;
use crate::manifest::{RuntimeExtension, RuntimeType};
use crate::output;
use crate::profile_preset;
use crate::runtime_metadata::{self, RuntimeMetadata};

pub fn list(version_spec: Option<&str>, available_only: bool, enabled_only: bool) -> Result<()> {
    let runtime = resolve_dynamic_runtime(version_spec)?;
    let enabled = enabled_extensions_from_conf_d(&runtime.path)?;

    output::heading(&format!("Extensions for PHP {}", runtime.version));
    if !enabled_only {
        output::info("Available:");
        for ext in &runtime.metadata.extension_catalog {
            let marker = if enabled.iter().any(|name| name == &ext.name) {
                "*"
            } else {
                " "
            };
            output::list_item(&format!(
                "{} {} {}",
                marker,
                ext.name,
                output::dim(&ext.extension_type)
            ));
        }
    }

    if !available_only {
        if !enabled_only {
            output::blank();
        }
        output::info("Enabled:");
        for name in enabled {
            output::list_item_dim(&name);
        }
    }

    Ok(())
}

pub fn enable(name: &str, version_spec: Option<&str>) -> Result<()> {
    let runtime = resolve_dynamic_runtime(version_spec)?;
    let ext = runtime
        .metadata
        .extension_catalog
        .iter()
        .find(|ext| ext.name == name)
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Extension '{}' is not available in PHP {}. Install it first with `phpvm ext install`.",
                name,
                runtime.version
            )
        })?;

    write_extension_snippet(&runtime.path, &ext, snippet_path(&runtime.path, name)?)?;
    validate_extension_loads(&runtime.path, &ext)
        .with_context(|| format!("Extension '{}' did not load cleanly", name))?;
    refresh_enabled_metadata(&runtime.version, &runtime.path)?;
    output::success(&format!(
        "Enabled extension '{}' for PHP {}",
        name, runtime.version
    ));
    Ok(())
}

pub fn disable(name: &str, version_spec: Option<&str>) -> Result<()> {
    let runtime = resolve_dynamic_runtime(version_spec)?;
    let snippet = snippet_path(&runtime.path, name)?;
    if snippet.exists() {
        fs::remove_file(&snippet).with_context(|| format!("Failed to remove {}", snippet))?;
        refresh_enabled_metadata(&runtime.version, &runtime.path)?;
        output::success(&format!(
            "Disabled extension '{}' for PHP {}",
            name, runtime.version
        ));
        return Ok(());
    }

    let profile_ini = runtime_metadata::conf_d_dir(&runtime.path).join("20-profile.ini");
    if profile_ini.exists() {
        let contents = fs::read_to_string(&profile_ini)
            .with_context(|| format!("Failed to read {}", profile_ini))?;
        if profile_preset::parse_enabled_extensions(&contents)
            .iter()
            .any(|enabled| enabled == name)
        {
            anyhow::bail!(
                "Extension '{}' is enabled by the active profile. Switch profiles or edit the profile preset.",
                name
            );
        }
    }

    output::warn(&format!(
        "Extension '{}' did not have a manual enable snippet for PHP {}",
        name, runtime.version
    ));
    Ok(())
}

pub fn install(
    source: &str,
    name: Option<&str>,
    kind: &str,
    version_spec: Option<&str>,
) -> Result<()> {
    let mut runtime = resolve_dynamic_runtime(version_spec)?;
    let source_path = fetch_source_if_needed(source)?;
    let extension_name = name
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| extension_name_from_source(source, &source_path));
    profile_preset::validate_profile_name(&extension_name)?;

    let dest_dir = runtime_metadata::custom_extension_dir(&runtime.path);
    fs::create_dir_all(&dest_dir).with_context(|| format!("Failed to create {}", dest_dir))?;
    let suffix = source_path
        .extension()
        .map(|ext| ext.to_string())
        .unwrap_or_else(|| default_extension_suffix().to_string());
    let dest = dest_dir.join(format!("{extension_name}.{suffix}"));
    fs::copy(&source_path, &dest)
        .with_context(|| format!("Failed to copy {} to {}", source_path, dest))?;

    let descriptor = RuntimeExtension {
        name: extension_name.clone(),
        extension_type: kind.to_string(),
        bundled: false,
        default_enabled: false,
        file: Some(relative_to_runtime(&runtime.path, &dest)?),
    };

    validate_extension_loads(&runtime.path, &descriptor)
        .with_context(|| format!("Custom extension '{}' did not load cleanly", extension_name))?;

    runtime
        .metadata
        .extension_catalog
        .retain(|ext| ext.name != extension_name);
    runtime.metadata.extension_catalog.push(descriptor.clone());
    runtime
        .metadata
        .available_extensions
        .retain(|ext| ext != &extension_name);
    runtime
        .metadata
        .available_extensions
        .push(extension_name.clone());
    runtime.metadata.write(&runtime.path)?;

    write_extension_snippet(
        &runtime.path,
        &descriptor,
        snippet_path(&runtime.path, &extension_name)?,
    )?;
    refresh_enabled_metadata(&runtime.version, &runtime.path)?;

    output::success(&format!(
        "Installed and enabled extension '{}' for PHP {}",
        extension_name, runtime.version
    ));
    Ok(())
}

pub fn path(version_spec: Option<&str>) -> Result<()> {
    let runtime = resolve_dynamic_runtime(version_spec)?;
    println!("{}", runtime_metadata::extension_dir(&runtime.path));
    Ok(())
}

struct DynamicRuntime {
    version: String,
    path: Utf8PathBuf,
    metadata: RuntimeMetadata,
}

fn resolve_dynamic_runtime(version_spec: Option<&str>) -> Result<DynamicRuntime> {
    let version = resolve_target_runtime(version_spec)?;
    let path = config::runtimes_dir()?.join(&version);
    let metadata = RuntimeMetadata::read(&version)?
        .with_context(|| format!("Runtime {} is missing metadata.json", version))?;
    if metadata.runtime_type != RuntimeType::Dynamic {
        anyhow::bail!(
            "PHP {} is a static runtime. Extension enable/disable requires a dynamic runtime bundle.",
            version
        );
    }
    Ok(DynamicRuntime {
        version,
        path,
        metadata,
    })
}

fn resolve_target_runtime(version_spec: Option<&str>) -> Result<String> {
    crate::version::resolve_active_runtime(version_spec)
}

fn write_extension_snippet(
    runtime_path: &Utf8Path,
    ext: &RuntimeExtension,
    snippet: Utf8PathBuf,
) -> Result<()> {
    let conf_d = runtime_metadata::conf_d_dir(runtime_path);
    fs::create_dir_all(&conf_d).with_context(|| format!("Failed to create {}", conf_d))?;
    let line = dynamic_load_line(runtime_path, ext);
    fs::write(
        &snippet,
        format!("; Generated by phpvm ext enable\n{line}\n"),
    )
    .with_context(|| format!("Failed to write {}", snippet))?;
    Ok(())
}

fn dynamic_load_line(runtime_path: &Utf8Path, ext: &RuntimeExtension) -> String {
    let target = ext.load_target();
    let path = if target.contains('/') || target.contains('\\') {
        runtime_path.join(target)
    } else {
        runtime_metadata::extension_dir(runtime_path).join(target)
    };
    format!("{}={}", ext.load_directive(), path)
}

fn validate_extension_loads(runtime_path: &Utf8Path, ext: &RuntimeExtension) -> Result<()> {
    let php = runtime_path
        .join("bin")
        .join(if cfg!(windows) { "php.exe" } else { "php" });
    if !php.exists() {
        anyhow::bail!("PHP binary not found: {}", php);
    }

    let mut command = Command::new(php.as_str());
    command
        .arg("-n")
        .arg("-d")
        .arg(dynamic_load_line(runtime_path, ext))
        .arg("-m");
    let output = command
        .output()
        .with_context(|| "Failed to run PHP extension validation")?;
    if !output.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(())
}

fn enabled_extensions_from_conf_d(runtime_path: &Utf8Path) -> Result<Vec<String>> {
    let conf_d = runtime_metadata::conf_d_dir(runtime_path);
    let mut enabled = Vec::new();
    if !conf_d.exists() {
        return Ok(enabled);
    }

    let mut files = fs::read_dir(conf_d.as_std_path())?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| Utf8PathBuf::from_path_buf(entry.path()).ok())
        .filter(|path| path.extension() == Some("ini"))
        .collect::<Vec<_>>();
    files.sort();

    for path in files {
        let contents =
            fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path))?;
        for ext in profile_preset::parse_enabled_extensions(&contents) {
            if !enabled.iter().any(|existing| existing == &ext) {
                enabled.push(ext);
            }
        }
    }
    Ok(enabled)
}

fn refresh_enabled_metadata(version: &str, runtime_path: &Utf8Path) -> Result<()> {
    let Some(mut metadata) = RuntimeMetadata::read(version)? else {
        return Ok(());
    };
    metadata.enabled_extensions = enabled_extensions_from_conf_d(runtime_path)?;
    metadata.write(runtime_path)
}

fn snippet_path(runtime_path: &Utf8Path, name: &str) -> Result<Utf8PathBuf> {
    profile_preset::validate_profile_name(name)?;
    Ok(runtime_metadata::conf_d_dir(runtime_path).join(format!("30-extension-{name}.ini")))
}

fn fetch_source_if_needed(source: &str) -> Result<Utf8PathBuf> {
    if !source.starts_with("http://") && !source.starts_with("https://") {
        return Ok(Utf8PathBuf::from(source));
    }
    let client = crate::net::blocking_client()?;
    let response = client
        .get(source)
        .send()
        .with_context(|| format!("Failed to fetch extension from {}", source))?
        .error_for_status()
        .with_context(|| format!("Extension download returned error status from {}", source))?;
    let suffix = source
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.').map(|(_, ext)| ext))
        .unwrap_or(default_extension_suffix());
    let mut file = tempfile::Builder::new()
        .suffix(&format!(".{suffix}"))
        .tempfile()
        .with_context(|| "Failed to create temporary extension download file")?;
    let bytes = response
        .bytes()
        .with_context(|| "Failed to read extension download body")?;
    file.write_all(&bytes)
        .with_context(|| "Failed to write extension download")?;
    let (_file, path) = file
        .keep()
        .with_context(|| "Failed to keep extension download file")?;
    Utf8PathBuf::from_path_buf(path)
        .map_err(|path| anyhow::anyhow!("Downloaded path is not valid UTF-8: {:?}", path))
}

fn extension_name_from_path(path: &Utf8Path) -> String {
    path.file_stem()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "extension".to_string())
}

fn extension_name_from_source(source: &str, path: &Utf8Path) -> String {
    if source.starts_with("http://") || source.starts_with("https://") {
        return source
            .rsplit('/')
            .next()
            .and_then(|name| name.split('?').next())
            .and_then(|name| Utf8Path::new(name).file_stem())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| extension_name_from_path(path));
    }
    extension_name_from_path(path)
}

fn default_extension_suffix() -> &'static str {
    if cfg!(windows) {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    }
}

fn relative_to_runtime(runtime_path: &Utf8Path, path: &Utf8Path) -> Result<String> {
    path.strip_prefix(runtime_path)
        .map(|relative| relative.to_string())
        .with_context(|| format!("{} is not inside {}", path, runtime_path))
}
