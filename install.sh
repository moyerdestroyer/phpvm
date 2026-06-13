#!/usr/bin/env bash
#
# phpvm installer
# Usage (recommended):
#   curl -fsSL https://raw.githubusercontent.com/moyerdestroyer/phpvm/master/install.sh | bash
#
# Advanced:
#   PHPVM_VERSION=0.1.0 PHPVM_INSTALL_DIR=$HOME/bin bash install.sh
#   PHPVM_UNINSTALL=1 bash install.sh
#
# The script downloads a prebuilt binary from GitHub Releases, verifies its
# checksum, and installs it into a user-writable directory (default ~/.local/bin).
# It deliberately avoids requiring root or any PHP/Rust toolchain on the host.
#
set -euo pipefail

REPO="moyerdestroyer/phpvm"
DEFAULT_INSTALL_DIR="${HOME}/.local/bin"
INSTALL_DIR="${PHPVM_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
VERSION="${PHPVM_VERSION:-}"
UNINSTALL="${PHPVM_UNINSTALL:-}"

# Optional hook for local/CI testing of the installer itself (not used in normal runs).
# If set, the script will fetch "${PHPVM_TEST_DOWNLOAD_BASE}/phpvm-${VER}-${TARGET}.tar.gz"
# and the matching .sha256 instead of constructing GitHub release URLs.
TEST_BASE="${PHPVM_TEST_DOWNLOAD_BASE:-}"

SUPPORTED_TARGETS=(
  "x86_64-apple-darwin"
  "aarch64-apple-darwin"
  "x86_64-unknown-linux-gnu"
)

# --- helpers -----------------------------------------------------------------

err() {
  echo "error: $*" >&2
  exit 1
}

info() {
  echo "$*"
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

# Normalize uname output to our TARGET triples.
detect_target() {
  local os arch

  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m | tr '[:upper:]' '[:lower:]')"

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *) err "Unsupported architecture: $arch (supported: x86_64, aarch64)" ;;
  esac

  case "$os" in
    darwin)
      echo "${arch}-apple-darwin"
      ;;
    linux)
      echo "${arch}-unknown-linux-gnu"
      ;;
    *)
      err "Unsupported OS: $os (supported: darwin, linux)"
      ;;
  esac
}

# Return the tag (e.g. v0.1.0). If VERSION provided, ensure it has a leading v for the tag.
resolve_tag() {
  local v="$1"
  if [[ -z "$v" ]]; then
    if [[ -n "${TEST_BASE}" ]]; then
      err "When using PHPVM_TEST_DOWNLOAD_BASE you must also set PHPVM_VERSION (the script will not call the GitHub API in test mode)."
    fi
    # Discover latest from GitHub API (no jq dependency).
    local api_url="https://api.github.com/repos/${REPO}/releases/latest"
    local tag
    # Some environments have very old curl; --fail --silent --show-error keeps output clean on error.
    tag="$(curl -fsSL "$api_url" | grep -o '"tag_name": *"[^"]*"' | head -n1 | cut -d'"' -f4 || true)"
    if [[ -z "$tag" ]]; then
      err "Failed to determine latest version from GitHub. Set PHPVM_VERSION explicitly or check https://github.com/${REPO}/releases"
    fi
    echo "$tag"
  else
    # Accept either "0.1.0" or "v0.1.0"
    if [[ "$v" =~ ^v ]]; then
      echo "$v"
    else
      echo "v${v}"
    fi
  fi
}

# Compute the "bare" version used inside archive names (no leading v).
bare_version() {
  local tag="$1"
  echo "${tag#v}"
}

# Build the download URLs (or local paths when TEST_BASE is active) for a given tag + target.
asset_urls() {
  local tag="$1"
  local target="$2"
  local ver
  ver="$(bare_version "$tag")"

  if [[ -n "$TEST_BASE" ]]; then
    # Testing hook (used by `cargo test` style verification and CI of the installer).
    # Caller must also supply PHPVM_VERSION. The values are treated as filesystem paths.
    echo "${TEST_BASE}/phpvm-${ver}-${target}.tar.gz"
    echo "${TEST_BASE}/phpvm-${ver}-${target}.tar.gz.sha256"
    return
  fi

  local base="https://github.com/${REPO}/releases/download/${tag}"
  echo "${base}/phpvm-${ver}-${target}.tar.gz"
  echo "${base}/phpvm-${ver}-${target}.tar.gz.sha256"
}

# Fetch a remote URL or local file into the destination path.
# This lets the TEST_BASE hook work without requiring a web server or file:// URLs for curl.
fetch_to() {
  local src="$1"
  local dest="$2"
  if [[ "$src" == /* ]]; then
    # Local path (TEST_BASE)
    cp -f "$src" "$dest"
  else
    curl -fsSL --output "$dest" "$src"
  fi
}

# Verify a downloaded archive against its .sha256 file.
# Works on both GNU (sha256sum) and BSD/macOS (shasum).
verify_checksum() {
  local archive="$1"
  local sumfile="$2"
  local dir
  dir="$(dirname "$archive")"

  pushd "$dir" >/dev/null
  if have_cmd sha256sum; then
    sha256sum -c "$(basename "$sumfile")" >/dev/null
  elif have_cmd shasum; then
    shasum -a 256 -c "$(basename "$sumfile")" >/dev/null
  else
    err "Neither sha256sum nor shasum found. Cannot verify download integrity."
  fi
  popd >/dev/null
}

# Extract the single binary from the tarball (assumes archive root contains "phpvm").
extract_binary() {
  local archive="$1"
  local outdir="$2"

  while IFS= read -r member; do
    if [[ "$member" == /* ]] || [[ "$member" == *".."* ]]; then
      err "Archive contains unsafe path: ${member}"
    fi
  done < <(tar -tzf "$archive")

  tar -xzf "$archive" -C "$outdir"
}

install_binary() {
  local src="$1"
  local dest_dir="$2"
  local dest="${dest_dir}/phpvm"

  mkdir -p "$dest_dir"
  mv "$src" "$dest"
  chmod +x "$dest"
  echo "$dest"
}

is_in_path() {
  local dir="$1"
  case ":${PATH}:" in
    *":${dir}:"*) return 0 ;;
    *) return 1 ;;
  esac
}

print_path_hint() {
  local dir="$1"
  info ""
  info "NOTE: ${dir} is not in your \$PATH."
  info "Add it to your shell profile (e.g. ~/.bashrc, ~/.zshrc, ~/.profile):"
  info ""
  info "    export PATH=\"${dir}:\$PATH\""
  info ""
  info "Then restart your shell or run: source <your-profile>"
  info ""
}

do_uninstall() {
  local bin="${INSTALL_DIR}/phpvm"
  if [[ -f "$bin" ]]; then
    rm -f "$bin"
    info "Removed $bin"
  else
    info "phpvm not found at $bin (nothing to uninstall)"
  fi
  exit 0
}

# --- main --------------------------------------------------------------------

main() {
  if [[ -n "$UNINSTALL" && "$UNINSTALL" != "0" ]]; then
    do_uninstall
  fi

  local target
  target="$(detect_target)"

  # Validate we are producing a target we actually release for.
  local supported=0
  for t in "${SUPPORTED_TARGETS[@]}"; do
    if [[ "$t" == "$target" ]]; then supported=1; break; fi
  done
  if [[ $supported -eq 0 ]]; then
    if [[ "$target" == "aarch64-unknown-linux-gnu" ]]; then
      err "Linux arm64 (aarch64) is not supported yet. Published targets: ${SUPPORTED_TARGETS[*]}. See https://github.com/${REPO}/releases for manual downloads."
    fi
    err "Detected target '${target}' is not yet supported by releases. Supported: ${SUPPORTED_TARGETS[*]}. See https://github.com/${REPO}/releases for manual downloads."
  fi

  local tag
  tag="$(resolve_tag "$VERSION")"
  local ver
  ver="$(bare_version "$tag")"

  info "Installing phpvm ${ver} for ${target} into ${INSTALL_DIR}"

  local urls archive_url sum_url
  urls="$(asset_urls "$tag" "$target")"
  archive_url="$(echo "$urls" | head -n1)"
  sum_url="$(echo "$urls" | tail -n1)"

  local tmp
  tmp="$(mktemp -d)"
  # Defensive: with set -u an early exit before tmp= would otherwise make the EXIT trap fail.
  trap '[[ -n "${tmp:-}" ]] && rm -rf "$tmp" || true' EXIT

  local archive="${tmp}/phpvm-${ver}-${target}.tar.gz"
  local sumfile="${tmp}/phpvm-${ver}-${target}.tar.gz.sha256"

  info "Fetching ${archive_url}"
  fetch_to "$archive_url" "$archive" || err "Download failed: ${archive_url}"
  info "Fetching checksum ${sum_url}"
  fetch_to "$sum_url" "$sumfile" || err "Checksum download failed: ${sum_url}"

  info "Verifying checksum"
  verify_checksum "$archive" "$sumfile"

  info "Extracting"
  extract_binary "$archive" "$tmp"

  local binary_src="${tmp}/phpvm"
  if [[ ! -f "$binary_src" ]]; then
    # Some archives may nest; try to find it.
    binary_src="$(find "$tmp" -type f -name phpvm | head -n1 || true)"
    if [[ -z "$binary_src" ]]; then
      err "Archive did not contain a 'phpvm' binary. The release may be malformed."
    fi
  fi

  local installed
  installed="$(install_binary "$binary_src" "$INSTALL_DIR")"

  info "Installed: $installed"
  if "$installed" --version >/dev/null 2>&1; then
    "$installed" --version
  fi

  if ! is_in_path "$INSTALL_DIR"; then
    print_path_hint "$INSTALL_DIR"
  else
    info ""
    info "phpvm is on your PATH. Try: phpvm --help"
    info ""
  fi

  info "To update later, re-run the same curl | bash command (or set PHPVM_VERSION)."
  info "To uninstall: PHPVM_UNINSTALL=1 bash install.sh  (or simply rm ${INSTALL_DIR}/phpvm)"
}

main "$@"
