#!/usr/bin/env bash
#
# phpvm installer
# Usage (recommended):
#   curl -fsSL https://raw.githubusercontent.com/moyerdestroyer/phpvm/master/install.sh | bash
#
# Advanced:
#   PHPVM_VERSION=0.1.0 PHPVM_INSTALL_DIR=$HOME/bin bash install.sh
#   PHPVM_UNINSTALL=1 bash install.sh
#   PHPVM_MODIFY_SHELL=1 curl ... | bash  # add shell integration to rc without prompting
#   PHPVM_MODIFY_SHELL=0 curl ... | bash  # never modify shell rc (manual hint only)
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
MODIFY_SHELL="${PHPVM_MODIFY_SHELL:-${PHPVM_MODIFY_PATH:-}}"

SHELL_MARKER_BEGIN="# phpvm installer: enable shell integration (begin)"
SHELL_MARKER_END="# phpvm installer: enable shell integration (end)"

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

shell_quote() {
  local value="$1"
  printf "'%s'" "$(printf "%s" "$value" | sed "s/'/'\\\\''/g")"
}

# Pick the shell rc file most users of this shell will actually load.
detect_shell_rc() {
  if [[ "${PROFILE:-}" == "/dev/null" ]]; then
    return 0
  fi
  if [[ -n "${PROFILE:-}" && -f "${PROFILE}" ]]; then
    echo "${PROFILE}"
    return 0
  fi

  local shell_name="${SHELL##*/}"
  local detected=""

  case "$shell_name" in
    bash)
      if [[ -f "${HOME}/.bashrc" ]]; then
        detected="${HOME}/.bashrc"
      elif [[ -f "${HOME}/.bash_profile" ]]; then
        detected="${HOME}/.bash_profile"
      fi
      ;;
    zsh)
      local zdot="${ZDOTDIR:-${HOME}}"
      if [[ -f "${zdot}/.zshrc" ]]; then
        detected="${zdot}/.zshrc"
      elif [[ -f "${zdot}/.zprofile" ]]; then
        detected="${zdot}/.zprofile"
      fi
      ;;
    fish)
      detected="${HOME}/.config/fish/config.fish"
      ;;
  esac

  if [[ -z "$detected" ]]; then
    for candidate in ".profile" ".bashrc" ".bash_profile" ".zprofile" ".zshrc"; do
      local zdot="${ZDOTDIR:-${HOME}}"
      if [[ -f "${zdot}/${candidate}" ]]; then
        detected="${zdot}/${candidate}"
        break
      fi
    done
  fi

  if [[ -n "$detected" ]]; then
    echo "$detected"
  fi
}

profile_configures_env() {
  local f
  for f in \
    "${HOME}/.bashrc" "${HOME}/.bash_profile" \
    "${ZDOTDIR:-${HOME}}/.zshrc" "${ZDOTDIR:-${HOME}}/.zprofile" \
    "${HOME}/.profile" "${HOME}/.config/fish/config.fish"; do
    [[ -f "$f" ]] || continue
    if grep -qF "$SHELL_MARKER_BEGIN" "$f" 2>/dev/null \
      || grep -qF 'phpvm env' "$f" 2>/dev/null; then
      echo "$f"
      return 0
    fi
  done
  return 1
}

remove_shell_integration_from_rc() {
  local rc_file="$1"
  [[ -f "$rc_file" ]] || return 0

  if ! grep -qF "$SHELL_MARKER_BEGIN" "$rc_file" 2>/dev/null; then
    return 0
  fi

  local tmp
  tmp="$(mktemp)"
  awk -v begin="$SHELL_MARKER_BEGIN" -v end="$SHELL_MARKER_END" '
    $0 == begin { skip=1; next }
    $0 == end { skip=0; next }
    skip == 0 { print }
  ' "$rc_file" >"$tmp"
  mv "$tmp" "$rc_file"
  info "Removed phpvm shell integration from ${rc_file}"
}

can_prompt() {
  [[ -r /dev/tty && -w /dev/tty ]]
}

# Read y/n from the controlling terminal (works when stdin is a curl pipe).
prompt_yes_no() {
  local prompt="$1"
  local default="${2:-n}"
  local response

  if ! can_prompt; then
    return 1
  fi

  if [[ "$default" == "y" ]]; then
    printf "%s [Y/n] " "$prompt" >/dev/tty
  else
    printf "%s [y/N] " "$prompt" >/dev/tty
  fi
  read -r response </dev/tty || return 1
  case "${response:-$default}" in
    y|Y|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

append_shell_integration_to_rc() {
  local rc_file="$1"
  local installed="$2"
  local install_dir="$3"
  local quoted_installed
  local quoted_install_dir
  quoted_installed="$(shell_quote "$installed")"
  quoted_install_dir="$(shell_quote "$install_dir")"
  local shell_flag="posix"

  if [[ "${rc_file##*/}" == "config.fish" ]]; then
    shell_flag="fish"
  fi

  if [[ -f "$rc_file" ]] && grep -qF "$SHELL_MARKER_BEGIN" "$rc_file" 2>/dev/null; then
    info "phpvm shell integration already present in ${rc_file}"
    return 0
  fi

  if [[ -f "$rc_file" ]] && grep -qF 'phpvm env' "$rc_file" 2>/dev/null; then
    info "${rc_file} already references phpvm env; leaving it unchanged"
    return 0
  fi

  mkdir -p "$(dirname "$rc_file")"
  {
    echo ""
    echo "$SHELL_MARKER_BEGIN"
    echo "export PHPVM_BIN=${quoted_installed}"
    if [[ "${rc_file##*/}" == "config.fish" ]]; then
      echo "if not contains -- ${quoted_install_dir} \$PATH"
      echo "  set -gx PATH ${quoted_install_dir} \$PATH"
      echo "end"
      echo 'if test -x "$PHPVM_BIN"'
      echo '  $PHPVM_BIN env --shell fish | source'
      echo "end"
    else
      echo "case \":\${PATH}:\": in"
      echo "  *\":${install_dir}:\"*) ;;"
      echo "  *) export PATH=${quoted_install_dir}:\$PATH ;;"
      echo "esac"
      echo '[ -x "$PHPVM_BIN" ] && eval "$("$PHPVM_BIN" env --shell '"$shell_flag"')"'
    fi
    echo "$SHELL_MARKER_END"
  } >>"$rc_file"
  info "Updated ${rc_file}"
}

phpvm_config_set_use_on_cd() {
  local enabled="$1"
  local config_dir="${PHPVM_HOME:-${HOME}/.phpvm}"
  local config_file="${config_dir}/config.toml"

  mkdir -p "$config_dir"
  if [[ -f "$config_file" ]] && grep -qE '^[[:space:]]*use_on_cd[[:space:]]*=' "$config_file" 2>/dev/null; then
    sed -i -E "s/^[[:space:]]*use_on_cd[[:space:]]*=.*/use_on_cd = ${enabled}/" "$config_file"
  elif [[ -f "$config_file" ]]; then
    printf '\nuse_on_cd = %s\n' "$enabled" >>"$config_file"
  else
    printf 'use_on_cd = %s\n' "$enabled" >"$config_file"
  fi
}

print_source_hint() {
  local rc_file="$1"
  info ""
  info "Run: source ${rc_file}"
  info "Or open a new terminal, then try: phpvm --help"
  info "After installing a runtime, try: phpvm use 8.3 && php -v"
  info "Per-project auto-switch on cd: set use_on_cd = true in ~/.phpvm/config.toml"
  info ""
}

print_env_hint() {
  local rc_file="$1"
  local installed="$2"
  local install_dir="$3"
  local quoted_installed
  local quoted_install_dir
  quoted_installed="$(shell_quote "$installed")"
  quoted_install_dir="$(shell_quote "$install_dir")"

  info ""
  info "For phpvm plus bare php/composer after phpvm use, add this to ${rc_file}:"
  info ""
  info "    $SHELL_MARKER_BEGIN"
  info "    export PHPVM_BIN=${quoted_installed}"
  info "    export PATH=${quoted_install_dir}:\$PATH"
  info '    [ -x "$PHPVM_BIN" ] && eval "$("$PHPVM_BIN" env)"'
  info "    $SHELL_MARKER_END"
  info ""
  info "Then restart your shell or run: source ${rc_file}"
  info "Optional: use_on_cd = true in ~/.phpvm/config.toml auto-switches on cd"
  info ""
}

configure_shell_integration() {
  local installed="$1"
  local install_dir="$2"
  local rc_file
  rc_file="$(detect_shell_rc)"

  if [[ -z "$rc_file" ]]; then
    info ""
    info "No shell profile found. Add phpvm to PATH and run: eval \"\$(phpvm env)\""
    info ""
    return
  fi

  local configured_in=""
  if configured_in="$(profile_configures_env)"; then
    info ""
    info "phpvm shell integration is configured in ${configured_in}."
    info ""
    return
  fi

  local should_update=0
  if [[ "$MODIFY_SHELL" == "1" || "$MODIFY_SHELL" == "yes" ]]; then
    should_update=1
  elif [[ "$MODIFY_SHELL" == "0" || "$MODIFY_SHELL" == "no" ]]; then
    should_update=0
  elif prompt_yes_no "Enable phpvm shell integration via ${rc_file}?"; then
    should_update=1
  fi

  if [[ $should_update -eq 1 ]]; then
    append_shell_integration_to_rc "$rc_file" "$installed" "$install_dir"
    if prompt_yes_no "Auto-switch PHP when cd'ing into projects with .phpvm-version?"; then
      phpvm_config_set_use_on_cd true
      info "Enabled use_on_cd in ~/.phpvm/config.toml"
    fi
    print_source_hint "$rc_file"
  else
    print_env_hint "$rc_file" "$installed" "$install_dir"
  fi
}

do_uninstall() {
  local bin="${INSTALL_DIR}/phpvm"
  local rc_file
  for rc_file in \
    "${HOME}/.bashrc" "${HOME}/.bash_profile" \
    "${ZDOTDIR:-${HOME}}/.zshrc" "${ZDOTDIR:-${HOME}}/.zprofile" \
    "${HOME}/.profile" "${HOME}/.config/fish/config.fish"; do
    remove_shell_integration_from_rc "$rc_file"
  done

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

  configure_shell_integration "$installed" "$INSTALL_DIR"

  info "To update later, re-run the same curl | bash command (or set PHPVM_VERSION)."
  info "To uninstall: PHPVM_UNINSTALL=1 bash install.sh  (or simply rm ${INSTALL_DIR}/phpvm)"
}

main "$@"
