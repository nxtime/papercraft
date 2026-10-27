#!/usr/bin/env bash
# Build the current checkout, install it as PaperCraft.app on macOS, and launch it.
# Usage: tools/install-local.sh [--no-launch] [--skip-build]
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG_DIR="$ROOT/packaging/macos"
DEFAULT_DEST="/Applications/PaperCraft.app"
APP_NAME="PaperCraft"
stage_root=""   # global: read by the EXIT trap after main returns

usage() {
  cat <<'EOF'
PaperCraft local installer (macOS only)

Usage: tools/install-local.sh [--no-launch] [--skip-build]

Options:
  --no-launch    Install the app but do not launch it afterwards.
  --skip-build   Do not compile; reuse the existing release binary in the target dir.
  -h, --help     Print this help and exit.

Environment:
  CARGO_TARGET_DIR     Build directory (default: <repo>/target, shared with cargo test).
  PAPERCRAFT_SIGN_IDENTITY  Code-signing identity (default: "PaperCraft Local" when present, else ad hoc).
  PAPERCRAFT_APP_PATH  Install destination (default: /Applications/PaperCraft.app).

The previous bundle is kept next to the new one as PaperCraft.app.backup-<timestamp>.
A running app is asked to quit gracefully before the swap; it is never force killed.
This script never uses sudo and never deletes user files.
EOF
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

# PIDs of the exact installed binary (never another app).
app_pids() {
  local bin="$1" pid cmd
  for pid in $(pgrep -f -- "$bin" 2>/dev/null); do
    cmd="$(ps -p "$pid" -o comm= 2>/dev/null || true)"
    [ "$cmd" = "$bin" ] || continue
    printf '%s\n' "$pid"
  done
}

cleanup_stage() {
  [ -n "${stage_root:-}" ] || return 0
  [ -d "$stage_root" ] || return 0
  find "$stage_root" -depth -mindepth 1 -delete 2>/dev/null || true
  rmdir "$stage_root" 2>/dev/null || true
}

main() {
  local no_launch=0 skip_build=0

  while [ $# -gt 0 ]; do
    case "$1" in
      --no-launch) no_launch=1; shift ;;
      --skip-build) skip_build=1; shift ;;
      -h|--help) usage; exit 0 ;;
      *) printf 'unknown argument: %s\n\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
  done

  [ "$(uname -s)" = "Darwin" ] || die "this installer supports macOS only (host: $(uname -s))"

  local version short_version
  version="$(awk '/^\[package\]/{p=1;next} /^\[/{p=0} p&&$1=="version"{gsub(/"/,"",$3);print $3;exit}' "$ROOT/Cargo.toml")"
  [ -n "$version" ] || die "could not read the version from $ROOT/Cargo.toml"
  short_version="${version%%-*}"

  local target_triple
  case "$(uname -m)" in
    arm64|aarch64) target_triple="aarch64-apple-darwin" ;;
    x86_64|amd64) target_triple="x86_64-apple-darwin" ;;
    *) die "unsupported host architecture: $(uname -m)" ;;
  esac

  local dest="${PAPERCRAFT_APP_PATH:-$DEFAULT_DEST}"
  while [ -n "$dest" ] && [ "${dest%/}" != "$dest" ]; do dest="${dest%/}"; done
  [ -n "$dest" ] || die "PAPERCRAFT_APP_PATH must not be empty"
  local dest_dir dest_base
  dest_dir="$(dirname "$dest")"
  dest_base="$(basename "$dest")"
  [ -d "$dest_dir" ] || die "install directory does not exist: $dest_dir"
  [ -w "$dest_dir" ] || die "$dest_dir is not writable; set PAPERCRAFT_APP_PATH to a writable folder (this script never uses sudo)"

  # The repo's own target dir: one shared build cache, on the same volume as the checkout.
  local target_dir="${CARGO_TARGET_DIR:-$ROOT/target}"
  case "$target_dir" in /*) ;; *) target_dir="$ROOT/$target_dir" ;; esac
  mkdir -p "$target_dir"

  local bin="$target_dir/$target_triple/release/papercraft"
  if [ "$skip_build" -eq 0 ]; then
    echo "==> building papercraft $version ($target_triple)"
    (cd "$ROOT" && CARGO_TARGET_DIR="$target_dir" MACOSX_DEPLOYMENT_TARGET=11.0 \
      cargo build --release --locked --bin papercraft --bin papercraft-cli --bin papercraft-mcp --target "$target_triple")
  else
    echo "==> skipping the build (--skip-build)"
    if [ ! -f "$bin" ]; then
      bin="$target_dir/release/papercraft"
    fi
  fi
  [ -f "$bin" ] || die "release binary not found: $bin (build first, or drop --skip-build)"

  local stage_root_new
  stage_root_new="$(mktemp -d "$target_dir/.install-local-stage.XXXXXX")"
  stage_root="$stage_root_new"
  trap 'if [ "$?" = 0 ]; then cleanup_stage; else printf "staging left at %s\n" "$stage_root" >&2; fi' EXIT

  local staged_app="$stage_root/$APP_NAME.app"
  mkdir -p "$staged_app/Contents/MacOS" "$staged_app/Contents/Resources"
  cp "$bin" "$staged_app/Contents/MacOS/$APP_NAME"
  chmod 0755 "$staged_app/Contents/MacOS/$APP_NAME"
  cp "$ROOT/assets/app-icon/papercraft.icns" "$staged_app/Contents/Resources/papercraft.icns"
  cp "$ROOT"/LICENSE-* "$staged_app/Contents/Resources/"
  sed -e "s/@VERSION@/$version/g" -e "s/@SHORT_VERSION@/$short_version/g" \
    "$PKG_DIR/Info.plist.in" >"$staged_app/Contents/Info.plist"
  printf 'APPL????' >"$staged_app/Contents/PkgInfo"

  plutil -lint "$staged_app/Contents/Info.plist" >/dev/null
  # A stable identity keeps macOS privacy grants (e.g. removable volumes) across rebuilds;
  # ad-hoc signing ("-") makes every build a new app that asks again.
  local sign_id="${PAPERCRAFT_SIGN_IDENTITY:-}"
  if [ -z "$sign_id" ] && security find-identity -p codesigning 2>/dev/null | grep -q '"PaperCraft Local"'; then
    sign_id="PaperCraft Local"
  fi
  sign_id="${sign_id:--}"
  codesign --force --sign "$sign_id" --timestamp=none --options runtime \
    --entitlements "$PKG_DIR/entitlements.plist" "$staged_app/Contents/MacOS/$APP_NAME"
  codesign --force --sign "$sign_id" --timestamp=none --options runtime \
    --entitlements "$PKG_DIR/entitlements.plist" "$staged_app"
  codesign --verify --strict --deep --verbose=2 "$staged_app"
  echo "==> signed and verified $staged_app"

  local install_bin="$dest/Contents/MacOS/$APP_NAME"
  local pids
  pids="$(app_pids "$install_bin")"
  if [ -n "$pids" ]; then
    echo "==> $APP_NAME is running (pid $(echo "$pids" | tr '\n' ' ')); asking it to quit"
    local bundle_id
    bundle_id="$(plutil -extract CFBundleIdentifier raw -o - "$dest/Contents/Info.plist" 2>/dev/null || true)"
    local escaped="${dest//\\/\\\\}"
    escaped="${escaped//\"/\\\"}"
    local out
    if ! out="$(osascript -e "tell application \"$escaped\" to quit" 2>&1)"; then
      case "$out" in
        *-128*|*"User canceled"*)
          printf '%s\n' "$out" >&2
          die "the quit request was cancelled; $APP_NAME is still running. Quit it and re-run; nothing was changed."
          ;;
      esac
      if [ -z "$bundle_id" ] || ! out="$(osascript -e "tell application id \"$bundle_id\" to quit" 2>&1)"; then
        printf '%s\n' "$out" >&2
        die "could not ask $APP_NAME to quit; it is still running. Quit it and re-run; nothing was changed."
      fi
    fi
    local waited=0
    while [ -n "$(app_pids "$install_bin")" ]; do
      sleep 0.5
      waited=$((waited + 1))
      [ "$waited" -lt 30 ] || die "$APP_NAME did not exit within 15s; aborting without forcing it"
    done
    echo "==> $APP_NAME exited"
  fi

  local new_app="$dest_dir/.$dest_base.new.$$"
  local backup=""
  local swapped=0
  swap_rollback() {
    local rc=$?
    trap - ERR INT TERM
    if [ "$swapped" = 1 ] && [ -n "$backup" ] && [ -e "$backup" ] && [ ! -e "$dest" ]; then
      if mv "$backup" "$dest"; then
        printf '==> rolled back: the previous app was restored to %s\n' "$dest" >&2
      else
        printf 'error: rollback failed; the previous app is still at %s\n' "$backup" >&2
      fi
    fi
    exit "$rc"
  }
  trap swap_rollback ERR INT TERM

  echo "==> staging next to $dest"
  ditto "$staged_app" "$new_app"
  if [ -e "$dest" ]; then
    backup="$dest_dir/$dest_base.backup-$(date +%Y%m%d-%H%M%S)"
    local n=2
    while [ -e "$backup" ]; do
      backup="$dest_dir/$dest_base.backup-$(date +%Y%m%d-%H%M%S)-$n"
      n=$((n + 1))
    done
    echo "==> keeping the current app as $(basename "$backup")"
    # Only the newest previous bundle is kept; a day of installs once left twelve.
    ls -dt "$dest_dir/$dest_base".backup-* 2>/dev/null | tail -n +2 | while read -r old; do rm -rf "$old"; done || true
    mv "$dest" "$backup"
  fi
  swapped=1
  mv "$new_app" "$dest"
  swapped=0
  trap - ERR INT TERM

  echo "==> installed $dest"

  # Keep the CLI and the stdio MCP bridge agents call in step with the app: both answer the
  # tool list from their own copy, so a stale one hides newer tools.
  local tool
  for tool in papercraft-cli papercraft-mcp; do
    if [ -f "${bin%/papercraft}/$tool" ]; then
      mkdir -p "$HOME/.local/bin"
      cp "${bin%/papercraft}/$tool" "$HOME/.local/bin/$tool"
      echo "==> installed $HOME/.local/bin/$tool"
    fi
  done

  if [ "$no_launch" -eq 0 ]; then
    open "$dest"
    echo "==> launched"
  fi

  echo
  echo "installed path : $dest"
  echo "local command  : open \"$dest\""
  if [ -n "$backup" ]; then
    echo "previous bundle: $backup"
  fi
}

main "$@"
