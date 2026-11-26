#!/bin/bash
# Build on Apple Silicon with Apple's command-line tools. No Developer ID required.
set -euo pipefail
cd "$(dirname "$0")/../.."
[[ "$(uname -s)/$(uname -m)" == Darwin/arm64 ]] || { echo "Requires Apple Silicon macOS" >&2; exit 1; }
cargo build --locked --release --features desktop --target-dir target
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
app="$stage/Ferriscope.app"
icons="$stage/Ferriscope.iconset"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$icons" target/dist
install -m755 target/release/ferriscope "$app/Contents/MacOS/"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" assets/ferriscope.png --out "$icons/icon_${size}x${size}.png" >/dev/null
  pixels=$((size * 2))
  sips -z "$pixels" "$pixels" assets/ferriscope.png --out "$icons/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$icons" -o "$app/Contents/Resources/Ferriscope.icns"
version=$(cargo pkgid | sed 's/.*[#@]//')
plist="$app/Contents/Info.plist"
plutil -create xml1 "$plist"
for field in \
  CFBundleName:Ferriscope CFBundleDisplayName:Ferriscope CFBundleExecutable:ferriscope \
  CFBundlePackageType:APPL CFBundleIconFile:Ferriscope.icns \
  CFBundleIdentifier:com.github.tvanboxtel.ferriscope \
  "CFBundleShortVersionString:$version" "CFBundleVersion:$version"; do
  plutil -insert "${field%%:*}" -string "${field#*:}" "$plist"
done
codesign --force --sign - "$app"
ditto -c -k --keepParent "$app" target/dist/Ferriscope-macOS-arm64.zip
# Exercise the distributable, not the pre-archive bundle, in the launch check.
ditto -x -k target/dist/Ferriscope-macOS-arm64.zip target/dist
