# Security Policy

`pile` is a local desktop app. The highest-risk issues are data loss, unsafe
session recovery, unexpected file access, and platform packaging problems.

## Supported Versions

Only the latest commit on `main` and the latest tagged release receive fixes.

## Reporting a Vulnerability

If you find a security issue or a credible data-loss bug, please open a private
GitHub security advisory if available. If that is not available, contact the
maintainer through the GitHub repository with enough detail to reproduce the
problem.

Please include:

- operating system and app version or commit;
- steps to reproduce;
- whether scratch buffer contents, session files, backups, or imported/exported
  files are involved;
- any crash logs or terminal output.

Do not publish exploit details or private scratch-buffer contents in public
issues.
