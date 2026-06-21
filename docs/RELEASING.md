# Releasing pile

Releases are cut from `main` with annotated version tags. Tags must use the
format `vMAJOR.MINOR.PATCH`, for example `v0.1.0`.

GitHub Actions runs checks, builds platform artifacts, generates checksums and
an update manifest, and publishes release assets in two channels:

- `stable`: immutable releases from `v*` tags.
- `continuous`: a rolling prerelease updated after every successful push to
  `main`.

Current release artifacts:

- macOS `.app` bundles zipped per architecture.
- Windows portable zip.
- Linux tarball with desktop metadata.
- Linux Debian package.
- `SHA256SUMS`.
- `pile-update-manifest.json`.

## Continuous Releases

Every commit merged to `main` triggers the build workflow. If checks and all
platform package jobs pass, CI force-moves the `continuous` tag to that commit
and updates the GitHub prerelease named `Continuous build`.

Continuous releases are for early testing and future opt-in update channels.
They are marked as prereleases and are not published as GitHub's latest stable
release.

## Cut a Stable Release

1. Update `Cargo.toml` package version.
2. Update `Cargo.toml` `[package.metadata.bundle].version`.
3. Update `assets/Info.plist` `CFBundleShortVersionString`.
4. Update `CHANGELOG.md`.
5. Commit and push `main`.
6. Run the release script in dry-run mode:

```sh
scripts/release.sh v0.1.0
```

7. If the dry run is correct, execute it:

```sh
scripts/release.sh v0.1.0 --execute
```

The script verifies:

- the working tree is clean;
- the current branch is `main`;
- `Cargo.toml` package version and app metadata match the tag without the
  leading `v`;
- the tag does not already exist locally or on `origin`.

Then it creates an annotated tag, pushes `main`, and pushes the tag. CI does the
formatting, Clippy, test, package-build, artifact-upload, checksum, manifest,
and stable GitHub Release publishing work from the pushed tag.

## Release Assets

CI packages target-specific artifacts:

- `pile-VERSION-x86_64-apple-darwin-macos.zip`
- `pile-VERSION-aarch64-apple-darwin-macos.zip`
- `pile-VERSION-x86_64-pc-windows-msvc-windows.zip`
- `pile-VERSION-x86_64-unknown-linux-gnu-linux.tar.gz`
- `pile_VERSION_amd64.deb`

Both stable and continuous release jobs publish:

- `SHA256SUMS`, generated over all release assets and the update manifest.
- `pile-update-manifest.json`, a machine-readable artifact index with version,
  channel, tag, commit, download URLs, platform labels, target triples, package
  kinds, and SHA-256 hashes.
- `SHA256SUMS.asc` and `pile-update-manifest.json.asc` when `GPG_PRIVATE_KEY`
  is configured.

## Signing and Notarization

Unsigned builds still package successfully. Add these GitHub Actions secrets to
enable signed macOS artifacts:

- `APPLE_CERTIFICATE_P12`: base64-encoded Developer ID Application certificate.
- `APPLE_CERTIFICATE_PASSWORD`: certificate password.
- `APPLE_CODESIGN_IDENTITY`: Developer ID identity name.
- `APPLE_NOTARIZE`: set to `1` to notarize.
- `APPLE_ID`: Apple ID used with notarytool.
- `APPLE_TEAM_ID`: Apple developer team id.
- `APPLE_APP_SPECIFIC_PASSWORD`: app-specific password for notarytool.

Add `WINDOWS_SIGNTOOL_CERT_SHA1` to sign Windows binaries with `signtool` when
the certificate is available in the Windows runner certificate store. If the
certificate is supplied another way, adapt `scripts/package-windows.ps1` to
import it before signing.

Add these secrets to sign release metadata:

- `GPG_PRIVATE_KEY`: ASCII-armored private key.
- `GPG_PASSPHRASE`: passphrase for the private key.

## After Tag Push

Verify the GitHub Release:

- all expected platform artifacts were uploaded;
- `SHA256SUMS` contains every artifact;
- `pile-update-manifest.json` points at the release downloads;
- release notes are readable;
- at least one downloaded artifact launches successfully on each platform you
  can access.

## Current Limitations

- macOS and Windows signing depend on repository secrets and certificates.
- Windows is a portable zip, not yet MSI/MSIX.
- Linux has tarball and `.deb`; AppImage, Flatpak, and `.rpm` are not wired yet.
- Automatic apply is currently macOS-first. Windows and Linux artifacts are
  still published in the manifest for future platform-specific apply backends.
