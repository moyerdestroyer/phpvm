use serde::{Deserialize, Serialize};

use crate::output::OutputFormat;

// ---------------------------------------------------------------------------
// Profile — a named set of PHP extensions
// ---------------------------------------------------------------------------

/// An extension profile: a name and a list of PHP extensions.
///
/// Built-in profiles (wordpress, laravel, minimal) ship with PHPVM.
/// Users can define custom profiles in `.phpvm.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    /// Profile name (e.g. "wordpress", "laravel", "minimal", "drupal")
    pub name: String,

    /// PHP extensions included in this profile
    pub extensions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Built-in profiles
// ---------------------------------------------------------------------------

/// Return the built-in wordpress profile.
pub fn wordpress() -> Profile {
    Profile {
        name: "wordpress".to_string(),
        extensions: vec![
            "curl".to_string(),
            "dom".to_string(),
            "gd".to_string(),
            "intl".to_string(),
            "mbstring".to_string(),
            "mysqli".to_string(),
            "openssl".to_string(),
            "pdo_mysql".to_string(),
            "xml".to_string(),
            "zip".to_string(),
        ],
    }
}

/// Return the built-in laravel profile.
pub fn laravel() -> Profile {
    Profile {
        name: "laravel".to_string(),
        extensions: vec![
            "curl".to_string(),
            "intl".to_string(),
            "mbstring".to_string(),
            "openssl".to_string(),
            "pdo_mysql".to_string(),
            "tokenizer".to_string(),
            "xml".to_string(),
            "zip".to_string(),
        ],
    }
}

/// Return the built-in minimal profile.
pub fn minimal() -> Profile {
    Profile {
        name: "minimal".to_string(),
        extensions: vec![],
    }
}

/// Return all built-in profiles.
pub fn builtins() -> Vec<Profile> {
    vec![wordpress(), laravel(), minimal()]
}

/// Look up a built-in profile by name.
pub fn builtin(name: &str) -> Option<Profile> {
    match name {
        "wordpress" => Some(wordpress()),
        "laravel" => Some(laravel()),
        "minimal" => Some(minimal()),
        _ => None,
    }
}

/// Resolve a profile name to a Profile, checking built-ins first then
/// custom profiles.
pub fn resolve(name: &str, custom_profiles: &[Profile]) -> Option<Profile> {
    // Built-in profiles take priority.
    if let Some(p) = builtin(name) {
        return Some(p);
    }
    // Then check custom profiles.
    custom_profiles.iter().find(|p| p.name == name).cloned()
}

/// Resolve a profile name, falling back to "minimal" if not found.
pub fn resolve_or_minimal(name: &str, custom_profiles: &[Profile]) -> Profile {
    resolve(name, custom_profiles).unwrap_or_else(minimal)
}

// ---------------------------------------------------------------------------
// Listing profiles
// ---------------------------------------------------------------------------

/// List all available profiles (built-in + custom from config) to stdout.
pub fn list_profiles(format: OutputFormat) -> anyhow::Result<()> {
    let project_dir = std::env::current_dir()?;
    let config = crate::config::load_config(&project_dir)?;

    let all_profiles: Vec<Profile> = builtins().into_iter().chain(config.profiles).collect();

    match format {
        OutputFormat::Human => {
            crate::output::info("Available Profiles");
            crate::output::info("==================");
            for p in &all_profiles {
                if p.extensions.is_empty() {
                    println!("  {} (no extensions)", p.name);
                } else {
                    println!("  {} [{}]", p.name, p.extensions.join(", "));
                }
            }
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&all_profiles)
                .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
            println!("{}", json);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordpress_profile_has_expected_extensions() {
        let p = wordpress();
        assert_eq!(p.name, "wordpress");
        assert!(p.extensions.contains(&"mysqli".to_string()));
        assert!(p.extensions.contains(&"gd".to_string()));
        assert_eq!(p.extensions.len(), 10);
    }

    #[test]
    fn laravel_profile_has_expected_extensions() {
        let p = laravel();
        assert_eq!(p.name, "laravel");
        assert!(p.extensions.contains(&"tokenizer".to_string()));
        assert_eq!(p.extensions.len(), 8);
    }

    #[test]
    fn minimal_profile_has_no_extensions() {
        let p = minimal();
        assert_eq!(p.name, "minimal");
        assert!(p.extensions.is_empty());
    }

    #[test]
    fn builtins_returns_three_profiles() {
        let profiles = builtins();
        assert_eq!(profiles.len(), 3);
        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"wordpress"));
        assert!(names.contains(&"laravel"));
        assert!(names.contains(&"minimal"));
    }

    #[test]
    fn builtin_lookup_works() {
        assert!(builtin("wordpress").is_some());
        assert!(builtin("laravel").is_some());
        assert!(builtin("minimal").is_some());
        assert!(builtin("drupal").is_none());
    }

    #[test]
    fn resolve_finds_builtin_first() {
        let custom = vec![Profile {
            name: "wordpress".to_string(),
            extensions: vec!["custom_ext".to_string()],
        }];
        let resolved = resolve("wordpress", &custom).unwrap();
        // Built-in takes priority, so custom wordpress is ignored.
        assert_eq!(resolved.extensions.len(), 10);
    }

    #[test]
    fn resolve_finds_custom_profile() {
        let custom = vec![Profile {
            name: "drupal".to_string(),
            extensions: vec!["curl".to_string(), "gd".to_string(), "mbstring".to_string()],
        }];
        let resolved = resolve("drupal", &custom).unwrap();
        assert_eq!(resolved.name, "drupal");
        assert_eq!(resolved.extensions.len(), 3);
    }

    #[test]
    fn resolve_returns_none_for_unknown() {
        let resolved = resolve("unknown", &[]);
        assert!(resolved.is_none());
    }

    #[test]
    fn resolve_or_minimal_falls_back() {
        let resolved = resolve_or_minimal("unknown", &[]);
        assert_eq!(resolved.name, "minimal");
    }

    #[test]
    fn profile_serialization_roundtrip() {
        let p = wordpress();
        let json = serde_json::to_string(&p).unwrap();
        let deserialized: Profile = serde_json::from_str(&json).unwrap();
        assert_eq!(p, deserialized);
    }

    #[test]
    fn profile_toml_roundtrip() {
        let p = Profile {
            name: "drupal".to_string(),
            extensions: vec!["curl".to_string(), "gd".to_string()],
        };
        let toml_str = toml::to_string(&p).unwrap();
        let deserialized: Profile = toml::from_str(&toml_str).unwrap();
        assert_eq!(p, deserialized);
    }
}
