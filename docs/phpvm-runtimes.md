# phpvm-runtimes — Publisher Guide

This document defines the **phpvm-runtimes** companion repository: how to lay out builds, name release assets, and publish `manifest.json` so the [phpvm](https://github.com/moyerdestroyer/phpvm) CLI can install PHP on **Linux x86_64** and **macOS Apple Silicon**. Intel Mac (`x86_64-apple-darwin`) is planned but not published yet.

The **phpvm** repo ships the CLI. **phpvm-runtimes** ships PHP+Composer trees and the manifest. Do not mix the two in one GitHub Release.

---

## Catalog policy

| Rule | Value |
|---|---|
| PHP minor lines published | **4** (e.g. 8.1, 8.2, 8.3, 8.4 — adjust over time) |
| Patches per minor | **Latest only** (one exact version per line, e.g. `8.3.23`) |
| Platforms per version | **2** today (`linux-gnu`, `aarch64-apple-darwin`); **3** when Intel Mac ships |
| Total remote tarballs | **8** today (4 versions × 2 platforms); **12** at full matrix |
| Older patches | **Not hosted**; remain usable only if already installed under `~/.phpvm/runtimes/<version>/` |

When PHP `8.3.24` replaces `8.3.23`, update the manifest entry and replace the three `8.3.24` assets. Delete or expire `8.3.23` assets to save space.

Users install with specifiers like `8.3`, `8.3.latest`, or the exact patch `8.3.23`. Exact patches not in the manifest fail fresh install by design.

---

## Supported target triples

Must match phpvm’s installer and runtime resolution (same strings as `install.sh` / Rust targets):

| Target | OS / CPU |
|---|---|
| `x86_64-unknown-linux-gnu` | Linux x86_64 (glibc; built on Ubuntu 22.04+ class runners) |
| `x86_64-apple-darwin` | macOS Intel *(planned — not in catalog yet)* |
| `aarch64-apple-darwin` | macOS Apple Silicon |

**Not in v1 catalog:** Linux ARM64 (`aarch64-unknown-linux-gnu`), Windows.

---

## Repository layout

```text
phpvm-runtimes/
├── README.md
├── manifest.json                 # Source of truth (also attached to releases)
├── AGENTS.md                     # Optional: your build/publish checklist
│
├── profiles/                     # Optional duplicates of phpvm starters (usually omit;
│                                 # phpvm bundles wordpress/laravel/minimal.ini)
│
├── builds/                       # Build recipes (not the binaries themselves)
│   ├── common/
│   │   ├── extensions.json       # Shared extension list for “full” builds
│   │   └── composer-version.txt  # e.g. 2.9.2
│   ├── 8.3.23/
│   │   ├── static-php-cli.json   # Per-version SPC config (or shell driver)
│   │   └── notes.md              # glibc floor, known quirks
│   └── …
│
├── scripts/
│   ├── package-runtime.sh        # Validate tree → tar.gz + sha256
│   ├── update-manifest.py        # Inject urls/checksums into manifest.json
│   └── verify-manifest.sh        # Schema + HTTPS + 64-char sha256
│
└── .github/
    └── workflows/
        ├── build-runtime.yml       # matrix: 3 targets × 1 version (manual dispatch)
        └── publish-catalog.yml     # Bump manifest, create GitHub Release
```

**Do not commit** multi-hundred-MB tarballs to git. Binaries live only on **GitHub Releases** (or object storage later).

---

## Runtime tarball layout

Each archive is a **gzip tarball** with a **single top-level directory** (stripped on extract by phpvm). After extraction, phpvm requires:

```text
php-8.3.23-x86_64-unknown-linux-gnu/    # top-level dir (any single segment name is fine)
└── bin/
    ├── php                             # executable
    └── composer                          # executable (PHAR or binary wrapper)
```

phpvm does **not** require `etc/` inside the tarball; it creates `etc/php.ini` and `metadata.json` on first `phpvm install` / profile activation.

### Packaging rules

- Format: `.tar.gz`
- Root contains **only** `bin/php` and `bin/composer` (plus anything those need at runtime, e.g. `lib/` if your static build ships shared libs — prefer fully static when possible)
- No symlinks pointing outside the archive (phpvm rejects unsafe tar entries)
- Run `package-runtime.sh` to produce **sidecar checksum**:

```text
php-8.3.23-x86_64-unknown-linux-gnu.tar.gz
php-8.3.23-x86_64-unknown-linux-gnu.tar.gz.sha256   # optional for humans; manifest carries sha256
```

Checksum in manifest: **lowercase hex, 64 characters**, of the `.tar.gz` file bytes.

---

## Release asset naming

**Required pattern** (phpvm resolves downloads from manifest URLs; names must be stable and predictable):

```text
php-{PHP_VERSION}-{TARGET}.tar.gz
```

Examples:

```text
php-8.3.23-x86_64-unknown-linux-gnu.tar.gz
php-8.3.23-x86_64-apple-darwin.tar.gz
php-8.3.23-aarch64-apple-darwin.tar.gz
```

### GitHub Releases strategy (recommended)

Use a **catalog release** per publish (not one release per PHP patch forever):

| Approach | Tag example | Assets |
|---|---|---|
| **Rolling catalog** (simplest) | `catalog` or `catalog-2025-06-12` | All 12 tarballs + `manifest.json` |
| Per PHP version | `php-8.3.23` | 3 tarballs for that version only |

**Recommended for v1:** tag `catalog-YYYY-MM-DD`, attach **all current tarballs + `manifest.json`**, set manifest URLs to:

```text
https://github.com/<org>/phpvm-runtimes/releases/download/catalog-2025-06-12/php-8.3.23-x86_64-unknown-linux-gnu.tar.gz
```

Manifest is also served from the repo for review/diff:

```text
https://raw.githubusercontent.com/<org>/phpvm-runtimes/master/manifest.json
```

phpvm fetches this catalog by default (`DEFAULT_MANIFEST_URL`). Override for forks, staging, or air-gapped mirrors:

```toml
manifest_url = "https://raw.githubusercontent.com/<org>/phpvm-runtimes/master/manifest.json"
```

---

## Manifest schema (v2.1 — per-platform artifacts)

Manifest v2 in phpvm assumed **one URL per PHP version**. Multi-platform catalogs need an **`artifacts`** map keyed by target triple.

### Top-level shape

```json
{
  "schema": "2.1",
  "published_at": "2025-06-12T00:00:00Z",
  "catalog_tag": "catalog-2025-06-12",
  "profiles": [ … ],
  "runtimes": [ … ]
}
```

- **`profiles`** — optional starter templates (extension lists). phpvm’s bundled `.ini` files take precedence; manifest templates are fallback seeding only.
- **`runtimes`** — **exactly four entries**, one per supported minor line (latest patch).

### Runtime entry

```json
{
  "php": "8.3.23",
  "composer": "2.9.2",
  "extensions": [
    "curl", "dom", "gd", "intl", "mbstring", "mysqli", "openssl",
    "pdo_mysql", "tokenizer", "xml", "zip"
  ],
  "artifacts": {
    "x86_64-unknown-linux-gnu": {
      "url": "https://github.com/.../php-8.3.23-x86_64-unknown-linux-gnu.tar.gz",
      "sha256": "abcdef0123456789..."
    },
    "x86_64-apple-darwin": {
      "url": "https://github.com/.../php-8.3.23-x86_64-apple-darwin.tar.gz",
      "sha256": "..."
    },
    "aarch64-apple-darwin": {
      "url": "https://github.com/.../php-8.3.23-aarch64-apple-darwin.tar.gz",
      "sha256": "..."
    }
  }
}
```

| Field | Required | Notes |
|---|---|---|
| `php` | yes | Exact semver `MAJOR.MINOR.PATCH` |
| `composer` | yes | Bundled Composer version string |
| `extensions` | yes | Full catalog compiled into **all** platform builds for this version (same list across targets) |
| `artifacts` | yes | Map of target triple → `{ url, sha256 }` |
| `url` / `sha256` (top-level) | no | Legacy v2; do not use in new catalogs |

### phpvm consumer behavior (contract)

On `phpvm install <spec>`:

1. Resolve specifier against manifest `runtimes[].php` (e.g. `8.3.latest` → `8.3.23`).
2. Detect host target triple (same logic as `install.sh`).
3. Select `artifacts[target]`.
4. Download, verify `sha256`, extract, apply profile preset.

If the host triple is missing from `artifacts`, fail with a clear error (e.g. Linux ARM not published).

> **Note:** phpvm `master` still reads legacy single `url`/`sha256`. Implement v2.1 `artifacts` support in phpvm before publishing this catalog (tracked in phpvm repo).

### Example full manifest (minimal)

```json
{
  "schema": "2.1",
  "published_at": "2025-06-12T00:00:00Z",
  "catalog_tag": "catalog-2025-06-12",
  "profiles": [
    {
      "name": "wordpress",
      "extensions": ["curl", "dom", "gd", "intl", "mbstring", "mysqli", "openssl", "pdo_mysql", "xml", "zip"]
    },
    {
      "name": "laravel",
      "extensions": ["curl", "intl", "mbstring", "openssl", "pdo_mysql", "tokenizer", "xml", "zip"]
    },
    { "name": "minimal", "extensions": [] }
  ],
  "runtimes": [
    {
      "php": "8.1.33",
      "composer": "2.8.9",
      "extensions": ["curl", "mbstring", "openssl", "xml", "zip"],
      "artifacts": {
        "x86_64-unknown-linux-gnu": { "url": "…", "sha256": "…" },
        "x86_64-apple-darwin": { "url": "…", "sha256": "…" },
        "aarch64-apple-darwin": { "url": "…", "sha256": "…" }
      }
    },
    {
      "php": "8.2.29",
      "composer": "2.9.2",
      "extensions": ["…"],
      "artifacts": { "…": { "url": "…", "sha256": "…" } }
    },
    {
      "php": "8.3.23",
      "composer": "2.9.2",
      "extensions": ["…"],
      "artifacts": { "…": { "url": "…", "sha256": "…" } }
    },
    {
      "php": "8.4.8",
      "composer": "2.9.2",
      "extensions": ["…"],
      "artifacts": { "…": { "url": "…", "sha256": "…" } }
    }
  ]
}
```

Extension lists should match what you actually compile into the static build for every platform build of that PHP version.

---

## Build matrix (what to produce)

For each catalog publish, build **12 artifacts**:

| PHP | Linux x86_64 | macOS Intel | macOS ARM |
|---|---|---|---|
| 8.1.x (latest) | ✓ | ✓ | ✓ |
| 8.2.x (latest) | ✓ | ✓ | ✓ |
| 8.3.x (latest) | ✓ | ✓ | ✓ |
| 8.4.x (latest) | ✓ | ✓ | ✓ |

Suggested tooling: [static-php-cli](https://github.com/crazywhalecc/static-php-cli) with a shared extension set per PHP minor.

| Target | Where to build |
|---|---|
| `x86_64-unknown-linux-gnu` | GitHub Actions `ubuntu-22.04` or local Linux |
| `x86_64-apple-darwin` | GitHub Actions `macos-latest` (cross-compile from Apple Silicon) |
| `aarch64-apple-darwin` | GitHub Actions `macos-latest` |

Document minimum OS/glibc/macOS versions in `phpvm-runtimes/README.md` (e.g. “Linux: glibc 2.35+”, “macOS 12+”).

---

## Publish checklist

1. **Build** all 12 tarballs for the new catalog.
2. **Verify** each tarball: `bin/php -v`, `bin/composer -V`, `php -m` covers manifest `extensions`.
3. **Compute** SHA-256 for each `.tar.gz`.
4. **Update** `manifest.json` (four `runtimes`, three `artifacts` each).
5. **Run** `scripts/verify-manifest.sh`.
6. **Create** GitHub Release `catalog-YYYY-MM-DD`; upload 12 tarballs + `manifest.json`.
7. **Commit** `manifest.json` to `master` on phpvm-runtimes.
8. **Smoke test** on each OS:
   ```bash
   phpvm install 8.3
   phpvm run 8.3 php -v
   phpvm profile use wordpress
   phpvm doctor
   ```
9. **Prune** previous catalog release assets if you need GitHub storage headroom (optional; old local installs unaffected).

---

## Rotation example

**Before:** manifest lists `8.3.23` with three artifacts.

**After PHP 8.3.24 ships:**

1. Build three new `8.3.24` tarballs.
2. Replace the single `runtimes[]` row: `php` `8.3.23` → `8.3.24`, new urls/checksums.
3. Publish new catalog release; remove `8.3.23` assets when convenient.
4. Users with `~/.phpvm/runtimes/8.3.23/` keep working; `phpvm install 8.3.23` on a new machine fails unless they use a custom manifest mirror.

---

## Validation rules (enforced by phpvm)

These match phpvm’s manifest parser today and planned v2.1 support:

- `url` must be `https://`
- `sha256` must be 64 hex characters (case-insensitive at verify time)
- Exactly **one** runtime row per `php` version string
- No conflicting URLs for the same `php` + target
- `extensions` must reflect the built binary (doctor/profile switching depends on catalog truth)

---

## Relationship to phpvm CLI releases

| Repo | Ships | Release trigger |
|---|---|---|
| `phpvm` | `phpvm` binary, `install.sh` | Tag `v0.1.0` |
| `phpvm-runtimes` | PHP runtimes + `manifest.json` | Tag `catalog-…` or manual release |

End-user flow:

```bash
# 1. Install CLI (once)
curl -fsSL https://raw.githubusercontent.com/.../phpvm/master/install.sh | bash

# 2. Install a runtime (manifest fetched from phpvm-runtimes by default)
phpvm install 8.3   # host target selects linux-gnu or aarch64-apple-darwin artifact
```

---

## Next steps

1. Create empty `phpvm-runtimes` repo with this layout.
2. Implement manifest **v2.1 `artifacts`** parsing in phpvm (`src/manifest.rs` + `static_php` download path).
3. Build **one** version end-to-end (e.g. `8.3.23` × Linux only) as a smoke test, then fill the 12-artifact matrix.
4. ~~Point `DEFAULT_MANIFEST_URL` at the catalog~~ — done; phpvm defaults to the raw GitHub manifest.

See also: [manifest-v2.md](./manifest-v2.md) (profile presets and installed runtime layout).
