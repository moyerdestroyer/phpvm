# Manifest v2 — Full Binary + Profile Presets (static model)

PHPVM v2/v2.1 manifests describe **one minimal static runtime artifact per PHP version** (only `bin/php` + `bin/composer*`). Profiles are user-level `.ini` presets (tuning only). The compiled-in extension catalog is listed in the manifest and reported by `php -m`; profiles do not load `.so` files.

> Dynamic (loadable ext/ + runtime-owned etc/conf.d) catalogs are described by the legacy manifest v3 contract. See [manifest-v3.md](manifest-v3.md). New catalogs should be v2.1 static.

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

## On-disk layout (static model)

```text
# Project (committed)
my-app/.phpvm.toml              # profile = "wordpress" (default preset name)
my-app/.phpvm/profiles/
  wordpress.ini                 # full ini — team shares this (tuning only; no extension= needed)
  debug.ini

# Global (personal)
~/.phpvm/config.toml            # profile = "minimal" (optional default)
~/.phpvm/profiles/
  minimal.ini
  xdebug.ini

# Per installed runtime (minimal static tarball; phpvm adds only bookkeeping)
~/.phpvm/runtimes/8.3.31/
  bin/php
  bin/composer
  bin/composer.phar
  metadata.json                 # active_profile, catalog snapshot, etc. (phpvm only)
```

For static runtimes phpvm may also write a managed copy of the active preset to `~/.phpvm/ini/8.3.31.ini` (used via PHPRC for `phpvm use` / bare execution). The runtime tarball itself contains **only** `bin/`.

Legacy dynamic installs may still have `etc/`, `ext/`, etc. under their runtime dir; phpvm honors them when present but does not create them for new static artifacts.

Bundled starters ship in the phpvm repo under `profiles/` (`wordpress.ini`, `laravel.ini`, `minimal.ini`). For static they contain tuning directives only (extension lists live in the manifest `runtimes[].extensions`).

See also: [phpvm-runtimes publisher guide](./phpvm-runtimes.md) (companion repo layout, per-platform artifacts, catalog policy).

## Publisher checklist

- [ ] Build fat static PHP binaries with the full extension catalog per version
- [ ] Publish one tarball per PHP version (not per profile)
- [ ] List starter templates in manifest `profiles` (for names not covered by bundled `.ini` files)
- [ ] List compiled extensions in each runtime entry's `extensions`
- [ ] Verify checksums match published archives
