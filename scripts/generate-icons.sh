#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

source="${LOGO_SOURCE:-assets/pile-logo.svg}"
iconset="${ICONSET_DIR:-assets/icons.iconset}"
mac_icon="${MAC_ICON_PATH:-assets/pile.icns}"
windows_icon="${WINDOWS_ICON_PATH:-assets/pile.ico}"

if [[ ! -f "$source" ]]; then
  echo "generate-icons: source SVG not found: $source" >&2
  exit 1
fi

command -v magick >/dev/null 2>&1 || {
  echo "generate-icons: ImageMagick's magick command is required" >&2
  exit 1
}

mkdir -p "$iconset"
output_dir="$(mktemp -d -t pile-icons)"
trap 'rm -R "$output_dir"' EXIT

render_png() {
  local size="$1"
  local output="$2"
  magick -background none "$source" -resize "${size}x${size}" "PNG32:$output"
}

render_png 16   "$iconset/icon_16x16.png"
render_png 32   "$iconset/icon_16x16@2x.png"
render_png 32   "$iconset/icon_32x32.png"
render_png 64   "$iconset/icon_32x32@2x.png"
render_png 64   "$iconset/icon_64x64.png"
render_png 128  "$iconset/icon_64x64@2x.png"
render_png 128  "$iconset/icon_128x128.png"
render_png 256  "$iconset/icon_128x128@2x.png"
render_png 256  "$iconset/icon_256x256.png"
render_png 512  "$iconset/icon_256x256@2x.png"
render_png 512  "$iconset/icon_512x512.png"
render_png 1024 "$iconset/icon_512x512@2x.png"
render_png 1024 "$iconset/icon_1024x1024.png"

magick -background none "$source" \
  -define icon:auto-resize=256,128,64,48,32,16 \
  "ICO:$output_dir/pile.ico"
mv "$output_dir/pile.ico" "$windows_icon"

if command -v sips >/dev/null 2>&1; then
  sips -s format icns "$iconset/icon_512x512.png" --out "$output_dir/pile.icns" >/dev/null
  mv "$output_dir/pile.icns" "$mac_icon"
elif command -v iconutil >/dev/null 2>&1; then
  iconset_for_icns="$output_dir/pile.iconset"
  mkdir -p "$iconset_for_icns"
  for name in \
    icon_16x16.png icon_16x16@2x.png \
    icon_32x32.png icon_32x32@2x.png \
    icon_128x128.png icon_128x128@2x.png \
    icon_256x256.png icon_256x256@2x.png \
    icon_512x512.png icon_512x512@2x.png; do
    cp "$iconset/$name" "$iconset_for_icns/$name"
  done
  iconutil -c icns "$iconset_for_icns" -o "$output_dir/pile.icns"
  mv "$output_dir/pile.icns" "$mac_icon"
else
  echo "generate-icons: sips/iconutil not found; skipped $mac_icon (run on macOS to generate it)" >&2
fi

echo "generate-icons: updated $iconset, $windows_icon${mac_icon:+, $mac_icon}"
