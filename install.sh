#!/usr/bin/env bash
#
# tusk installer — downloads the latest prebuilt tusk binary for Linux x86_64
# and installs it to ~/.tusk/bin, then adds that directory to your PATH.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/lschvn/tusk/main/install.sh | bash
#
# Environment variables:
#   TUSK_VERSION     Version to install (default: latest)
#                    Examples: v0.1.0, latest
#   TUSK_INSTALL     Install directory (default: ~/.tusk)
#                    Set to e.g. /usr/local to install into /usr/local/bin
#                    (requires write permission to that directory)
#   TUSK_REPO        Override the GitHub repo (default: lschvn/tusk)
#   TUSK_RELEASE_URL Override the full download URL (testing/local use)
#                    When set, TUSK_VERSION and TUSK_REPO are ignored.
#
# Supported platforms (downloaded as prebuilt binaries):
#   - Linux  x86_64
#
# Other platforms (macOS, Linux arm64) are detected but will print a message
# pointing to the build-from-source instructions, since prebuilt binaries for
# them are not yet published.
#
# On other platforms, or if no binary is found, the installer prints a
# message pointing to the build-from-source instructions.
#
# This script is idempotent: re-running it upgrades tusk in place.

set -euo pipefail

REPO="${TUSK_REPO:-lschvn/tusk}"
VERSION="${TUSK_VERSION:-latest}"
INSTALL_ROOT="${TUSK_INSTALL:-$HOME/.tusk}"
BIN_DIR="${INSTALL_ROOT%/}/bin"

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------

detect_target() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "${os}" in
        Linux)
            case "${arch}" in
                x86_64|amd64) echo "x86_64-unknown-linux-gnu" ;;
                aarch64|arm64) echo "aarch64-unknown-linux-gnu" ;;
                *) return 1 ;;
            esac
            ;;
        Darwin)
            case "${arch}" in
                x86_64) echo "x86_64-apple-darwin" ;;
                arm64|aarch64) echo "aarch64-apple-darwin" ;;
                *) return 1 ;;
            esac
            ;;
        *) return 1 ;;
    esac
}

if ! TARGET="$(detect_target)"; then
    echo "ERROR: tusk does not ship a prebuilt binary for $(uname -s)/$(uname -m)." >&2
    echo "Build from source instead:" >&2
    echo "  cargo install --git https://github.com/${REPO} --bin tusk" >&2
    exit 1
fi

ARCHIVE="tusk-${TARGET}.tar.gz"

# ---------------------------------------------------------------------------
# Resolve download URL
# ---------------------------------------------------------------------------

if [ -n "${TUSK_RELEASE_URL:-}" ]; then
    DOWNLOAD_URL="${TUSK_RELEASE_URL}"
elif [ "${VERSION}" = "latest" ]; then
    DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${ARCHIVE}"
else
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"
fi

# ---------------------------------------------------------------------------
# Download + extract
# ---------------------------------------------------------------------------

TMPDIR="$(mktemp -d)"
trap 'rm -rf "${TMPDIR}"' EXIT

echo "Downloading tusk ${VERSION} (${TARGET})..."
echo "  from: ${DOWNLOAD_URL}"
echo "  to:   ${BIN_DIR}"

if ! curl -fsSL --retry 3 --connect-timeout 15 -o "${TMPDIR}/${ARCHIVE}" "${DOWNLOAD_URL}"; then
    echo "" >&2
    echo "ERROR: download failed." >&2
    if [ "${VERSION}" = "latest" ]; then
        echo "  Check that a release exists at https://github.com/${REPO}/releases" >&2
    else
        echo "  Check that tag ${VERSION} exists at https://github.com/${REPO}/releases/tag/${VERSION}" >&2
    fi
    echo "  Or build from source: cargo install --git https://github.com/${REPO} --bin tusk" >&2
    exit 1
fi

tar -xzf "${TMPDIR}/${ARCHIVE}" -C "${TMPDIR}"

if [ ! -f "${TMPDIR}/tusk" ]; then
    echo "ERROR: archive did not contain a 'tusk' binary" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Install
# ---------------------------------------------------------------------------

mkdir -p "${BIN_DIR}"
chmod +x "${TMPDIR}/tusk"
mv -f "${TMPDIR}/tusk" "${BIN_DIR}/tusk"

INSTALLED_VERSION="$("${BIN_DIR}/tusk" --version 2>/dev/null || echo "unknown")"

echo ""
echo "✓ tusk installed to ${BIN_DIR}/tusk"
echo "  version: ${INSTALLED_VERSION}"

# ---------------------------------------------------------------------------
# PATH integration
# ---------------------------------------------------------------------------

# Detect the user's shell so we can patch the right rc file.
SHELL_NAME="$(basename "${SHELL:-/bin/sh}")"
case "${SHELL_NAME}" in
    bash) RC_FILE="${HOME}/.bashrc" ;;
    zsh)  RC_FILE="${HOME}/.zshrc" ;;
    fish) RC_FILE="${HOME}/.config/fish/config.fish" ;;
    *)    RC_FILE="${HOME}/.profile" ;;
esac

PATH_MARKER="tusk"

# Idempotent PATH update: add the line only if it's not already present.
if ! grep -qF "${BIN_DIR}" "${RC_FILE}" 2>/dev/null; then
    if [ "${SHELL_NAME}" = "fish" ]; then
        {
            echo ""
            echo "# tusk"
            echo "set -gx PATH \"${BIN_DIR} \$PATH\""
        } >> "${RC_FILE}"
    else
        {
            echo ""
            echo "# tusk"
            echo "export PATH=\"${BIN_DIR}:\$PATH\""
        } >> "${RC_FILE}"
    fi
    echo "  added ${BIN_DIR} to PATH in ${RC_FILE}"
    NEED_PATH_RELOAD=1
else
    echo "  ${BIN_DIR} already in PATH (via ${RC_FILE})"
    NEED_PATH_RELOAD=0
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

echo ""
if [ "${NEED_PATH_RELOAD:-0}" = "1" ]; then
    echo "Restart your shell, or run:"
    echo "  source ${RC_FILE}"
    echo ""
fi
echo "Then verify:"
echo "  tusk --help"
echo ""
echo "Try it on a PHP project:"
echo "  cd path/to/your/php/project"
echo "  tusk install"
