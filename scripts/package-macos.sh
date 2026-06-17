#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

version="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
target="${TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
binary="${BINARY_PATH:-target/$target/release/pile}"
dist="${DIST_DIR:-dist}"
bundle_version="${BUNDLE_VERSION:-${GITHUB_RUN_NUMBER:-1}}"
app_dir="$dist/pile.app"
zip_path="$dist/pile-$version-$target-macos.zip"

if [[ ! -x "$binary" ]]; then
  echo "package-macos: binary not found or not executable: $binary" >&2
  exit 1
fi

rm -rf "$app_dir"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
cp "$binary" "$app_dir/Contents/MacOS/pile"
cp assets/pile.icns "$app_dir/Contents/Resources/pile.icns"
chmod 0755 "$app_dir/Contents/MacOS/pile"

perl \
  -e '
    local $/;
    my $plist = <>;
    $plist =~ s/\$\(DEVELOPMENT_LANGUAGE\)/en/g;
    $plist =~ s/<key>CFBundleShortVersionString<\/key>\s*<string>[^<]*<\/string>/<key>CFBundleShortVersionString<\/key>\n    <string>'"$version"'<\/string>/;
    $plist =~ s/<key>CFBundleVersion<\/key>\s*<string>[^<]*<\/string>/<key>CFBundleVersion<\/key>\n    <string>'"$bundle_version"'<\/string>/;
    print $plist;
  ' assets/Info.plist > "$app_dir/Contents/Info.plist"

if [[ -n "${APPLE_CODESIGN_IDENTITY:-}" ]]; then
  codesign --force --options runtime --timestamp --sign "$APPLE_CODESIGN_IDENTITY" "$app_dir"
else
  codesign --force --sign - "$app_dir"
fi

rm -f "$zip_path"
ditto -c -k --keepParent "$app_dir" "$zip_path"

if [[ "${APPLE_NOTARIZE:-0}" == "1" ]]; then
  : "${APPLE_ID:?APPLE_ID is required when APPLE_NOTARIZE=1}"
  : "${APPLE_TEAM_ID:?APPLE_TEAM_ID is required when APPLE_NOTARIZE=1}"
  : "${APPLE_APP_SPECIFIC_PASSWORD:?APPLE_APP_SPECIFIC_PASSWORD is required when APPLE_NOTARIZE=1}"
  xcrun notarytool submit "$zip_path" \
    --apple-id "$APPLE_ID" \
    --team-id "$APPLE_TEAM_ID" \
    --password "$APPLE_APP_SPECIFIC_PASSWORD" \
    --wait
  xcrun stapler staple "$app_dir"
  rm -f "$zip_path"
  ditto -c -k --keepParent "$app_dir" "$zip_path"
fi

echo "package-macos: wrote $zip_path"
