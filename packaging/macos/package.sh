#!/usr/bin/env bash
# Build and sign (ad-hoc by default) the macOS artifacts into dist/release:
#   paper-rs-<version>-macos-<arch>.dmg          paper-rs.app on a drag-to-Applications DMG
#   paper-rs-mcp-<version>-macos-<arch>.zip      the MCP server binary
# Usage: packaging/macos/package.sh [--arch universal|aarch64|x86_64] [--skip-build]
# Env: MACOS_SIGN_IDENTITY (default "-" = ad-hoc, not notarized; Gatekeeper rejects it on other Macs).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
VERSION="$(awk '/^\[package\]/{p=1;next} /^\[/{p=0} p&&$1=="version"{gsub(/"/,"",$3);print $3;exit}' "$ROOT/Cargo.toml")"
DIST="${DIST:-$ROOT/dist/release}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

ARCH=universal; SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$ARCH" in
  universal) TARGETS=(aarch64-apple-darwin x86_64-apple-darwin) ;;
  aarch64) TARGETS=(aarch64-apple-darwin) ;;
  x86_64) TARGETS=(x86_64-apple-darwin) ;;
  *) echo "unknown --arch $ARCH" >&2; exit 2 ;;
esac
export MACOSX_DEPLOYMENT_TARGET=11.0   # keep in sync with LSMinimumSystemVersion
IDENTITY="${MACOS_SIGN_IDENTITY:--}"
SHORT_VERSION="${VERSION%%-*}"
WORK="$CARGO_TARGET_DIR/macos-package"
APP="$WORK/paper-rs.app"
DMG="$DIST/paper-rs-$VERSION-macos-$ARCH.dmg"
MCP_ZIP="$DIST/paper-rs-mcp-$VERSION-macos-$ARCH.zip"
[ "$IDENTITY" = "-" ] && echo "warning: ad-hoc signing; artifacts are not notarized" >&2

if [ "$SKIP_BUILD" = 0 ]; then
  args=(); for t in "${TARGETS[@]}"; do args+=(--target "$t"); done
  (cd "$ROOT" && cargo build --release --locked --bins "${args[@]}")
fi
rm -rf "$WORK"; mkdir -p "$WORK/bin" "$DIST"
for bin in paper-rs paper-rs-mcp; do
  inputs=(); for t in "${TARGETS[@]}"; do inputs+=("$CARGO_TARGET_DIR/$t/release/$bin"); done
  lipo -create -output "$WORK/bin/$bin" "${inputs[@]}"
done

sign() {
  local ts=(--timestamp); [ "$IDENTITY" = "-" ] && ts=(--timestamp=none)
  codesign --force --sign "$IDENTITY" "${ts[@]}" "$@"
}

echo "==> assembling $APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$WORK/bin/paper-rs" "$APP/Contents/MacOS/paper-rs"
cp "$ROOT/assets/app-icon/paper-rs.icns" "$APP/Contents/Resources/paper-rs.icns"
cp "$ROOT"/LICENSE-* "$APP/Contents/Resources/"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHORT_VERSION@/$SHORT_VERSION/g" "$HERE/Info.plist.in" >"$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
printf 'APPL????' >"$APP/Contents/PkgInfo"
sign --options runtime --entitlements "$HERE/entitlements.plist" "$APP/Contents/MacOS/paper-rs"
sign --options runtime --entitlements "$HERE/entitlements.plist" "$APP"
codesign --verify --strict --deep --verbose=2 "$APP"

echo "==> building $DMG"
STAGE="$WORK/dmg"; mkdir -p "$STAGE"
ditto "$APP" "$STAGE/paper-rs.app"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG" "$WORK/raw.dmg"
# makehybrid + convert needs no mounted device (create -srcfolder is flaky on CI and in sandboxes).
hdiutil makehybrid -hfs -hfs-volume-name "paper-rs" -hfs-openfolder "$STAGE" -o "$WORK/raw.dmg" "$STAGE"
hdiutil convert "$WORK/raw.dmg" -format UDZO -imagekey zlib-level=9 -o "$DMG"
rm -f "$WORK/raw.dmg"
sign "$DMG"; codesign --verify --strict --verbose=2 "$DMG"

echo "==> building $MCP_ZIP"
MCP_DIR="$WORK/paper-rs-mcp-$VERSION-macos-$ARCH"; mkdir -p "$MCP_DIR"
cp "$WORK/bin/paper-rs-mcp" "$ROOT/README.md" "$ROOT"/LICENSE-* "$MCP_DIR/"
sign --options runtime "$MCP_DIR/paper-rs-mcp"
rm -f "$MCP_ZIP"; ditto -c -k --keepParent "$MCP_DIR" "$MCP_ZIP"
echo "==> done"; ls -lh "$DMG" "$MCP_ZIP"
