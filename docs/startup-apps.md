# Startup applications

The Startup page manages applications that open when a user signs in to Windows.
It lists conventional Run entries and Startup-folder shortcuts. It does not claim
to enumerate services, drivers, scheduled tasks, RunOnce entries, or every packaged
application startup task.

## First version

- Search the application name or command and filter enabled/disabled entries.
- Enable or disable supported entries for the current user. Disabling preserves
  the original command or shortcut so it can be enabled again.
- Select a local Windows executable, confirm its name, and create a new shortcut
  in the user's Startup folder. Adding an entry does not launch the application.
- Display machine Run entries and the common Startup folder as read-only.
- Refresh when the page opens, when the application regains focus, or on request.
  There is no resident startup watcher or automatic cleanup.

Unknown approval states and unreadable entries remain visible but read-only.
Partial enumeration failures are reported; an incomplete list is not presented as
proof that there are no startup applications. A write uses the entry identity and
revision returned by the last read and rejects a changed or missing entry.

## Process and permissions

The ordinary host launches the validated helper executable as a short-lived
`--startup-manager` process with a single JSON request on stdin. This branch runs
before runtime initialization: it does not start the window/input engine, acquire
the runtime singleton, load user configuration, or connect to its WebSocket.
It rejects elevated tokens, so a helper session elevated with another account
cannot accidentally change that account's current-user startup settings.

Requests have a closed schema and a 16 KiB limit. Responses are bounded to 1 MiB;
hosts use a timeout and never invoke a command shell. Sources and writable
locations are selected by the backend, not supplied as arbitrary registry paths.
The API version is independent of the runtime's protocol v7 and configuration v9.
Old helper executables must be rejected before attempting this command, because
they do not implement the short-lived operation.

Windows stores Task Manager approval information under `StartupApproved`. This
binary format is not a documented stable public API. The implementation accepts
only known shapes and states; unknown values are not guessed or overwritten.
Compatibility must be checked when supported Windows versions change.

## Validation

Pure tests cover request validation, identities, approval states and revisions.
Native tests use dedicated temporary registry keys and folders, never existing
startup entries. They exercise shortcut creation, reversible state changes and
conflicting updates. Browser tests use the production UI with mocked host APIs;
they do not demonstrate that an application ran at the next Windows sign-in.

Real acceptance requires an explicitly selected disposable application: add it,
verify Windows sees the entry, disable/re-enable it, and check the next sign-in.
The regular application's settings, startup entries and running processes must
not be used as implicit test fixtures.

## References

The implementation is original. The references below informed platform behavior
and interaction choices; their code, graphics and wording were not copied.

- [Microsoft: Run and RunOnce registry keys](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys)
- [Microsoft: known folder identifiers](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid)
- [Microsoft: Shell links](https://learn.microsoft.com/en-us/windows/win32/shell/links)
- [Bulk Crap Uninstaller: startup-entry handling](https://github.com/BCUninstaller/Bulk-Crap-Uninstaller/tree/master/source/UninstallTools/Startup)
