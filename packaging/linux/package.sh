#!/bin/sh
# Run on the Linux distribution and architecture you want to support.
set -eu
cd "$(dirname "$0")/../.."
[ "$(uname -s)" = Linux ] || { echo "This package is for Linux." >&2; exit 1; }
cargo build --locked --release --features desktop --target-dir target
version=$(cargo pkgid | sed 's/.*[#@]//')
name="ferriscope-$version-linux-$(uname -m)"
mkdir -p target/dist
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir "$stage/$name"
install -m755 target/release/ferriscope "$stage/$name/"
strip "$stage/$name/ferriscope"
install -m755 packaging/linux/install.sh "$stage/$name/"
install -m644 LICENSE packaging/linux/INSTALL.txt "$stage/$name/"
install -m644 assets/ferriscope.svg "$stage/$name/"
tar -C "$stage" -czf "target/dist/$name.tar.gz" "$name"
printf 'Share this archive: %s/target/dist/%s.tar.gz\n' "$PWD" "$name"
