# Security Policy

## Supported Versions

Security fixes are provided on the latest commit of the active `develop` branch until the first stable release. Published support ranges will be listed here when releases exist.

## Reporting a Vulnerability

Do not open a public issue for a suspected vulnerability, exposed token, or private user data. Use GitHub's private vulnerability reporting for `ximizhou/convenient_window_free` under **Security > Advisories > Report a vulnerability**.

Include the affected commit, Windows version, reproduction steps, impact, and any relevant logs with secrets removed. You should receive an acknowledgement within seven days. No bounty or response deadline is promised.

## Security Boundary

The app processes global input, window metadata, screenshots, and local configuration. OCR is performed using Windows local APIs. Reports involving authentication tokens, local IPC, command execution actions, sidecar packaging, update or installer integrity, and unintended data capture are in scope.
