#!/usr/bin/env bash
#
# Tusk installer — a fast PHP toolchain in Rust.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/lschvn/tusk/main/install.sh | bash
#   wget -qO- https://raw.githubusercontent.com/lschvn/tusk/main/install.sh | bash
#
# Install a specific version:
#   curl -fsSL .../install.sh | bash -s v0.1.0
#   TUSK_VERSION=v0.1.0 curl -fsSL .../install.sh | bash
#
# Install to a custom location (default: ~/.tusk):
#   TUSK_INSTALL=/opt/tusk curl -fsSL .../install.sh | bash
#
# The script downloads a prebuilt binary for your platform from GitHub
# Releases. If no prebuilt binary exists for your platform, it falls back
# to building from source with `cargo` (requires a Rust toolchain).

set -euo pipefail

# --- configuration ----------------------------------------------------------

REPO="lschvn/tusk"
BIN_NAME="tusk"

INSTALL_DIR="${TUSK_INSTALL:-${HOME}/.tusk}"
BIN_DIR="${INSTALL_DIR}/bin"
EXE="${BIN_DIR}/${BIN_NAME}"

# Version can come from $1, $TUSK_VERSION, or default to "latest".
VERSION="${1:-${TUSK_VERSION:-latest}}"

# --- pretty output ----------------------------------------------------------

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    Color_Off='\033[0m'
    Bold='\033[1m'
    Red='\033[0;31m'
    Green='\033[0;32m'
    Yellow='\033[0;33m'
    Blue='\033[0;34m'
    Dim='\033[0;2m'
else
    Color_Off='' Bold='' Red='' Green='' Yellow='' Blue='' Dim=''
fi

info() { printf '%b\n' "${Dim}>${Color_Off} $*"; }
success() { printf '%b\n' "${Green}✓${Color_Off} $*"; }
warn() { printf '%b\n' "${Yellow}!${Color_Off} $*" >&2; }
error() {
    printf '%b\n' "${Red}error${Color_Off}: $*" >&2
    exit 1
}

# --- helpers ----------------------------------------------------------------

has() { command -v "$1" >/dev/null 2>&1; }

# Download $1 to stdout (or to file $2 if given). Uses curl, falls back to wget.
download() {
    local url="$1" out="${2:-}"
    if has curl; then
        if [ -n "$out" ]; then
            curl -fsSL --proto '=https' --tlsv1.2 -o "$out" "$url"
        else
            curl -fsSL --proto '=https' --tlsv1.2 "$url"
        fi
    elif has wget; then
        if [ -n "$out" ]; then
            wget -qO "$out" "$url"
        else
            wget -qO- "$url"
        fi
    else
        error "neither 'curl' nor 'wget' is installed; cannot download."
    fi
}

# Resolve "latest" to a concrete tag by following the GitHub redirect.
# This avoids the rate-limited / auth-gated api.github.com endpoint.
resolve_latest() {
    local effective
    if has curl; then
        effective="$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
            "https://github.com/${REPO}/releases/latest")"
    elif has wget; then
        effective="$(wget -q -S --max-redirect=10 -O /dev/null \
            "https://github.com/${REPO}/releases/latest" 2>&1 \
            | awk '/^  Location: / {print $2}' | tail -n1)"
    else
        error "neither 'curl' nor 'wget' is installed; cannot resolve version."
    fi
    # ".../releases/tag/vX.Y.Z" -> "vX.Y.Z"
    case "$effective" in
        */tag/*) printf '%s' "${effective##*/tag/}" ;;
        *) printf '' ;;
    esac
}

# Verify the sha256 of $1 against the checksum file at URL $2 (best effort).
verify_checksum() {
    local file="$1" sums_url="$2" expected actual sum_tool
    if has sha256sum; then
        sum_tool="sha256sum"
    elif has shasum; then
        sum_tool="shasum -a 256"
    else
        warn "no sha256 tool found; skipping checksum verification."
        return 0
    fi

    expected="$(download "$sums_url" 2>/dev/null | awk '{print $1}' | head -n1 || true)"
    if [ -z "$expected" ]; then
        warn "checksum file unavailable; skipping verification."
        return 0
    fi

    actual="$(${sum_tool} "$file" | awk '{print $1}')"
    if [ "$expected" != "$actual" ]; then
        error "checksum mismatch for $(basename "$file")\n  expected: ${expected}\n  actual:   ${actual}"
    fi
    success "Checksum verified."
}

# --- platform detection -----------------------------------------------------

detect_target() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux) os="unknown-linux-musl" ;;
        Darwin) os="apple-darwin" ;;
        *) error "unsupported operating system: ${os}" ;;
    esac

    case "$arch" in
        x86_64 | amd64) arch="x86_64" ;;
        arm64 | aarch64) arch="aarch64" ;;
        *) error "unsupported architecture: ${arch}" ;;
    esac

    printf '%s-%s' "$arch" "$os"
}

# --- install strategies -----------------------------------------------------

install_from_release() {
    local target="$1" tag="$2"
    local asset="${BIN_NAME}-${target}.tar.gz"
    local base="${TUSK_BASE_URL:-https://github.com/${REPO}/releases/download}/${tag}"
    local tmp
    tmp="$(mktemp -d)"
    # shellcheck disable=SC2064
    trap "rm -rf '${tmp}'" RETURN

    info "Downloading ${Bold}${asset}${Color_Off} (${tag})..."
    if ! download "${base}/${asset}" "${tmp}/${asset}" 2>/dev/null; then
        return 1
    fi

    verify_checksum "${tmp}/${asset}" "${base}/${asset}.sha256"

    info "Extracting..."
    tar -xzf "${tmp}/${asset}" -C "${tmp}"

    local found
    found="$(find "${tmp}" -type f -name "${BIN_NAME}" | head -n1)"
    [ -n "$found" ] || error "archive did not contain a '${BIN_NAME}' binary."

    mkdir -p "${BIN_DIR}"
    install -m 0755 "$found" "${EXE}"
    return 0
}

install_from_source() {
    local tag="$1"
    has cargo || error \
        "no prebuilt binary for your platform and 'cargo' is not installed.\n  Install Rust from https://rustup.rs and re-run, or build manually:\n    git clone https://github.com/${REPO} && cd ${BIN_NAME} && cargo install --path crates/tusk-cli --root \"${INSTALL_DIR}\""

    warn "No prebuilt binary available — building from source with cargo."
    info "This may take a few minutes..."

    local ref_args=()
    if [ -n "$tag" ] && [ "$tag" != "latest" ]; then
        ref_args=(--tag "$tag")
    fi

    cargo install \
        --git "https://github.com/${REPO}" \
        "${ref_args[@]+"${ref_args[@]}"}" \
        --root "${INSTALL_DIR}" \
        --force \
        "${BIN_NAME}"
}

# --- PATH wiring ------------------------------------------------------------

# Append an export line to a shell rc file if the bin dir isn't already there.
add_path_posix() {
    local rc="$1"
    [ -n "$rc" ] || return 0
    if [ -f "$rc" ] && grep -qF "${BIN_DIR}" "$rc"; then
        return 0
    fi
    {
        printf '\n# tusk\n'
        printf 'export PATH="%s:$PATH"\n' "${BIN_DIR}"
    } >>"$rc"
    info "Added ${BIN_DIR} to PATH in ${Bold}${rc}${Color_Off}"
}

add_path_fish() {
    local rc="${XDG_CONFIG_HOME:-${HOME}/.config}/fish/config.fish"
    if [ -f "$rc" ] && grep -qF "${BIN_DIR}" "$rc"; then
        return 0
    fi
    mkdir -p "$(dirname "$rc")"
    {
        printf '\n# tusk\n'
        printf 'fish_add_path "%s"\n' "${BIN_DIR}"
    } >>"$rc"
    info "Added ${BIN_DIR} to PATH in ${Bold}${rc}${Color_Off}"
}

setup_path() {
    # Already reachable on PATH? Nothing to do.
    case ":${PATH}:" in
        *":${BIN_DIR}:"*) return 0 ;;
    esac

    local shell_name
    shell_name="$(basename "${SHELL:-}")"

    case "$shell_name" in
        bash)
            add_path_posix "${HOME}/.bashrc"
            # macOS login shells read .bash_profile, not .bashrc.
            if [ "$(uname -s)" = "Darwin" ]; then
                add_path_posix "${HOME}/.bash_profile"
            fi
            ;;
        zsh)
            add_path_posix "${ZDOTDIR:-${HOME}}/.zshrc"
            ;;
        fish)
            add_path_fish
            ;;
        *)
            add_path_posix "${HOME}/.profile"
            ;;
    esac
}

# --- main -------------------------------------------------------------------

main() {
    case "${1:-}" in
        -h | --help)
            cat <<EOF
Tusk installer — a fast PHP toolchain in Rust.

Usage:
  install.sh [VERSION]

Examples:
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash -s v0.1.0

Environment variables:
  TUSK_VERSION   Version/tag to install (default: latest)
  TUSK_INSTALL   Install directory (default: \$HOME/.tusk)
  NO_COLOR       Disable colored output
EOF
            exit 0
            ;;
    esac

    [ -n "${HOME:-}" ] || error "\$HOME is not set; cannot determine install location."

    printf '%b\n' "${Blue}${Bold}🐘 Installing Tusk${Color_Off} ${Dim}— a fast PHP toolchain in Rust${Color_Off}"

    local target tag
    target="$(detect_target)"
    info "Platform: ${Bold}${target}${Color_Off}"

    if [ "$VERSION" = "latest" ]; then
        tag="$(resolve_latest || true)"
        if [ -n "$tag" ]; then
            info "Latest release: ${Bold}${tag}${Color_Off}"
        else
            warn "Could not resolve the latest release tag."
        fi
    else
        tag="$VERSION"
        info "Requested version: ${Bold}${tag}${Color_Off}"
    fi

    # Prefer a prebuilt binary; fall back to building from source.
    if [ -n "$tag" ] && install_from_release "$target" "$tag"; then
        :
    else
        install_from_source "$tag"
    fi

    [ -x "$EXE" ] || error "installation failed: ${EXE} is missing."

    setup_path

    printf '\n'
    success "Tusk was installed to ${Bold}${EXE}${Color_Off}"
    info "Version: ${Bold}$("${EXE}" --version 2>/dev/null || echo unknown)${Color_Off}"

    printf '\n'
    case ":${PATH}:" in
        *":${BIN_DIR}:"*)
            info "Run ${Bold}${BIN_NAME} --help${Color_Off} to get started."
            ;;
        *)
            info "Restart your shell or run this to use ${BIN_NAME} now:"
            printf '\n    %bexport PATH="%s:$PATH"%b\n\n' "${Bold}" "${BIN_DIR}" "${Color_Off}"
            info "Then run ${Bold}${BIN_NAME} --help${Color_Off} to get started."
            ;;
    esac
}

main "$@"
