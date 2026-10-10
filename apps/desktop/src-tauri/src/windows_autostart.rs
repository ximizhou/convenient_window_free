use std::path::Path;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

pub const STARTUP_NAME: &str = "Convenient Window";
const LEGACY_NAMES: [&str; 2] = ["便捷窗口", "convenient-window"];
const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APPROVED: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";

fn command_request(command: &str, executable: &Path) -> Option<bool> {
    let command = command.trim();
    let (path, args) = if let Some(quoted) = command.strip_prefix('"') {
        let Some((path, args)) = quoted.split_once('"') else {
            return None;
        };
        (path, args.trim())
    } else {
        let Some(path) = command.strip_suffix(" --autostart") else {
            return None;
        };
        (path, "--autostart")
    };
    // Upgrades retain the install directory. A different path belongs to another copy.
    if !Path::new(path).is_absolute() || !path.eq_ignore_ascii_case(&executable.to_string_lossy()) {
        return None;
    }
    match args {
        "--autostart" => Some(false),
        "--autostart --request-admin" => Some(true),
        _ => None,
    }
}

fn is_our_command(command: &str, executable: &Path) -> bool {
    command_request(command, executable).is_some()
}

fn startup_command(executable: &Path, request_admin: bool) -> String {
    format!(
        "\"{}\" --autostart{}",
        executable.display(),
        if request_admin {
            " --request-admin"
        } else {
            ""
        }
    )
}

#[derive(serde::Serialize)]
pub struct AdminStartupState {
    pub enabled: bool,
}

pub fn admin_request_state() -> std::io::Result<AdminStartupState> {
    let enabled = if enabled_for_current_executable()? {
        let command: String = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(RUN)?
            .get_value(STARTUP_NAME)?;
        command_request(&command, &std::env::current_exe()?) == Some(true)
    } else {
        false
    };
    Ok(AdminStartupState { enabled })
}

pub fn set_admin_request(enabled: bool) -> std::io::Result<()> {
    if enabled {
        return enable_request(Some(true));
    }
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let run = match user.open_subkey_with_flags(RUN, KEY_READ | KEY_WRITE) {
        Ok(run) => run,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let command: String = match run.get_value(STARTUP_NAME) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let executable = std::env::current_exe()?;
    if command_request(&command, &executable) == Some(true) {
        // Removing the request must preserve Task Manager's disabled state.
        run.set_value(STARTUP_NAME, &startup_command(&executable, false))?;
    }
    Ok(())
}

/// Write the new entry and its OS override before removing the old names.
/// A pre-existing new entry wins, including a Task Manager disabled state.
fn migrate(run: &RegKey, approved: Option<&RegKey>, executable: &Path) -> std::io::Result<()> {
    for name in LEGACY_NAMES {
        let old: String = match run.get_value(name) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if !is_our_command(&old, executable) {
            continue;
        }
        match run.get_value::<String, _>(STARTUP_NAME) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(key) = approved {
                    match key.get_raw_value(name) {
                        Ok(value) => key.set_raw_value(STARTUP_NAME, &value)?,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error),
                    }
                }
                run.set_value(
                    STARTUP_NAME,
                    &format!("\"{}\" --autostart", executable.display()),
                )?;
            }
            Ok(command) if is_our_command(&command, executable) => {}
            Ok(_) => continue,
            Err(error) => return Err(error),
        }
        run.delete_value(name)?;
        if let Some(key) = approved {
            match key.delete_value(name) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}

pub fn migrate_current_user() -> std::io::Result<()> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let run = match user.open_subkey_with_flags(RUN, KEY_READ) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let executable = std::env::current_exe()?;
    if !LEGACY_NAMES.iter().any(|name| {
        run.get_value::<String, _>(name)
            .is_ok_and(|command| is_our_command(&command, &executable))
    }) {
        return Ok(());
    }
    let run = user.open_subkey_with_flags(RUN, KEY_READ | KEY_WRITE)?;
    let approved = match user.open_subkey_with_flags(APPROVED, KEY_READ | KEY_WRITE) {
        Ok(key) => Some(key),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    migrate(&run, approved.as_ref(), &executable)
}

/// auto-launch 0.5 omits path quotes. Write the complete command in one operation.
pub fn enable() -> std::io::Result<()> {
    enable_request(None)
}

fn registered_request(run: &RegKey, executable: &Path) -> std::io::Result<Option<bool>> {
    match run.get_value::<String, _>(STARTUP_NAME) {
        Ok(command) => Ok(command_request(&command, executable)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn delete_if_present(key: &RegKey, name: &str) -> std::io::Result<()> {
    match key.delete_value(name) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn write_request(
    run: &RegKey,
    approved: Option<&RegKey>,
    executable: &Path,
    request_admin: Option<bool>,
) -> std::io::Result<()> {
    // Task Manager may disable the entry without removing its command. Re-enabling
    // startup must retain that command's administrator preference, not its active state.
    let request_admin = match request_admin {
        Some(value) => value,
        None => registered_request(run, executable)?.unwrap_or(false),
    };
    run.set_value(STARTUP_NAME, &startup_command(executable, request_admin))?;
    if let Some(approved) = approved {
        delete_if_present(approved, STARTUP_NAME)?;
    }
    Ok(())
}

fn approved_key(user: &RegKey, flags: u32) -> std::io::Result<Option<RegKey>> {
    match user.open_subkey_with_flags(APPROVED, flags) {
        Ok(key) => Ok(Some(key)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn enable_request(request_admin: Option<bool>) -> std::io::Result<()> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = user.create_subkey(RUN)?;
    let approved = approved_key(&user, KEY_WRITE)?;
    write_request(
        &run,
        approved.as_ref(),
        &std::env::current_exe()?,
        request_admin,
    )
}

fn disable_registered(
    run: &RegKey,
    approved: Option<&RegKey>,
    executable: &Path,
) -> std::io::Result<()> {
    // A portable copy must not remove the installed copy's startup registration.
    if registered_request(run, executable)?.is_some() {
        delete_if_present(run, STARTUP_NAME)?;
        if let Some(approved) = approved {
            delete_if_present(approved, STARTUP_NAME)?;
        }
    }
    Ok(())
}

pub fn disable() -> std::io::Result<()> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let run = match user.open_subkey_with_flags(RUN, KEY_READ | KEY_WRITE) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let approved = approved_key(&user, KEY_WRITE)?;
    disable_registered(&run, approved.as_ref(), &std::env::current_exe()?)
}

pub fn enabled_for_current_executable() -> std::io::Result<bool> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let run = match user.open_subkey(RUN) {
        Ok(run) => run,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let command: String = match run.get_value(STARTUP_NAME) {
        Ok(command) => command,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !is_our_command(&command, &std::env::current_exe()?) {
        return Ok(false);
    }
    match user
        .open_subkey(APPROVED)
        .and_then(|key| key.get_raw_value(STARTUP_NAME))
    {
        Ok(value) => Ok(!matches!(value.bytes.first(), Some(3 | 7))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winreg::{enums::RegType::REG_BINARY, RegValue};

    #[test]
    fn login_request_is_explicit_and_bound_to_the_registered_copy() {
        let exe = Path::new(r"C:\Apps with spaces\ConvenientWindow.exe");
        assert_eq!(
            command_request(&startup_command(exe, false), exe),
            Some(false)
        );
        assert_eq!(
            command_request(&startup_command(exe, true), exe),
            Some(true)
        );
        assert_eq!(
            command_request(&startup_command(exe, true), Path::new(r"C:\Other.exe")),
            None
        );
        assert_eq!(
            command_request(&format!("{} --extra", startup_command(exe, true)), exe),
            None
        );
        assert_eq!(
            command_request(
                r#""C:\Apps with spaces\ConvenientWindow.exe" --request-admin"#,
                exe
            ),
            None
        );
    }

    #[test]
    fn startup_updates_retain_disabled_admin_request_and_respect_other_copies() {
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!("Software\\ConvenientWindowTests-{}", uuid::Uuid::new_v4());
        let (root, _) = user.create_subkey(&path).unwrap();
        let (run, _) = root.create_subkey("Run").unwrap();
        let (approved, _) = root.create_subkey("Approved").unwrap();
        let result = std::panic::catch_unwind(|| {
            let executable = Path::new(r"C:\Apps with spaces\ConvenientWindow.exe");
            let other = Path::new(r"C:\Portable\ConvenientWindow.exe");
            let disabled = RegValue {
                bytes: vec![3, 0, 0, 0, 1, 2, 3, 4, 0, 0, 0, 0],
                vtype: REG_BINARY,
            };
            run.set_value(STARTUP_NAME, &startup_command(executable, true))
                .unwrap();
            approved.set_raw_value(STARTUP_NAME, &disabled).unwrap();
            write_request(&run, Some(&approved), executable, None).unwrap();
            assert_eq!(registered_request(&run, executable).unwrap(), Some(true));
            assert!(approved.get_raw_value(STARTUP_NAME).is_err());

            approved.set_raw_value(STARTUP_NAME, &disabled).unwrap();
            disable_registered(&run, Some(&approved), other).unwrap();
            assert_eq!(registered_request(&run, executable).unwrap(), Some(true));
            assert_eq!(
                approved.get_raw_value(STARTUP_NAME).unwrap().bytes,
                disabled.bytes
            );
            disable_registered(&run, Some(&approved), executable).unwrap();
            disable_registered(&run, Some(&approved), executable).unwrap();
            assert_eq!(registered_request(&run, executable).unwrap(), None);
            assert!(approved.get_raw_value(STARTUP_NAME).is_err());

            // An explicit app-level disable clears the administrator preference.
            write_request(&run, Some(&approved), executable, None).unwrap();
            assert_eq!(registered_request(&run, executable).unwrap(), Some(false));
            write_request(&run, Some(&approved), executable, Some(true)).unwrap();
            assert_eq!(registered_request(&run, executable).unwrap(), Some(true));
        });
        drop(run);
        drop(approved);
        drop(root);
        user.delete_subkey_all(&path).unwrap();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }

    #[test]
    fn migration_preserves_disabled_state_is_idempotent_and_respects_new_entry() {
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!("Software\\ConvenientWindowTests-{}", uuid::Uuid::new_v4());
        let (root, _) = user.create_subkey(&path).unwrap();
        let (run, _) = root.create_subkey("Run").unwrap();
        let (approved, _) = root.create_subkey("Approved").unwrap();
        let result = std::panic::catch_unwind(|| {
            let executable = Path::new("C:\\Apps\\Convenient Window\\convenient-window.exe");
            let disabled = RegValue {
                bytes: vec![3, 0, 0, 0, 1, 2, 3, 4, 0, 0, 0, 0],
                vtype: REG_BINARY,
            };
            run.set_value(
                LEGACY_NAMES[0],
                &format!("{} --autostart", executable.display()),
            )
            .unwrap();
            approved.set_raw_value(LEGACY_NAMES[0], &disabled).unwrap();
            let read_only_run = root.open_subkey_with_flags("Run", KEY_READ).unwrap();
            assert!(migrate(&read_only_run, Some(&approved), executable).is_err());
            assert!(run.get_raw_value(LEGACY_NAMES[0]).is_ok());
            assert!(run.get_raw_value(STARTUP_NAME).is_err());
            drop(read_only_run);
            migrate(&run, Some(&approved), executable).unwrap();
            assert_eq!(
                approved.get_raw_value(STARTUP_NAME).unwrap().bytes,
                disabled.bytes
            );
            let command: String = run.get_value(STARTUP_NAME).unwrap();
            assert_eq!(
                command,
                "\"C:\\Apps\\Convenient Window\\convenient-window.exe\" --autostart"
            );
            assert!(run.get_raw_value(LEGACY_NAMES[0]).is_err());
            migrate(&run, Some(&approved), executable).unwrap();
            run.set_value(
                LEGACY_NAMES[1],
                &format!("\"{}\" --autostart", executable.display()),
            )
            .unwrap();
            migrate(&run, Some(&approved), executable).unwrap();
            assert_eq!(run.get_value::<String, _>(STARTUP_NAME).unwrap(), command);
            assert!(run.get_raw_value(LEGACY_NAMES[1]).is_err());
            assert_eq!(
                approved.get_raw_value(STARTUP_NAME).unwrap().bytes,
                disabled.bytes
            );
            let foreign = "C:\\Other\\convenient-window.exe --autostart";
            run.set_value(LEGACY_NAMES[0], &foreign).unwrap();
            migrate(&run, Some(&approved), executable).unwrap();
            assert_eq!(
                run.get_value::<String, _>(LEGACY_NAMES[0]).unwrap(),
                foreign
            );
            run.set_value(LEGACY_NAMES[0], &command).unwrap();
            run.set_value(STARTUP_NAME, &foreign).unwrap();
            migrate(&run, Some(&approved), executable).unwrap();
            assert_eq!(
                run.get_value::<String, _>(LEGACY_NAMES[0]).unwrap(),
                command
            );
            assert_eq!(run.get_value::<String, _>(STARTUP_NAME).unwrap(), foreign);
        });
        drop(run);
        drop(approved);
        drop(root);
        user.delete_subkey_all(&path).unwrap();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }

    #[test]
    fn unrelated_or_malformed_startup_commands_are_preserved() {
        let exe = Path::new("C:\\Apps\\convenient-window.exe");
        assert!(!is_our_command("C:\\other.exe --autostart", exe));
        assert!(!is_our_command(
            "C:\\Other\\convenient-window.exe --autostart",
            exe
        ));
        assert!(!is_our_command("convenient-window.exe --autostart", exe));
        assert!(is_our_command(
            "\"c:\\apps\\convenient-window.exe\" --autostart",
            exe
        ));
        assert!(!is_our_command(
            "\"C:\\convenient-window.exe --autostart",
            exe
        ));
        assert!(!is_our_command("C:\\convenient-window.exe --other", exe));
    }
}
