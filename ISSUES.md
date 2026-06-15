# phpvm Issues

Testing notes from phpvm v0.1.1 on Linux x86_64 (static runtimes). Most items below
are resolved in the current branch; the live catalog is now manifest v3 (`catalog-2026-06-15`).

## Resolved bugs

### 1. Bundled profiles caused PHP startup warnings — fixed

Dynamic runtimes load extensions via `etc/conf.d/`. Static installs strip `extension=`
lines from the active `php.ini`. Re-apply a profile or reinstall to refresh an existing
runtime:

```bash
phpvm profile use laravel
# or
phpvm install 8.4 --profile=laravel
```

### 2. Exit codes not forwarded — fixed

`phpvm run` now exits with the child process status (e.g. `exit(42)` → `$?` is 42).

### 3. `phpvm ls` ignored config.toml — fixed

Persisted `current_version` from `phpvm use` wins over a stale `PHPVM_VERSION` env var.

## v3 dynamic catalog (live)

Remote manifest: `schema: "3.0"`, `catalog-2026-06-15`, published 2026-06-15.

| PHP | Targets | `runtime_type` |
|---|---|---|
| 8.1.34 | `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` | `dynamic` |
| 8.2.31 | `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` | `dynamic` |
| 8.3.31 | `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` | `dynamic` |
| 8.4.22 | `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` | `dynamic` |

Loadable bundled extensions now include **simplexml**, **session**, **iconv**, **opcache**,
**xdebug**, **sqlite3**, **pdo_sqlite**, **soap**, and **sockets** (26 `.so` modules per
version). Enable via profile presets or `phpvm ext enable <name>`.

**Migration:** existing static installs do not auto-upgrade. Reinstall each version to
pick up the v3 bundles:

```bash
phpvm install 8.4 --profile=laravel
```

Bundled `laravel` / `wordpress` presets in this repo include `session`, `simplexml`, and
`iconv`. The remote manifest profile templates still list the older extension sets; bundled
starters take precedence when seeding presets.

## Composer tooling (expected after v3 reinstall)

| Package | Notes |
|---|---|
| phpstan/phpstan | Works |
| squizlabs/php_codesniffer | Should work once simplexml is enabled via profile |
| vimeo/psalm | Should work once simplexml is enabled |
| phpunit/phpunit | Should work once session + simplexml + iconv are enabled |
| laravel/installer | `laravel new` should work once session is enabled |
| friendsofphp/php-cs-fixer | Still caps PHP 8.4 without `PHP_CS_FIXER_IGNORE_ENV=1` (upstream) |

Per-minor Composer home isolation (`~/.phpvm/composer-homes/8.4`, etc.) is intentional.

## Open observations

- **`phpvm matrix`** runs all installed runtimes; no `--versions` filter yet.
- **CLI-only** — no FPM/CGI/embed; not a web-server replacement for apt PHP.
- **Manual security updates** — run `phpvm install <patch>` when catalog publishes new patches.
- **Platform coverage** — v3 adds `aarch64-apple-darwin`; no Windows or older PHP (<8.1) yet.

## What works well

- Multi-version management and `phpvm matrix`
- Project pins (`.phpvm-version`, `.phpvm.toml`, `.phpvm/profiles/`)
- Shell integration (`eval "$(phpvm env)"`, `use_on_cd`)
- `etc/conf.d/` for dynamic runtimes (profiles, `phpvm ext`, custom snippets)