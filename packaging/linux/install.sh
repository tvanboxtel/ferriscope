#!/bin/sh
# Install the binary next to this script, never download or compile anything.
set -eu
cd "$(dirname "$0")"

# ldd can exit successfully even when the loader cannot resolve a library.
if ! libraries=$(LC_ALL=C ldd ./ferriscope 2>&1); then
    printf 'Ferriscope was not installed. Could not check runtime libraries:\n%s\n' "$libraries" >&2
    exit 1
fi
if missing=$(printf '%s\n' "$libraries" | grep -E '=> not found[[:space:]]*$|: version .* not found'); then
    printf 'Ferriscope was not installed. Missing or incompatible runtime libraries:\n%s\n' "$missing" >&2
    printf 'See INSTALL.txt for requirements, or use a build for your Linux distribution.\n' >&2
    exit 1
fi

bin="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
mkdir -p "$bin" "$apps" "$icons"
install -m644 ferriscope.svg "$icons/ferriscope.svg"
install -m755 ferriscope "$bin/ferriscope"
# Desktop launchers need an absolute path: their PATH often differs from zsh's.
exec_path=$(printf '%s' "$bin/ferriscope" | sed 's/[\\`"$]/\\&/g; s/%/%%/g; s/\\/\\\\/g')
# Use the installed image directly, even if the panel has stale icon-theme data.
icon_path=$(printf '%s' "$icons/ferriscope.svg" | sed 's/\\/\\\\/g')
cat > "$apps/ferriscope.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Ferriscope
Comment=Read and review GitHub pull requests
Exec="$exec_path"
Icon=$icon_path
StartupWMClass=ferriscope
Terminal=false
Categories=Development;
EOF
printf 'Installed. Open Ferriscope from your application menu or run "%s/ferriscope".\n' "$bin"
