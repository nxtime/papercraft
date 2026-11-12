#!/usr/bin/env bash
# Build the PaperCraft Android APK: the Rust app as libpapercraft_android.so under a GameActivity.
# Usage: packaging/android/package.sh [--abi arm64-v8a|x86_64|all] [--debug]
# Needs: rustup targets aarch64-linux-android / x86_64-linux-android, ANDROID_HOME (SDK with
# build-tools, platforms;android-36, ndk), a JDK (javac, keytool), python, and curl for the
# GameActivity library (fetched once, or copied into $CARGO_TARGET_DIR/android-deps).
# Output: $DIST/PaperCraft-<version>.apk, signed with the debug key (~/.android/debug.keystore, made on
# first use) unless ANDROID_KEYSTORE / ANDROID_KEYSTORE_PASS / ANDROID_KEY_ALIAS are set.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HERE="$ROOT/packaging/android"
VERSION=$(awk -F'"' '/^version *=/ { print $2; exit }' "$ROOT/Cargo.toml")
[ -n "$VERSION" ] || { echo "no version in $ROOT/Cargo.toml" >&2; exit 1; }
DIST="${DIST:-$ROOT/dist/release}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

abi=arm64-v8a; profile=release
while [ $# -gt 0 ]; do
  case "$1" in
    --abi) abi=${2:?--abi needs a value}; shift 2 ;;
    --debug) profile=debug; shift ;;
    *) echo "unknown option $1" >&2; exit 64 ;;
  esac
done

: "${ANDROID_HOME:?set ANDROID_HOME to the Android SDK}"
API=26; PLATFORM=36; BUILD_TOOLS=$(ls "$ANDROID_HOME/build-tools" | sort -V | tail -n1)
NDK=$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -n1)
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) host=windows-x86_64; ext=.cmd; exe=.exe; bat=.bat ;; Darwin) host=darwin-x86_64; ext=; exe=; bat= ;; *) host=linux-x86_64; ext=; exe=; bat= ;; esac
LLVM=$NDK/toolchains/llvm/prebuilt/$host/bin
BT=$ANDROID_HOME/build-tools/$BUILD_TOOLS
JAR=$ANDROID_HOME/platforms/android-$PLATFORM/android.jar
[ -f "$JAR" ] || { echo "missing $JAR (sdkmanager 'platforms;android-$PLATFORM')" >&2; exit 1; }

case "$abi" in all) abis="arm64-v8a x86_64" ;; arm64-v8a|x86_64) abis=$abi ;; *) echo "--abi: arm64-v8a, x86_64 or all" >&2; exit 64 ;; esac
stage=$CARGO_TARGET_DIR/android-apk
rm -rf "$stage"; mkdir -p "$stage/lib" "$stage/res/mipmap-xxxhdpi"

for a in $abis; do
  case "$a" in arm64-v8a) triple=aarch64-linux-android ;; x86_64) triple=x86_64-linux-android ;; esac
  upper=$(echo "$triple" | tr 'a-z-' 'A-Z_')
  # The C and C++ in GameActivity's glue and zstd are built with the NDK's clang.
  export "CC_${triple//-/_}=$LLVM/$triple$API-clang$ext" "CXX_${triple//-/_}=$LLVM/$triple$API-clang++$ext" "AR_${triple//-/_}=$LLVM/llvm-ar$exe" \
         "CARGO_TARGET_${upper}_LINKER=$LLVM/$triple$API-clang$ext"
  flag=; [ "$profile" = release ] && flag=--release
  cargo build $flag --manifest-path "$ROOT/apps/android/Cargo.toml" --lib --target "$triple" ${CARGO_ARGS:-}
  mkdir -p "$stage/lib/$a"
  cp "$CARGO_TARGET_DIR/$triple/$profile/libpapercraft_android.so" "$stage/lib/$a/"
  # Rust needs only bionic; a C++ dependency would need libc++_shared.so next to it.
  if "$LLVM/llvm-readelf$exe" -d "$stage/lib/$a/libpapercraft_android.so" | grep -q 'libc++_shared'; then
    cp "$NDK/toolchains/llvm/prebuilt/$host/sysroot/usr/lib/$triple/libc++_shared.so" "$stage/lib/$a/"
  fi
done

cp "$ROOT/assets/app-icon/papercraft-1024.png" "$stage/res/mipmap-xxxhdpi/ic_launcher.png"
code=$(echo "$VERSION" | awk -F'[.-]' '{ printf "%d", ($1*10000 + $2*100 + $3) }')
sed -e "s/VERSION_CODE/$code/" -e "s/VERSION_NAME/$VERSION/" "$HERE/AndroidManifest.xml" > "$stage/AndroidManifest.xml"

"$BT/aapt2$exe" compile --dir "$stage/res" -o "$stage/res.zip"
"$BT/aapt2$exe" link -o "$stage/unsigned.apk" -I "$JAR" --manifest "$stage/AndroidManifest.xml" "$stage/res.zip"
# GameActivity: the AAR's classes.jar, fetched once and checked. Its AppCompat and AndroidX core
# needs are covered by the stand-ins in java/androidx (no resources).
GA_VERSION=4.4.0
GA_SHA256=3a8b0887ac55a0a6ab56021ced7353a250539eac811278b74bccd8a5b678d9c9
deps=$CARGO_TARGET_DIR/android-deps; ga_aar=$deps/games-activity-$GA_VERSION.aar; ga_jar=$deps/games-activity-$GA_VERSION.jar
mkdir -p "$deps"
py=$(command -v python || command -v python3)
if [ ! -f "$ga_jar" ]; then
  [ -f "$ga_aar" ] || curl -fsSL -o "$ga_aar.part" "https://dl.google.com/android/maven2/androidx/games/games-activity/$GA_VERSION/games-activity-$GA_VERSION.aar"
  [ ! -f "$ga_aar.part" ] || mv "$ga_aar.part" "$ga_aar"
  echo "$GA_SHA256  $ga_aar" | sha256sum -c --quiet - || { rm -f "$ga_aar"; echo "games-activity $GA_VERSION: checksum mismatch" >&2; exit 1; }
  "$py" -I -c 'import sys, zipfile; open(sys.argv[2], "wb").write(zipfile.ZipFile(sys.argv[1]).read("classes.jar"))' "$ga_aar" "$ga_jar"
fi
# The Java side: PaperActivity (a GameActivity) and the AndroidX stand-ins.
mkdir -p "$stage/classes" "$stage/dex"
javac --release 8 -Xlint:-options -classpath "$JAR:$ga_jar" -d "$stage/classes" $(find "$HERE/java" -name '*.java')
"$BT/d8$bat" --min-api $API --lib "$JAR" --output "$stage/dex" $(find "$stage/classes" -name '*.class') "$ga_jar"
cp "$stage/dex/classes.dex" "$stage/classes.dex"
( cd "$stage" && "$py" -I - <<'PY'
import os, zipfile
with zipfile.ZipFile("unsigned.apk", "a", zipfile.ZIP_DEFLATED) as apk:
    apk.write("classes.dex", "classes.dex")
    for root, _, files in os.walk("lib"):
        for name in files:
            path = os.path.join(root, name)
            apk.write(path, path.replace(os.sep, "/"))
PY
)
"$BT/zipalign$exe" -p -f 4 "$stage/unsigned.apk" "$stage/aligned.apk"

if [ -n "${ANDROID_KEYSTORE:-}" ]; then
  ks=$ANDROID_KEYSTORE; pass=${ANDROID_KEYSTORE_PASS:?}; alias=${ANDROID_KEY_ALIAS:?}
else
  ks=$HOME/.android/debug.keystore; pass=android; alias=androiddebugkey
  [ -f "$ks" ] || { mkdir -p "$(dirname "$ks")"; keytool -genkeypair -keystore "$ks" -storepass android -keypass android \
    -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US" >/dev/null; }
fi
mkdir -p "$DIST"
out=$DIST/PaperCraft-$VERSION.apk
"$BT/apksigner$bat" sign --ks "$ks" --ks-pass "pass:$pass" --ks-key-alias "$alias" --out "$out" "$stage/aligned.apk"
"$BT/apksigner$bat" verify "$out"
echo "$out"
