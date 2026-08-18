# Security Policy

## Scope

Convenient Window is a Windows 11 desktop utility that processes global input, window metadata, screenshots, local configuration, and local IPC messages. Security reports are welcome for the standalone desktop application and the shared Rust helper in this repository.

The following areas are particularly relevant:

- Unauthorized or unintended data access, collection, or disclosure
- Local IPC authentication, authorization, or token handling
- Global input handling and synthetic input safety
- Window, screenshot, OCR, or clipboard operations that cross their intended boundaries
- Command execution, helper supervision, and process isolation
- Installer, portable package, update, or release-integrity issues
- Dependency or build-pipeline issues that could affect distributed artifacts

## Supported Versions

Security fixes are currently focused on:

| Version or branch | Support status |
| --- | --- |
| Latest stable release | Supported |
| `main` | Supported |
| `develop` | Supported for active development issues |
| Older releases and unmaintained branches | Upgrade to a supported version first |

Support status may change as new releases are published. Please include the exact application version, helper version, or commit when reporting an issue.

## Reporting a Vulnerability

Please do not open a public issue for a suspected vulnerability. Public disclosure can expose users before a fix is available.

Use GitHub's private vulnerability reporting for this repository:

**Repository > Security > Advisories > Report a vulnerability**

If private vulnerability reporting is unavailable, open a minimal public issue asking for a private contact channel. Do not include exploit details, credentials, tokens, personal data, or other sensitive material in that issue.

A useful report should include:

- A clear description of the security impact
- The affected version, branch, or commit
- The Windows version and relevant hardware or privilege context
- Reproduction steps or a minimal proof of concept
- Expected and observed behavior
- Relevant logs or screenshots with secrets and personal data removed
- Any suggested mitigation, if known

Please allow reasonable time for investigation and remediation before sharing details publicly. Coordinated disclosure helps protect users and maintainers.

## Response and Disclosure

Reports are reviewed in good faith and prioritized according to impact, exploitability, affected users, and the availability of a practical mitigation. An acknowledgement is normally targeted within seven calendar days, but this is not a guaranteed response or remediation deadline.

The maintainers may request additional information, provide a mitigation, publish a fix, or recommend upgrading to a supported release. When appropriate, a security advisory or release note will describe the issue without exposing unnecessary exploitation details.

No bug bounty or monetary compensation is currently offered.

## Out of Scope

The following are generally outside the security scope unless they demonstrate a broader security impact:

- Feature requests or ordinary bugs without a security consequence
- Issues limited to unsupported operating systems or modified builds
- Problems requiring physical access to an already unlocked device
- Social engineering, denial-of-service claims without meaningful user impact, or spam
- Vulnerabilities in third-party software that are not caused or materially worsened by this project

## Safe Handling

Please remove passwords, access tokens, private keys, personal data, and unrelated files before sending logs or screenshots. Do not test against another person's device, data, or account without explicit authorization.
