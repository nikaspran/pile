# Updates

pile can check the continuous release channel.
It reads `pile-update-manifest.json` from GitHub Releases.

Update checks run outside the editor.
They must not block typing, session writes, or startup.

## Manifest

The manifest contains:

- version;
- channel;
- tag;
- source commit;
- minimum session schema;
- package name, platform, target, type, URL, and SHA-256 hash.

The app checks the hash before it stages a package.
The manifest is not a trust root.
Signed manifest verification is future work.

## Update flow

1. pile fetches the manifest.
2. It compares the source commit with the running build.
3. It selects the package for the current platform and target.
4. It downloads and checks the package.
5. It stages the package.
6. The user chooses `Restart to Update`.

pile never asks the user to save notes before an update.

## Platform support

Automatic apply works on macOS.
pile stages the new `.app`, exits, replaces the old app, and starts the new app.

Windows and Linux packages are listed in the manifest.
Automatic apply for those platforms is not implemented yet.
