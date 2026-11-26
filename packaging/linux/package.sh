#!/bin/sh
# Run on the Linux distribution and architecture you want to support.
set -eu
cd "$(dirname "$0")/../.."
[ "$(uname -s)" = Linux ] || { echo "This package is for Linux." >&2; exit 1; }

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

# Keep the native menu working without depending on the host's libxdo SONAME.
xdo_version=4.20260303.1
xdo_sha256=c1f971a384da588eb99ca0755fc4300316d49c1e612537e3f1de52215e104fa3
curl -fLsS "https://codeload.github.com/jordansissel/xdotool/tar.gz/refs/tags/v$xdo_version" -o "$stage/xdotool.tar.gz"
printf '%s  %s\n' "$xdo_sha256" "$stage/xdotool.tar.gz" | sha256sum -c -
tar -xzf "$stage/xdotool.tar.gz" -C "$stage"
xdo_source="$stage/xdotool-$xdo_version"
make -C "$xdo_source" libxdo.a

# Override the libxdo-sys build script only for this packaged binary.
host=$(rustc -vV | sed -n 's/^host: //p')
cargo \
    --config "target.$host.xdo.rustc-link-search=['native=$xdo_source']" \
    --config "target.$host.xdo.rustc-link-lib=['static=xdo','X11','Xtst','Xinerama','xkbcommon']" \
    build --locked --release --features desktop --target "$host" --target-dir target
binary="target/$host/release/ferriscope"
dynamic=$(readelf -d "$binary")
if printf '%s\n' "$dynamic" | grep -Eq 'NEEDED.*\[libxdo\.so'; then
    echo "Refusing to package Ferriscope with a shared libxdo dependency." >&2
    exit 1
fi

version=$(cargo pkgid | sed 's/.*[#@]//')
name="ferriscope-$version-linux-$(uname -m)"
mkdir -p target/dist
mkdir "$stage/$name"
install -m755 "$binary" "$stage/$name/"
strip "$stage/$name/ferriscope"
install -m755 packaging/linux/install.sh "$stage/$name/"
install -m644 LICENSE packaging/linux/INSTALL.txt "$stage/$name/"
install -m644 "$xdo_source/COPYRIGHT" "$stage/$name/LICENSE-libxdo"
install -m644 assets/ferriscope.svg "$stage/$name/"
tar -C "$stage" -czf "target/dist/$name.tar.gz" "$name"
printf 'Share this archive: %s/target/dist/%s.tar.gz\n' "$PWD" "$name"
