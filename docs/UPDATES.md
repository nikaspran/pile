# Updates

`pile` releases publish `pile-update-manifest.json` next to the downloadable
artifacts. The manifest is the contract for a future update checker.

The app must never block typing, session persistence, or startup on network
update checks. Update work should run outside the editor state path and should
only consume immutable release metadata.

## Manifest Contract

Each manifest records:

- release `version`;
- release `channel`;
- release `tag`;
- source `commit`;
- minimum readable session schema;
- artifacts with `name`, `platform`, `kind`, `sha256`, and `url`.

The generated manifest is not a trust root by itself. A production updater must
verify the downloaded artifact hash and should verify
`pile-update-manifest.json.asc` with a public key embedded in the app.

## Update Policy

Use explicit user consent for applying updates. Passive background checks may
surface that a newer version exists, but they should not replace the app while
the user is writing.

Default update checks should use the `stable` channel. The `continuous` channel
is for explicit opt-in testing builds and should never be enabled by default.

Before applying an update, force a final session snapshot through the existing
save worker shutdown path. The updater must preserve the product rule that users
are never asked to save scratch buffers manually.

## Platform Strategy

macOS should update the signed `.app` bundle. A Sparkle-style signed appcast is
the preferred production path once Apple signing and notarization are enabled.

Windows should download a signed installer or portable zip, verify it, then
launch an external updater after `pile` exits. Replacing a running `.exe` from
inside the process is brittle and should be avoided.

Linux should prefer the package manager for `.deb` installs. Portable tarball
installs can support self-replacement only when the install location is
user-writable. AppImage or Flatpak update support can be added later if those
formats become release artifacts.

## Implementation Shape

The first app-facing update feature should be `Check for Updates...`:

- fetch the latest manifest from GitHub Releases;
- compare semver against `env!("CARGO_PKG_VERSION")`;
- show available artifact information;
- download only after user consent;
- verify SHA-256 before launching any installer;
- leave automatic installation for a later, signed-updater pass.
