# Updates

`pile` releases publish `pile-update-manifest.json` next to the downloadable
artifacts. The app uses the manifest to check, download, stage, and apply
continuous-channel updates.

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
- artifacts with `name`, `platform`, `target`, `kind`, `sha256`, and `url`.

The generated manifest is not a trust root by itself. The updater verifies the
downloaded artifact hash before staging it. Future signed-update hardening
should verify `pile-update-manifest.json.asc` with a public key embedded in the
app.

## Update Policy

The app checks the `continuous` channel on startup and then daily while running.
Continuous update identity is the source commit, not Cargo semver, so every
successful `main` build can supersede the currently running app.

Downloads are automatic after a newer matching artifact is found. Applying the
staged update requires either the explicit `Restart to Update` menu action or a
later normal app launch with a staged update already present.

Before applying an update, force a final session snapshot through the existing
save worker shutdown path. The updater must preserve the product rule that users
are never asked to save scratch buffers manually.

## Platform Strategy

macOS updates are implemented first. Pile stages the downloaded `.app` bundle
under the app data directory, then an external helper replaces the current
bundle after Pile exits and relaunches the app.

Windows should download a signed installer or portable zip, verify it, then
launch an external updater after `pile` exits. Replacing a running `.exe` from
inside the process is brittle and should be avoided.

Linux should prefer the package manager for `.deb` installs. Portable tarball
installs can support self-replacement only when the install location is
user-writable. AppImage or Flatpak update support can be added later if those
formats become release artifacts.

## Implementation Shape

The app-facing update feature includes `Check for Updates...` and
`Restart to Update` native menu actions:

- fetch the latest manifest from GitHub Releases;
- compare manifest `commit` against `PILE_BUILD_COMMIT`;
- select the artifact matching the current platform and target;
- download automatically after a newer artifact is found;
- verify SHA-256 before staging;
- enable `Restart to Update` only when a verified update is staged.
