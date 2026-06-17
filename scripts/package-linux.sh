#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

version="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
target="${TARGET:-x86_64-unknown-linux-gnu}"
binary="${BINARY_PATH:-target/$target/release/pile}"
dist="${DIST_DIR:-dist}"
app_id="ai.opencode.pile"
package_name="pile-$version-$target-linux"
package_dir="$dist/$package_name"

if [[ ! -x "$binary" ]]; then
  echo "package-linux: binary not found or not executable: $binary" >&2
  exit 1
fi

rm -rf "$package_dir"
mkdir -p \
  "$package_dir/bin" \
  "$package_dir/share/applications" \
  "$package_dir/share/metainfo" \
  "$package_dir/share/icons/hicolor"

cp "$binary" "$package_dir/bin/pile"
cp assets/linux/"$app_id".desktop "$package_dir/share/applications/$app_id.desktop"
cp assets/linux/"$app_id".metainfo.xml "$package_dir/share/metainfo/$app_id.metainfo.xml"
cp LICENSE README.md "$package_dir/"

for icon in assets/icons.iconset/icon_*x*.png; do
  file="$(basename "$icon")"
  size="${file#icon_}"
  size="${size%%@*}"
  size="${size%.png}"
  case "$size" in
    16x16|32x32|64x64|128x128|256x256|512x512|1024x1024)
      mkdir -p "$package_dir/share/icons/hicolor/$size/apps"
      cp "$icon" "$package_dir/share/icons/hicolor/$size/apps/$app_id.png"
      ;;
  esac
done

tarball="$dist/$package_name.tar.gz"
tar -C "$dist" -czf "$tarball" "$package_name"
echo "package-linux: wrote $tarball"

if command -v dpkg-deb >/dev/null 2>&1; then
  deb_root="$dist/deb-root"
  rm -rf "$deb_root"
  mkdir -p "$deb_root/DEBIAN" "$deb_root/usr"
  cp -R "$package_dir/bin" "$deb_root/usr/"
  cp -R "$package_dir/share" "$deb_root/usr/"
  chmod 0755 "$deb_root/usr/bin/pile"

  arch="amd64"
  case "$target" in
    aarch64-unknown-linux-gnu) arch="arm64" ;;
  esac

  installed_size="$(du -sk "$deb_root/usr" | awk '{print $1}')"
  cat > "$deb_root/DEBIAN/control" <<CONTROL
Package: pile
Version: $version
Section: editors
Priority: optional
Architecture: $arch
Maintainer: pile maintainers <noreply@github.com>
Installed-Size: $installed_size
Depends: libc6, libgcc-s1, libx11-6, libxi6, libgl1, libxrandr2, libxcursor1, libxinerama1, libxkbcommon0, libgtk-3-0
Homepage: https://github.com/nikaspran/pile
Description: minimalist infinite scratchpad editor
 pile is a native scratchpad editor for keeping many unsaved notes alive
 without ceremony.
CONTROL

  deb="$dist/pile_${version}_${arch}.deb"
  dpkg-deb --build --root-owner-group "$deb_root" "$deb"
  echo "package-linux: wrote $deb"
else
  echo "package-linux: dpkg-deb not found; skipped .deb package" >&2
fi
