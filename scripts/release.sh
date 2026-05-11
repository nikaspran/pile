#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/release.sh v0.1.0 [--execute]

Cuts a pile release tag and lets CI build/publish the release.

By default this script is a dry run: it validates state and prints the commands
it would run. Pass --execute to create and push the annotated tag.

Required:
  - clean git working tree
  - current branch is main
  - Cargo.toml package.version matches the tag without the leading "v"
  - tag does not already exist locally or on origin
USAGE
}

die() {
  echo "release: $*" >&2
  exit 1
}

run() {
  echo "+ $*"
  if [[ "$EXECUTE" == "1" ]]; then
    "$@"
  fi
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

TAG="${1:-}"
[[ -n "$TAG" ]] || {
  usage
  exit 1
}

EXECUTE=0
if [[ "${2:-}" == "--execute" ]]; then
  EXECUTE=1
elif [[ -n "${2:-}" ]]; then
  die "unknown argument: $2"
fi

[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || \
  die "tag must look like v0.1.0 or v0.1.0-rc.1"

VERSION="${TAG#v}"
CURRENT_BRANCH="$(git branch --show-current)"
[[ "$CURRENT_BRANCH" == "main" ]] || die "release must be cut from main, currently on $CURRENT_BRANCH"

[[ -z "$(git status --porcelain)" ]] || die "working tree is not clean"

CARGO_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
[[ "$CARGO_VERSION" == "$VERSION" ]] || \
  die "Cargo.toml version $CARGO_VERSION does not match tag $TAG"

git rev-parse --verify --quiet "$TAG" >/dev/null && die "local tag already exists: $TAG"

if git ls-remote --exit-code --tags origin "refs/tags/$TAG" >/dev/null 2>&1; then
  die "remote tag already exists on origin: $TAG"
fi

echo "release: preparing $TAG from main"
echo "release: mode $([[ "$EXECUTE" == "1" ]] && echo execute || echo dry-run)"

run git tag -a "$TAG" -m "pile $TAG"
run git push origin main
run git push origin "$TAG"

if [[ "$EXECUTE" == "1" ]]; then
  echo "release: pushed $TAG. GitHub Actions will run checks, build artifacts, and publish the release."
else
  echo "release: dry run complete. Re-run with --execute to create and push $TAG."
fi
