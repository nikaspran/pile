#!/usr/bin/env bash
set -euo pipefail

asset_dir="${1:-dist/release-assets}"
version="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
channel="${RELEASE_CHANNEL:-stable}"
tag="${RELEASE_TAG:-${GITHUB_REF_NAME:-v$version}}"
commit="${GITHUB_SHA:-$(git rev-parse HEAD)}"
repo="${GITHUB_REPOSITORY:-nikaspran/pile}"
base_url="${RELEASE_DOWNLOAD_BASE_URL:-https://github.com/$repo/releases/download/$tag}"
manifest="$asset_dir/pile-update-manifest.json"

mkdir -p "$asset_dir"

tmp="$(mktemp)"
{
  printf '{\n'
  printf '  "version": "%s",\n' "$version"
  printf '  "channel": "%s",\n' "$channel"
  printf '  "tag": "%s",\n' "$tag"
  printf '  "commit": "%s",\n' "$commit"
  printf '  "minimum_session_schema": 5,\n'
  printf '  "artifacts": [\n'

  first=1
  while IFS= read -r -d '' file; do
    name="$(basename "$file")"
    case "$name" in
      SHA256SUMS|pile-update-manifest.json) continue ;;
    esac

    sha="$(sha256sum "$file" | awk '{print $1}')"
    platform="unknown"
    kind="archive"
    case "$name" in
      *apple-darwin*|*macos*) platform="macos" ;;
      *windows*) platform="windows" ;;
      *.deb|*linux*) platform="linux" ;;
    esac
    case "$name" in
      *.deb) kind="deb" ;;
      *.zip) kind="zip" ;;
      *.tar.gz) kind="tar.gz" ;;
    esac

    if [[ "$first" -eq 0 ]]; then
      printf ',\n'
    fi
    first=0

    printf '    {\n'
    printf '      "name": "%s",\n' "$name"
    printf '      "platform": "%s",\n' "$platform"
    printf '      "kind": "%s",\n' "$kind"
    printf '      "sha256": "%s",\n' "$sha"
    printf '      "url": "%s/%s"\n' "$base_url" "$name"
    printf '    }'
  done < <(find "$asset_dir" -maxdepth 1 -type f -print0 | sort -z)

  printf '\n  ]\n'
  printf '}\n'
} > "$tmp"

mv "$tmp" "$manifest"
echo "generate-release-manifest: wrote $manifest"
