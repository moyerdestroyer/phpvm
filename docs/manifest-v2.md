# Manifest v2 — Full Binary + Profile Presets

PHPVM v2 manifests describe **one full runtime artifact per PHP version**. Profiles are ini presets applied after install — the authoritative user config is `.ini` files, not manifest extension lists.

## Schema

```json
{
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
      "sha256": "..."
    }
  ]
}
```

## Rules

1. **`runtimes`** — exactly one entry per PHP patch version.
2. **`runtimes[].extensions`** — authoritative catalog of extensions compiled into the binary.
3. **`profiles`** — starter templates only; used to seed a `.ini` file when no bundled/user preset exists. Must not overwrite existing preset files.
4. **No `profile` field on runtime entries** — profiles are not separate download artifacts.

## v1 compatibility

Legacy manifests with per-profile runtime entries are normalized on parse:

- Multiple entries for the same PHP version are merged into one.
- Extension catalogs are synthesized from built-in profile definitions when missing.

Publishers should migrate to v2 full binaries to enable instant `phpvm profile use` switching.

## On-disk layout

```text
# Project (committed)
my-app/.phpvm.toml              # profile = "wordpress" (default preset name)
my-app/.phpvm/profiles/
  wordpress.ini                 # full ini — team shares this
  debug.ini

# Global (personal)
~/.phpvm/config.toml            # profile = "minimal" (optional default)
~/.phpvm/profiles/
  minimal.ini
  xdebug.ini

# Per installed runtime
~/.phpvm/runtimes/8.3.23/
  bin/php, composer
  etc/php.ini                   # active config (copy of chosen preset)
  etc/profiles/                 # optional runtime-local overrides
  metadata.json                 # active_profile, preset_source, enabled_extensions
```

Bundled starters ship in the phpvm repo under `profiles/` (`wordpress.ini`, `laravel.ini`, `minimal.ini`).

See also: [phpvm-runtimes publisher guide](./phpvm-runtimes.md) (companion repo layout, per-platform artifacts, catalog policy).

## Publisher checklist

- [ ] Build fat static PHP binaries with the full extension catalog per version
- [ ] Publish one tarball per PHP version (not per profile)
- [ ] List starter templates in manifest `profiles` (for names not covered by bundled `.ini` files)
- [ ] List compiled extensions in each runtime entry's `extensions`
- [ ] Verify checksums match published archives
