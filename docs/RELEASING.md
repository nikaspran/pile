# Releasing pile

Releases are cut from `main` with annotated version tags. Tags must use the
format `vMAJOR.MINOR.PATCH`, for example `v0.1.0`.

GitHub Actions runs checks, builds release artifacts, and publishes the GitHub
Release when a `v*` tag is pushed. Current artifacts are plain platform
binaries; signed installers and app bundles are future packaging work.

## Cut a Release

1. Update `Cargo.toml` version if needed.
2. Update `CHANGELOG.md`.
3. Commit and push `main`.
4. Run the release script in dry-run mode:

```sh
scripts/release.sh v0.1.0
```

5. If the dry run is correct, execute it:

```sh
scripts/release.sh v0.1.0 --execute
```

The script verifies:

- the working tree is clean;
- the current branch is `main`;
- `Cargo.toml` version matches the tag without the leading `v`;
- the tag does not already exist locally or on `origin`.

Then it creates an annotated tag, pushes `main`, and pushes the tag. CI does the
formatting, Clippy, test, release-build, artifact-upload, and GitHub Release
publishing work from the pushed tag.

## After Tag Push

Verify the GitHub Release:

- all expected platform artifacts were uploaded;
- release notes are readable;
- at least one downloaded artifact launches successfully.

## Current Limitations

- Artifacts are raw binaries, not signed installers.
- macOS artifacts are not notarized.
- Windows artifacts are not Authenticode signed.
- Linux artifacts are not yet packaged as AppImage, Flatpak, deb, or rpm.
