# PHPVM — Code Quality & Contribution Guide

This document describes the standards and conventions for contributing to PHPVM. Read it carefully before submitting a PR.

---

## Required Checks

Every PR must pass all three gates before merge:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

No warnings. No formatting drift. No failing tests.

We plan to add `cargo nextest run` in the future. Get comfortable with it now if you can.

---

## Architecture

PHPVM is a single-crate CLI. Modules live in `src/` and are organized by domain:

| Module | Purpose |
|---|---|
| `cli` | Command-line argument parsing (clap derive) |
| `config` | Configuration loading (`.phpvm.toml`, global config) |
| `doctor` | Project inspection and recommendations |
| `install` | Runtime installation orchestration |
| `manifest` | Remote manifest fetching and parsing |
| `matrix` | Multi-runtime command execution |
| `output` | Terminal output formatting and styling |
| `profile` | Profile preset commands and manifest template seeding |
| `profile_preset` | Ini preset resolution, materialization, and activation |
| `runner` | Single-runtime command execution |
| `version` | PHP version resolution and listing |
| `providers/` | Runtime installation backends (static_php, docker, local) |

### Primary User Workflows

PHPVM exists primarily to support:

1. `phpvm release-check`
2. `phpvm doctor`
3. `phpvm matrix <command>`
4. `phpvm run <version> <command>`

When making design decisions, optimize for these workflows first.

### Key Principles

1. **Host Independence.** If a PHPVM command behaves differently because of host-installed software, that's a bug. Never rely on host PHP, host Composer, or host extensions in production code. Tests may use host tooling when necessary for bootstrapping or fixture generation.

2. **The Manifest is Truth.** Runtime download URLs and checksums come from the remote manifest. Never hardcode URLs.

3. **Explicit Over Magic.** The selected runtime should always be visible and predictable. Users should always know which PHP version, Composer version, extension profile, and runtime source they're using.

4. **Reproducibility.** The same runtime should behave identically on every machine.

### Providers

Providers implement a common interface. The rest of the application should not know whether a runtime came from `static_php`, `docker`, or `local`. Provider-specific behavior stays inside the provider implementation.

### Profiles

Profiles are **named `.ini` preset files** — full php.ini configs (extensions, memory limits, opcache, xdebug, etc.). One full binary is installed per PHP version; switching profiles copies a preset to the active `etc/php.ini`.

**Resolution order** (first match wins):

1. `<project>/.phpvm/profiles/<name>.ini`
2. `~/.phpvm/profiles/<name>.ini`
3. `~/.phpvm/runtimes/<version>/etc/profiles/<name>.ini`
4. Bundled starter (materialized to global **only if missing**)

Built-in starters: **wordpress**, **laravel**, **minimal** (shipped in `profiles/`; manifest templates are a fallback when no bundled file exists).

```toml
# .phpvm.toml — default preset name only
profile = "wordpress"
```

```text
# Project presets (commit these)
my-app/.phpvm/profiles/wordpress.ini
my-app/.phpvm/profiles/debug.ini
```

Commands:

- `phpvm install 8.3 --profile=wordpress` — download once, activate initial preset
- `phpvm profile use laravel` — switch active runtime's ini preset
- `phpvm profile list` / `phpvm profiles` — list presets and source paths
- `phpvm profile path [name]` — print resolved preset path (for editors/CI)
- `phpvm profile edit [name]` — open preset in `$EDITOR`
- `phpvm profile new <name> [--from <template>] [--global]`
- `phpvm profile fork <src> <dst>`
- `phpvm use 8.3 --profile=laravel` — version activation + profile in one step

phpvm **never overwrites** an existing project/global/runtime preset file.

### Manifest Contract

The remote manifest is the source of truth.

The manifest controls:

- Available PHP versions (one full binary per version)
- Extension catalog compiled into each binary
- Named profile templates for starter ini seeding (authoritative config is `.ini` files)
- Composer versions
- Download URLs
- Checksums

See `docs/manifest-v2.md` for the publishing contract.

The CLI should never hardcode runtime download URLs. Changes to distribution infrastructure should be possible without releasing a new CLI version.

### Output

User-facing output belongs in `output.rs`. Avoid scattered `println!()` calls throughout the codebase. Commands should return structured data where practical. Formatting should be centralized.

This matters because PHPVM will support machine-readable output:

```bash
phpvm matrix --report=json
phpvm doctor --json
```

### Future Compatibility

The architecture should support future commands such as:

```bash
phpvm release-check
phpvm wp release-check
phpvm wp doctor
phpvm verify
```

without requiring significant redesign.

---

## Rust Conventions

### Edition & Toolchain

- Rust stable only. No nightly features.
- Edition 2021.
- Max line width: 100 characters.

### Error Handling

- Use `anyhow::Result` for application code (commands, orchestration).
- Use `thiserror` for library-style error types that need structured variants.
- Never unwrap in non-test code. Use `?`, `context()`, or explicit error handling.
- Error messages should be actionable: tell the user what went wrong and what to do about it.

### Naming

- Modules: lowercase, single-word where possible (`cli`, `config`, `doctor`).
- Types: `PascalCase`.
- Functions: `snake_case`.
- Constants: `SCREAMING_SNAKE_CASE`.
- Booleans: prefix with `is_`, `has_`, `can_`, `should_`.

### Imports

- Group imports in this order, separated by blank lines:
  1. Standard library (`std::*`)
  2. External crates (`anyhow::*`, `clap::*`, etc.)
  3. Internal modules (`crate::*`, `super::*`)
- Use `use` granularly. Prefer `use crate::config::Config` over `use crate::config::*` except in test files.

### Types

- Use `camino::Utf8PathBuf` for file paths in application code. It avoids `OsStr` pain.
- Use `std::path::Path` only when interfacing with APIs that require it.
- Prefer `String` over `&str` in struct fields. Lifetimes add complexity; the runtime cost is negligible here.

### Testing

- **Unit tests** live in the same file as the code they test, in a `#[cfg(test)] mod tests` block.
- **Integration tests** live in `tests/`. Use `assert_cmd` for CLI integration tests.
- **Snapshot tests** use `insta`. Always review snapshots before committing.
- **Fixtures** live in `fixtures/`. Each fixture is a minimal project (WordPress plugin, Laravel app, Composer library) used for project detection testing.

### Testability

Core business logic should be separated from CLI code. Prefer:

- `parse` — interpret input
- `validate` — check constraints
- `resolve` — determine what to do
- `execute` — do it

as independent functions that can be unit tested. Avoid embedding business logic directly inside clap command handlers.

### What to Test First

Priority order for test coverage:

1. Version resolution (`version.rs`) — parsing and resolving specifiers like `8.3`, `8.3.latest`, `8.3.12`
2. Config parsing (`config.rs`) — loading `.phpvm.toml` files
3. Manifest handling (`manifest.rs`) — parsing and validating manifest entries
4. CLI argument parsing (`cli.rs`) — all subcommands and flags
5. Output formatting (`output.rs`) — snapshot tests for terminal output

---

## Git Conventions

### Commits

- Write clear, imperative commit messages: `add version resolution logic` not `added version resolution logic`.
- Keep commits focused. One logical change per commit.
- Don't commit generated files or secrets.

### PRs

- PRs must pass all required checks (`fmt`, `clippy`, `test`).
- Include tests for new functionality.
- Update documentation if you change behavior.
- Keep PRs small and reviewable. If a PR needs more than 400 lines of explanation, split it.

### Releasing

Packaging and distribution is handled by GitHub Releases + a small installer script (see the approved packaging plan and `.github/workflows/release.yml` + `install.sh`).

Typical release steps:

1. Update `version` in `Cargo.toml` (and any copy in docs if still present).
2. Run the three gates locally and fix anything.
3. Commit the version bump (message e.g. `release: v0.2.0`).
4. `git tag -a v0.2.0 -m "0.2.0 — <very short summary>"`
5. `git push origin v0.2.0`
6. Watch the "release" workflow in GitHub Actions. It will:
   - Build stripped binaries + checksums for the supported targets
   - Create a **draft** release
   - Attach the archives, `.sha256` files, and a pinned copy of `install.sh`
7. Review the draft release notes and the attached assets, then **Publish release** (required — draft releases are invisible to `install.sh`'s `/releases/latest` lookup and asset downloads fail until published).
8. Smoke test after publishing: `curl .../install.sh | bash`, `phpvm install`, `phpvm run`.

Never push a tag until the PR that contains the version bump (and any user-facing changes) has passed CI and been merged to `master`.

The `ci.yml` workflow runs the required gates on every PR/push, so a green `master` + passing local run is the signal that it is safe to tag.

---

## File Layout

```
src/
├── main.rs          # Entry point, CLI dispatch
├── cli.rs           # Clap argument definitions
├── config.rs        # Config loading and types
├── doctor.rs        # Project inspection
├── install.rs       # Runtime installation
├── manifest.rs      # Remote manifest handling
├── matrix.rs        # Multi-runtime execution
├── output.rs        # Terminal output formatting
├── profile.rs       # Profile preset CLI and manifest templates
├── profile_preset.rs # Ini preset resolution and activation
├── runtime_metadata.rs # Installed runtime metadata + ini paths
├── runner.rs        # Single-runtime command execution
├── version.rs       # Version resolution and listing
└── providers/
    ├── mod.rs        # Provider trait definition
    ├── static_php.rs # Prebuilt binary provider (V1)
    ├── docker.rs     # Docker provider (future)
    └── local.rs      # Host PHP provider (development only)

fixtures/
├── wordpress-plugin/ # Minimal WordPress plugin fixture
├── laravel-app/      # Minimal Laravel app fixture
└── composer-library/ # Minimal Composer library fixture

tests/
└── cli.rs           # CLI integration tests
```

---

## Design Decisions

### Why single-crate?

PHPVM is a CLI tool. Single-crate keeps builds fast and dependencies simple. If modules grow complex enough to warrant separation, we'll extract them then — not before.

### Why `camino`?

`std::path::Path` uses `OsStr`, which is painful on Windows and doesn't support string operations. `camino::Utf8PathBuf` gives us `str`-based paths with zero runtime cost.

### Why `anyhow` + `thiserror`?

`anyhow` for the application boundary (commands, main). `thiserror` for structured error types that callers need to match on. This is the standard Rust CLI pattern.

### Why prebuilt binaries only in V1?

Compiling PHP locally is slow, fragile, and environment-dependent. V1 uses prebuilt/static binaries to guarantee reproducibility and fast installs. Local compilation may come in V2.

---

## Things to Avoid

- **Don't** call host PHP or host Composer in production code. Always use the runtime's binaries. Tests may use host tooling when necessary for bootstrapping or fixture generation.
- **Don't** hardcode download URLs. Always use the manifest.
- **Don't** manage web servers, PHP-FPM, nginx, or Apache. That's out of scope.
- **Don't** compile PHP locally in V1.
- **Don't** normalize dependency resolution across PHP versions. Different PHP versions may legitimately produce different `composer.lock` files.
- **Don't** add dependencies without justification. Every crate in `Cargo.toml` should earn its place.