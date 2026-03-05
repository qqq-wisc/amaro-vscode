#!/usr/bin/env bash
# Build release binaries for all platforms and place them in bin/.
#
# Requirements:
#   cargo-zigbuild  ->  cargo install cargo-zigbuild
#   zig             ->  brew install zig
#
# On first run:
#   cargo install cargo-zigbuild
#   brew install zig
#   npm run build-binaries

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
BIN_DIR="$ROOT_DIR/bin"
LSP_DIR="$ROOT_DIR/amaro-lsp"

# ── Preflight checks ───────────────────────────────────────────────────────────

if ! command -v cargo &>/dev/null; then
    echo "ERROR: cargo not found. Install Rust from https://rustup.rs/"
    exit 1
fi

if ! cargo zigbuild --version &>/dev/null 2>&1; then
    echo "ERROR: cargo-zigbuild not found."
    echo "  Install it with:  cargo install cargo-zigbuild"
    echo "  Then install zig: brew install zig"
    exit 1
fi

if ! command -v zig &>/dev/null; then
    echo "ERROR: zig not found."
    echo "  Install it with: brew install zig"
    exit 1
fi

# ── Setup ──────────────────────────────────────────────────────────────────────

mkdir -p "$BIN_DIR"

echo "==> Adding Rust targets..."
rustup target add aarch64-apple-darwin        2>/dev/null || true
rustup target add x86_64-apple-darwin         2>/dev/null || true
rustup target add x86_64-unknown-linux-musl   2>/dev/null || true
rustup target add x86_64-pc-windows-gnu       2>/dev/null || true

cd "$LSP_DIR"

# ── macOS ──────────────────────────────────────────────────────────────────────
# Build a universal macOS binary (ARM + x86) using lipo.
# Falls back to native-only if lipo is unavailable.

echo ""
echo "==> Building macOS (arm64 + x86_64 universal)..."
cargo zigbuild --target aarch64-apple-darwin --release
cargo zigbuild --target x86_64-apple-darwin  --release

if command -v lipo &>/dev/null; then
    lipo -create \
        target/aarch64-apple-darwin/release/amaro-lsp \
        target/x86_64-apple-darwin/release/amaro-lsp \
        -output "$BIN_DIR/amaro-lsp-mac"
    echo "    -> Universal binary (arm64 + x86_64)"
else
    # lipo not available (shouldn't happen on macOS but just in case)
    HOST_ARCH="$(uname -m)"
    if [ "$HOST_ARCH" = "arm64" ]; then
        cp target/aarch64-apple-darwin/release/amaro-lsp "$BIN_DIR/amaro-lsp-mac"
    else
        cp target/x86_64-apple-darwin/release/amaro-lsp "$BIN_DIR/amaro-lsp-mac"
    fi
    echo "    -> Single arch: $HOST_ARCH"
fi

# ── Linux ──────────────────────────────────────────────────────────────────────
# musl target produces a fully static binary that runs on any 64-bit Linux.

echo ""
echo "==> Building Linux (x86_64-musl, static)..."
cargo zigbuild --target x86_64-unknown-linux-musl --release
cp target/x86_64-unknown-linux-musl/release/amaro-lsp "$BIN_DIR/amaro-lsp-linux"

# ── Windows ────────────────────────────────────────────────────────────────────
# GNU target avoids the need for xwin / MSVC toolchain.

echo ""
echo "==> Building Windows (x86_64-gnu)..."
cargo zigbuild --target x86_64-pc-windows-gnu --release
cp target/x86_64-pc-windows-gnu/release/amaro-lsp.exe "$BIN_DIR/amaro-lsp-win.exe"

# ── Summary ────────────────────────────────────────────────────────────────────

echo ""
echo "All binaries placed in bin/:"
ls -lh "$BIN_DIR/"
