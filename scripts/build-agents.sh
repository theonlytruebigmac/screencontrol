#!/usr/bin/env bash
set -euo pipefail

# ─── ScreenControl Agent Build Script ─────────────────────────
#
# Cross-compiles sc-agent for all supported platforms, computes
# SHA-256 checksums, and generates a manifest.json that the
# server uses for the update-check endpoint.
#
# Usage:
#   ./scripts/build-agents.sh             # Build all targets
#   ./scripts/build-agents.sh --release   # Release profile (default)
#   ./scripts/build-agents.sh --debug     # Debug profile
#
# Prerequisites:
#   - Rust toolchain with cross-compilation targets installed
#   - Or 'cross' tool: cargo install cross
#
# Output:
#   server/agent-builds/
#   ├── manifest.json
#   ├── sc-agent-linux-x86_64
#   ├── sc-agent-linux-aarch64
#   ├── sc-agent-macos-x86_64
#   ├── sc-agent-macos-aarch64
#   └── sc-agent-windows-x86_64.exe

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="$PROJECT_ROOT/server/agent-builds"
AGENT_CRATE="$PROJECT_ROOT/server"

# Parse args
PROFILE="release"
CARGO_PROFILE_FLAG="--release"
for arg in "$@"; do
    case "$arg" in
        --debug)
            PROFILE="debug"
            CARGO_PROFILE_FLAG=""
            ;;
        --release)
            PROFILE="release"
            CARGO_PROFILE_FLAG="--release"
            ;;
    esac
done

# Read version from workspace Cargo.toml
VERSION=$(grep '^version' "$AGENT_CRATE/crates/sc-agent/Cargo.toml" | head -1 | sed 's/.*workspace.*//')
if [ -z "$VERSION" ] || [ "$VERSION" = "" ]; then
    # Version is inherited from workspace
    VERSION=$(grep -A1 '\[workspace.package\]' "$AGENT_CRATE/Cargo.toml" | grep 'version' | sed 's/.*"\(.*\)"/\1/')
fi
echo "═══════════════════════════════════════════════"
echo "  ScreenControl Agent Builder v${VERSION}"
echo "  Profile: ${PROFILE}"
echo "═══════════════════════════════════════════════"

# Create output directory
mkdir -p "$BUILD_DIR"

# Define targets using parallel arrays (compatible with bash 3.2+)
TARGET_TRIPLES=(
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
    "x86_64-pc-windows-gnu"
)
TARGET_OUTPUTS=(
    "sc-agent-linux-x86_64"
    "sc-agent-linux-aarch64"
    "sc-agent-macos-x86_64"
    "sc-agent-macos-aarch64"
    "sc-agent-windows-x86_64.exe"
)

# Detect which build tool to use
if command -v cross &>/dev/null; then
    BUILD_CMD="cross"
    echo "Using 'cross' for cross-compilation"
else
    BUILD_CMD="cargo"
    echo "Using 'cargo' (native targets only)"
fi

# Track successful builds for manifest (parallel arrays)
BUILT_NAMES=()
BUILT_CHECKSUMS=()
BUILT_SIZES=()

build_target() {
    local target="$1"
    local output_name="$2"

    echo ""
    echo "━━━ Building for $target ━━━"

    # Check if target is installed
    if [ "$BUILD_CMD" = "cargo" ]; then
        if ! rustup target list --installed 2>/dev/null | grep -q "$target"; then
            echo "⚠️  Target $target not installed, skipping (install with: rustup target add $target)"
            return 1
        fi
    fi

    # Build
    if $BUILD_CMD build \
        --manifest-path "$AGENT_CRATE/Cargo.toml" \
        --package sc-agent \
        --target "$target" \
        $CARGO_PROFILE_FLAG 2>&1; then

        # Find the built binary
        local bin_name="sc-agent"
        if [[ "$target" == *windows* ]]; then
            bin_name="sc-agent.exe"
        fi
        local src="$AGENT_CRATE/target/$target/$PROFILE/$bin_name"

        if [ -f "$src" ]; then
            cp "$src" "$BUILD_DIR/$output_name"
            chmod +x "$BUILD_DIR/$output_name" 2>/dev/null || true

            local checksum
            if command -v shasum &>/dev/null; then
                checksum=$(shasum -a 256 "$BUILD_DIR/$output_name" | cut -d' ' -f1)
            else
                checksum=$(sha256sum "$BUILD_DIR/$output_name" | cut -d' ' -f1)
            fi
            local size
            size=$(stat -f%z "$BUILD_DIR/$output_name" 2>/dev/null || stat -c%s "$BUILD_DIR/$output_name")

            BUILT_NAMES+=("$output_name")
            BUILT_CHECKSUMS+=("$checksum")
            BUILT_SIZES+=("$size")
            echo "✅ $output_name ($size bytes, sha256: ${checksum:0:16}...)"
            return 0
        else
            echo "❌ Binary not found at $src"
            return 1
        fi
    else
        echo "❌ Build failed for $target"
        return 1
    fi
}

# If --native-only flag or no cross available, just build for current platform
NATIVE_ONLY=false
for arg in "$@"; do
    if [ "$arg" = "--native-only" ]; then
        NATIVE_ONLY=true
    fi
done

if [ "$NATIVE_ONLY" = true ]; then
    BUILD_CMD="cargo"  # Force cargo for native builds (avoid cross Docker glibc mismatch)
    CURRENT_TARGET=$(rustc -vV | grep host | sed 's/host: //')
    for i in "${!TARGET_TRIPLES[@]}"; do
        if [ "${TARGET_TRIPLES[$i]}" = "$CURRENT_TARGET" ]; then
            build_target "$CURRENT_TARGET" "${TARGET_OUTPUTS[$i]}" || true
            break
        fi
    done
else
    # Build all targets
    for i in "${!TARGET_TRIPLES[@]}"; do
        build_target "${TARGET_TRIPLES[$i]}" "${TARGET_OUTPUTS[$i]}" || true
    done
fi

# ── Generate manifest.json ────────────────────────────────────

echo ""
echo "━━━ Generating manifest.json ━━━"

MANIFEST="$BUILD_DIR/manifest.json"

# Start JSON
cat > "$MANIFEST" << EOF
{
  "version": "${VERSION}",
  "built_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "builds": {
EOF

# Add each built target
FIRST=true
for i in "${!BUILT_NAMES[@]}"; do
    output_name="${BUILT_NAMES[$i]}"
    checksum="${BUILT_CHECKSUMS[$i]}"
    size="${BUILT_SIZES[$i]}"

    # Derive os-arch key from filename
    local_key="${output_name#sc-agent-}"
    local_key="${local_key%.exe}"

    if [ "$FIRST" = true ]; then
        FIRST=false
    else
        echo "," >> "$MANIFEST"
    fi

    cat >> "$MANIFEST" << EOF
    "${local_key}": {
      "file": "${output_name}",
      "sha256": "${checksum}",
      "size": ${size}
    }
EOF
done

cat >> "$MANIFEST" << EOF

  },
  "release_notes": "",
  "mandatory": false
}
EOF

echo "✅ Manifest written to $MANIFEST"
echo ""
echo "═══════════════════════════════════════════════"
echo "  Build complete! ${#BUILT_NAMES[@]} target(s) built"
echo "  Output: $BUILD_DIR/"
echo "═══════════════════════════════════════════════"
