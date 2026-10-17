#!/usr/bin/env bash
# Build FormCraft-<version>-x86_64.AppImage with linuxdeploy (called by `cargo xtask dist`).
# Usage: build-appimage.sh <release-dir> <out-dir> <version>
set -euo pipefail
release="$1"; out="$2"; version="$3"
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
appdir="$(mktemp -d)/FormCraft.AppDir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/formcraft/plugins"
install -m755 "$release/formcraft" "$release/formcraft-cli" "$appdir/usr/bin/"
for p in gear material; do
  mkdir -p "$appdir/usr/share/formcraft/plugins/$p"
  cp "$root/examples/plugins/$p/plugin.toml" "$root/examples/plugins/$p/$p.wasm" "$appdir/usr/share/formcraft/plugins/$p/"
done
cp -r "$root/python" "$root/examples/scripts" "$appdir/usr/share/formcraft/"
cp "$root/README.md" "$appdir/usr/share/formcraft/"
command -v linuxdeploy >/dev/null || { echo "linuxdeploy not found (https://github.com/linuxdeploy/linuxdeploy)"; exit 1; }
VERSION="$version" OUTPUT="$out/FormCraft-$version-x86_64.AppImage" \
  linuxdeploy --appdir "$appdir" \
    --desktop-file "$here/formcraft.desktop" \
    --icon-file "$here/formcraft.png" \
    --output appimage
