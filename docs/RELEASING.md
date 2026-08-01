# Releasing pile

Releases start from `main`.
Tags use the form `vMAJOR.MINOR.PATCH`, for example `v0.1.0`.

GitHub Actions checks the code, builds the app, creates packages, and publishes
release files.

## Release channels

`stable` comes from version tags.

`continuous` is a prerelease. It is rebuilt after a successful push to `main`.
It is for testing.

## Create a release

1. Set the same version in these files:

   - `Cargo.toml`;
   - `Cargo.toml` under `[package.metadata.bundle]`;
   - `assets/Info.plist`;
   - `CHANGELOG.md`.

2. Commit and push `main`.

3. Check the release:

```sh
scripts/release.sh v0.1.0
```

4. Create and push the tag:

```sh
scripts/release.sh v0.1.0 --execute
```

The script checks the branch, working tree, versions, and tag.

CI then runs:

- format checks;
- Clippy;
- tests;
- release builds;
- package jobs;
- checksum and manifest generation;
- GitHub Release publishing.

## Release files

CI publishes:

- macOS archives for Intel and Apple silicon;
- a Windows portable zip;
- a Linux tarball;
- a Linux Debian package;
- `SHA256SUMS`;
- `pile-update-manifest.json`.

Artifact names include the version and target. The exact names are printed in
the GitHub Release.

## Application icons

`assets/pile-logo.svg` is the canonical logo source. Regenerate the committed
platform assets with:

```sh
scripts/generate-icons.sh
```

This updates the macOS/Linux PNG icon set, the macOS `AppIcon.appiconset`,
`assets/pile.icns`, and `assets/pile.ico`. ImageMagick is required. On macOS,
`actool` compiles the asset catalog into `Assets.car` during packaging, while
the `.icns` file remains a compatibility fallback.

## Signing

Unsigned packages still build.

macOS signing and notarization use these repository secrets:

- `APPLE_CERTIFICATE_P12`;
- `APPLE_CERTIFICATE_PASSWORD`;
- `APPLE_CODESIGN_IDENTITY`;
- `APPLE_NOTARIZE`;
- `APPLE_ID`;
- `APPLE_TEAM_ID`;
- `APPLE_APP_SPECIFIC_PASSWORD`.

Windows signing uses `WINDOWS_SIGNTOOL_CERT_SHA1` when the certificate is
available on the runner.

Release metadata can be signed with `GPG_PRIVATE_KEY` and `GPG_PASSPHRASE`.

## After the release

Check that:

- every expected file is present;
- the checksums include the packages and manifest;
- the manifest URLs point to the release;
- the release notes are correct;
- at least one package starts on each platform you can test.

## Current limits

- Windows has a portable zip. It does not have MSI or MSIX.
- Linux has a tarball and a Debian package. It does not have AppImage, Flatpak,
  or RPM packages.
- Automatic update apply works on macOS first.
