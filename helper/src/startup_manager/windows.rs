use super::{Item, Request, Response};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile,
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    FOLDERID_CommonStartup, FOLDERID_Startup, IShellLinkW, SHGetKnownFolderPath, ShellLink,
    KF_FLAG_DONT_VERIFY, SLGP_RAWPATH,
};
use winreg::enums::{
    RegType::{REG_BINARY, REG_EXPAND_SZ, REG_SZ},
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};
use winreg::{RegKey, RegValue};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";
const LIMIT: usize = 512;
const REPARSE_POINT: u32 = 0x400;

struct RegLocation {
    source: &'static str,
    machine: bool,
    path: String,
    approved: String,
    view: u32,
}
struct FolderLocation {
    source: &'static str,
    machine: bool,
    path: PathBuf,
    approved: String,
}
struct Locations {
    registry: Vec<RegLocation>,
    folders: Vec<FolderLocation>,
}
fn root(machine: bool) -> RegKey {
    RegKey::predef(if machine {
        HKEY_LOCAL_MACHINE
    } else {
        HKEY_CURRENT_USER
    })
}
fn known_folder(id: &windows::core::GUID) -> io::Result<PathBuf> {
    unsafe {
        let value = SHGetKnownFolderPath(id, KF_FLAG_DONT_VERIFY, HANDLE::default())
            .map_err(io::Error::other)?;
        let path = value
            .to_string()
            .map(PathBuf::from)
            .map_err(io::Error::other);
        CoTaskMemFree(Some(value.0 as *const _));
        path
    }
}
impl Locations {
    fn system() -> io::Result<Self> {
        Ok(Self {
            registry: vec![
                RegLocation {
                    source: "userRun",
                    machine: false,
                    path: RUN.into(),
                    approved: format!(r"{APPROVED}\Run"),
                    view: KEY_WOW64_64KEY,
                },
                RegLocation {
                    source: "machineRun",
                    machine: true,
                    path: RUN.into(),
                    approved: format!(r"{APPROVED}\Run"),
                    view: KEY_WOW64_64KEY,
                },
                RegLocation {
                    source: "machineRun32",
                    machine: true,
                    path: RUN.into(),
                    approved: format!(r"{APPROVED}\Run32"),
                    view: KEY_WOW64_32KEY,
                },
            ],
            folders: vec![
                FolderLocation {
                    source: "userStartup",
                    machine: false,
                    path: known_folder(&FOLDERID_Startup)?,
                    approved: format!(r"{APPROVED}\StartupFolder"),
                },
                FolderLocation {
                    source: "commonStartup",
                    machine: true,
                    path: known_folder(&FOLDERID_CommonStartup)?,
                    approved: format!(r"{APPROVED}\StartupFolder"),
                },
            ],
        })
    }
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}
struct WriteLock(HANDLE);
impl Drop for WriteLock {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}
fn write_lock() -> io::Result<WriteLock> {
    unsafe {
        let handle = CreateMutexW(
            None,
            false,
            windows::core::w!("Local\\ConvenientWindow.StartupManager.v1"),
        )
        .map_err(io::Error::other)?;
        match WaitForSingleObject(handle, 5000) {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(WriteLock(handle)),
            _ => {
                let _ = CloseHandle(handle);
                Err(io::Error::other("Another startup change is in progress"))
            }
        }
    }
}

pub fn execute(request: Request) -> Response {
    if let Err(error) = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() } {
        return Response::failure("unavailable", error);
    }
    let _apartment = Apartment;
    let locations = match Locations::system() {
        Ok(v) => v,
        Err(e) => return Response::failure("unavailable", e),
    };
    let _lock = if matches!(request, Request::List {}) {
        None
    } else {
        match write_lock() {
            Ok(lock) => Some(lock),
            Err(e) => return Response::failure("conflict", e),
        }
    };
    match request {
        Request::List {} => list(&locations),
        Request::SetEnabled {
            id,
            revision,
            enabled,
        } => match set_enabled(&locations, &id, &revision, enabled) {
            Ok(()) => list(&locations),
            Err(response) => response,
        },
        Request::Add {
            executable_path,
            name,
        } => match add(&locations, &executable_path, name.as_deref()) {
            Ok(()) => list(&locations),
            Err(response) => response,
        },
    }
}

fn approval(machine: bool, path: &str, name: &str) -> io::Result<Option<RegValue>> {
    match root(machine)
        .open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY)
        .and_then(|key| key.get_raw_value(name))
    {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
fn approved_state(value: Option<&RegValue>) -> Option<bool> {
    match value {
        None => Some(true),
        Some(value)
            if value.vtype == REG_BINARY
                && value.bytes.len() == 12
                && value.bytes[1..4] == [0, 0, 0] =>
        {
            match value.bytes[0] {
                2 | 6 => Some(true),
                3 | 7 => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
}
fn revision(source: &str, name: &str, data: &[u8], approval: Option<&RegValue>) -> String {
    // An optimistic concurrency stamp, not an authorization or executable trust hash.
    let mut hash = DefaultHasher::new();
    source.hash(&mut hash);
    name.hash(&mut hash);
    data.hash(&mut hash);
    approval
        .map(|v| (v.vtype.clone() as u32, &v.bytes))
        .hash(&mut hash);
    format!("{:016x}", hash.finish())
}
fn item(
    source: &str,
    name: &str,
    command: String,
    machine: bool,
    data: &[u8],
    approved: io::Result<Option<RegValue>>,
    supported: bool,
) -> Item {
    let readable = approved.is_ok();
    let approved = approved.ok().flatten();
    let enabled = if readable {
        approved_state(approved.as_ref())
    } else {
        None
    };
    let reason = if machine {
        Some("systemItem")
    } else if !readable {
        Some("unreadable")
    } else if !supported {
        Some("unsupportedEntry")
    } else if enabled.is_none() {
        Some("unknownState")
    } else {
        None
    };
    Item {
        id: format!("{source}:{name}"),
        name: name.into(),
        command,
        source: source.into(),
        enabled,
        editable: reason.is_none(),
        reason: reason.map(str::to_owned),
        revision: revision(source, name, data, approved.as_ref()),
    }
}
fn registry_item(location: &RegLocation, name: &str, value: RegValue) -> Item {
    let supported =
        matches!(value.vtype, REG_SZ | REG_EXPAND_SZ) && value.bytes.len().is_multiple_of(2);
    let command = if supported {
        let words: Vec<u16> = value
            .bytes
            .chunks_exact(2)
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
            .collect();
        String::from_utf16_lossy(&words)
            .trim_end_matches('\0')
            .chars()
            .take(8192)
            .collect()
    } else {
        String::new()
    };
    let mut data = value.bytes;
    data.extend_from_slice(&(value.vtype as u32).to_le_bytes());
    item(
        location.source,
        name,
        command,
        location.machine,
        &data,
        approval(location.machine, &location.approved, name),
        supported,
    )
}
fn wide(path: &Path) -> Vec<u16> {
    crate::windows_process::wide(path.as_os_str())
}
fn shortcut_command(path: &Path) -> io::Result<String> {
    unsafe {
        let link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(io::Error::other)?;
        let persist: IPersistFile = link.cast().map_err(io::Error::other)?;
        persist
            .Load(PCWSTR(wide(path).as_ptr()), STGM_READ)
            .map_err(io::Error::other)?;
        let mut target = vec![0; 32768];
        link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)
            .map_err(io::Error::other)?;
        let mut arguments = vec![0; 8192];
        link.GetArguments(&mut arguments)
            .map_err(io::Error::other)?;
        let target = String::from_utf16_lossy(
            &target[..target.iter().position(|v| *v == 0).unwrap_or(target.len())],
        );
        let args = String::from_utf16_lossy(
            &arguments[..arguments
                .iter()
                .position(|v| *v == 0)
                .unwrap_or(arguments.len())],
        );
        if target.is_empty() {
            return Err(io::Error::other("Unsupported shortcut target"));
        }
        Ok(format!("\"{target}\" {args}").trim_end().to_owned())
    }
}
fn folder_item(location: &FolderLocation, path: &Path) -> io::Result<Item> {
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| io::Error::other("Invalid filename"))?;
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.len() > 1024 * 1024 || meta.file_attributes() & REPARSE_POINT != 0 {
        return Ok(item(
            location.source,
            name,
            path.to_string_lossy().into_owned(),
            location.machine,
            &[],
            approval(location.machine, &location.approved, name),
            false,
        ));
    }
    let bytes = fs::read(path)?;
    let command = if path
        .extension()
        .is_some_and(|v| v.eq_ignore_ascii_case("lnk"))
    {
        shortcut_command(path).ok()
    } else {
        None
    };
    let supported = command.is_some();
    Ok(item(
        location.source,
        name,
        command.unwrap_or_else(|| path.to_string_lossy().into_owned()),
        location.machine,
        &bytes,
        approval(location.machine, &location.approved, name),
        supported,
    ))
}
fn list(locations: &Locations) -> Response {
    let mut response = Response {
        ok: true,
        version: 1,
        capability: super::CAPABILITY,
        items: vec![],
        warnings: vec![],
        error: None,
    };
    for location in &locations.registry {
        match root(location.machine)
            .open_subkey_with_flags(&location.path, KEY_READ | location.view)
        {
            Ok(key) => {
                for entry in key.enum_values().take(LIMIT + 1) {
                    if response.items.len() >= LIMIT {
                        response.warnings.push("itemLimit".into());
                        break;
                    }
                    match entry {
                        Ok((name, value)) => {
                            response.items.push(registry_item(location, &name, value))
                        }
                        Err(_) => response
                            .warnings
                            .push(format!("readFailed:{}", location.source)),
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => response
                .warnings
                .push(format!("readFailed:{}", location.source)),
        }
    }
    for location in &locations.folders {
        match fs::read_dir(&location.path) {
            Ok(entries) => {
                for entry in entries.take(LIMIT + 1) {
                    if response.items.len() >= LIMIT {
                        response.warnings.push("itemLimit".into());
                        break;
                    }
                    match entry.and_then(|entry| {
                        if entry.file_name().eq_ignore_ascii_case("desktop.ini") {
                            return Ok(None);
                        }
                        folder_item(location, &entry.path()).map(Some)
                    }) {
                        Ok(Some(item)) => response.items.push(item),
                        Ok(None) => {}
                        Err(_) => response
                            .warnings
                            .push(format!("readFailed:{}", location.source)),
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => response
                .warnings
                .push(format!("readFailed:{}", location.source)),
        }
    }
    response
        .items
        .sort_by_cached_key(|item| (item.name.to_lowercase(), item.source.clone()));
    response.warnings.sort();
    response.warnings.dedup();
    response
}

fn set_enabled(
    locations: &Locations,
    id: &str,
    expected: &str,
    enabled: bool,
) -> Result<(), Response> {
    let (source, name) = id
        .split_once(':')
        .ok_or_else(|| Response::failure("invalidRequest", "Invalid item identifier"))?;
    if name.is_empty() || name.contains('\0') || id.len() > 4096 {
        return Err(Response::failure(
            "invalidRequest",
            "Invalid item identifier",
        ));
    }
    let item = list(locations)
        .items
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| Response::failure("notFound", "Refresh the startup list"))?;
    if !item.editable {
        return Err(Response::failure(
            "readOnly",
            "This startup item is read-only",
        ));
    }
    if item.revision != expected {
        return Err(Response::failure(
            "conflict",
            "The startup item changed; refresh before retrying",
        ));
    }
    let approved_path = if let Some(location) = locations
        .registry
        .iter()
        .find(|v| v.source == source && !v.machine)
    {
        &location.approved
    } else if let Some(location) = locations
        .folders
        .iter()
        .find(|v| v.source == source && !v.machine)
    {
        &location.approved
    } else {
        return Err(Response::failure(
            "readOnly",
            "This startup item is read-only",
        ));
    };
    let current =
        approval(false, approved_path, name).map_err(|e| Response::failure("writeFailed", e))?;
    let state = approved_state(current.as_ref())
        .ok_or_else(|| Response::failure("readOnly", "Unknown Windows startup state"))?;
    if state == enabled {
        return Ok(());
    }
    // Preserve the 2/3 and 6/7 state families; unknown formats are never overwritten.
    let family = current.as_ref().map(|v| v.bytes[0] & !1).unwrap_or(2);
    let mut bytes = vec![0; 12];
    bytes[0] = family + u8::from(!enabled);
    if !enabled {
        let ticks = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            / 100
            + 116444736000000000;
        bytes[4..12].copy_from_slice(&(ticks as u64).to_le_bytes());
    }
    let value = RegValue {
        vtype: REG_BINARY,
        bytes,
    };
    let (key, _) = root(false)
        .create_subkey_with_flags(
            approved_path,
            winreg::enums::KEY_READ | winreg::enums::KEY_WRITE | KEY_WOW64_64KEY,
        )
        .map_err(|e| Response::failure("writeFailed", e))?;
    // Re-read the complete source snapshot immediately before the one-value write.
    if list(locations)
        .items
        .iter()
        .find(|item| item.id == id)
        .is_none_or(|item| item.revision != expected)
    {
        return Err(Response::failure(
            "conflict",
            "The startup item changed; refresh before retrying",
        ));
    }
    key.set_raw_value(name, &value)
        .map_err(|e| Response::failure("writeFailed", e))?;
    if approval(false, approved_path, name)
        .ok()
        .flatten()
        .as_ref()
        .and_then(|v| approved_state(Some(v)))
        != Some(enabled)
    {
        return Err(Response::failure(
            "conflict",
            "Windows startup state changed during the update",
        ));
    }
    Ok(())
}

fn safe_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    !name.is_empty()
        && name.chars().count() <= 80
        && name == name.trim()
        && !name.ends_with('.')
        && !name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        && ![
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&stem.as_str())
}
fn add(
    locations: &Locations,
    executable: &str,
    requested_name: Option<&str>,
) -> Result<(), Response> {
    let bytes = executable.as_bytes();
    let local = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let path = Path::new(executable);
    if !local
        || executable.contains('\0')
        || executable.contains('"')
        || !path
            .extension()
            .is_some_and(|v| v.eq_ignore_ascii_case("exe"))
        || !path.is_file()
    {
        return Err(Response::failure(
            "invalidExecutable",
            "Choose an existing local .exe file",
        ));
    }
    let executable =
        fs::canonicalize(path).map_err(|e| Response::failure("invalidExecutable", e))?;
    let canonical = executable.to_string_lossy();
    let canonical = canonical.strip_prefix(r"\\?\").unwrap_or(&canonical);
    if canonical.as_bytes().get(1) != Some(&b':') {
        return Err(Response::failure(
            "invalidExecutable",
            "Choose an executable on a local drive",
        ));
    }
    let executable = PathBuf::from(canonical);
    let name = requested_name
        .or_else(|| path.file_stem().and_then(|v| v.to_str()))
        .unwrap_or("");
    if !safe_name(name) {
        return Err(Response::failure(
            "invalidName",
            "Choose a short filename without reserved characters",
        ));
    }
    let folder = locations
        .folders
        .iter()
        .find(|v| v.source == "userStartup" && !v.machine)
        .ok_or_else(|| Response::failure("unavailable", "User startup folder unavailable"))?;
    fs::create_dir_all(&folder.path).map_err(|e| Response::failure("writeFailed", e))?;
    let destination = folder.path.join(format!("{name}.lnk"));
    if destination.exists() {
        return Err(Response::failure(
            "alreadyExists",
            "A startup item with this name already exists",
        ));
    }
    // A pre-existing disabled override belongs to a previous item; don't silently revive it.
    if approval(false, &folder.approved, &format!("{name}.lnk"))
        .map_err(|e| Response::failure("writeFailed", e))?
        .is_some()
    {
        return Err(Response::failure(
            "alreadyExists",
            "Windows retains a startup item with this name; use another name",
        ));
    }
    let temporary = folder
        .path
        .join(format!(".cw-startup-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> io::Result<()> {
        let _file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        drop(_file);
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(io::Error::other)?;
            link.SetPath(PCWSTR(wide(&executable).as_ptr()))
                .map_err(io::Error::other)?;
            if let Some(parent) = executable.parent() {
                link.SetWorkingDirectory(PCWSTR(wide(parent).as_ptr()))
                    .map_err(io::Error::other)?;
            }
            let persist: IPersistFile = link.cast().map_err(io::Error::other)?;
            persist
                .Save(PCWSTR(wide(&temporary).as_ptr()), true)
                .map_err(io::Error::other)?;
        }
        // Hard-link creation refuses an existing destination, including concurrent creators.
        fs::hard_link(&temporary, &destination)?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result.map_err(|e| {
        Response::failure(
            if e.kind() == io::ErrorKind::AlreadyExists {
                "alreadyExists"
            } else {
                "writeFailed"
            },
            e,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn approval_formats_are_conservative() {
        assert_eq!(approved_state(None), Some(true));
        for (state, expected) in [
            (2, Some(true)),
            (3, Some(false)),
            (6, Some(true)),
            (7, Some(false)),
            (42, None),
        ] {
            let mut bytes = vec![0; 12];
            bytes[0] = state;
            assert_eq!(
                approved_state(Some(&RegValue {
                    vtype: REG_BINARY,
                    bytes
                })),
                expected
            );
        }
        assert_eq!(
            approved_state(Some(&RegValue {
                vtype: REG_BINARY,
                bytes: vec![2]
            })),
            None
        );
        for name in ["", "../app", "NUL.txt", "a:b", "x.", " x"] {
            assert!(!safe_name(name));
        }
        assert!(safe_name("我的编辑器"));
    }
    #[test]
    fn isolated_registry_changes_preserve_commands_and_reject_stale_or_system_entries() {
        let key_path = format!(
            r"Software\ConvenientWindowStartupTests-{}",
            uuid::Uuid::new_v4()
        );
        let (key, _) = root(false).create_subkey(&key_path).unwrap();
        let run_path = format!(r"{key_path}\Run");
        let approved_path = format!(r"{key_path}\Approved");
        let (run, _) = root(false).create_subkey(&run_path).unwrap();
        let locations = Locations {
            registry: vec![RegLocation {
                source: "userRun",
                machine: false,
                path: run_path,
                approved: approved_path.clone(),
                view: 0,
            }],
            folders: vec![],
        };
        let result = std::panic::catch_unwind(|| {
            run.set_value("test", &r#""C:\Apps\one.exe" --keep-arguments"#)
                .unwrap();
            let first = list(&locations).items.remove(0);
            assert_eq!(first.enabled, Some(true));
            set_enabled(&locations, &first.id, &first.revision, false).unwrap();
            assert_eq!(
                run.get_value::<String, _>("test").unwrap(),
                r#""C:\Apps\one.exe" --keep-arguments"#
            );
            let disabled = list(&locations).items.remove(0);
            assert_eq!(disabled.enabled, Some(false));
            assert!(set_enabled(&locations, &first.id, &first.revision, true).is_err());
            run.set_value("test", &r#""C:\Apps\updated.exe""#).unwrap();
            assert!(set_enabled(&locations, &disabled.id, &disabled.revision, true).is_err());
            let changed = list(&locations).items.remove(0);
            set_enabled(&locations, &changed.id, &changed.revision, true).unwrap();
            assert_eq!(list(&locations).items[0].enabled, Some(true));
            assert!(set_enabled(&locations, "machineRun:test", "x", false).is_err());
            let (approved, _) = root(false).create_subkey(&approved_path).unwrap();
            approved
                .set_raw_value(
                    "test",
                    &RegValue {
                        vtype: REG_BINARY,
                        bytes: vec![42; 12],
                    },
                )
                .unwrap();
            let unknown = list(&locations).items.remove(0);
            assert_eq!(unknown.enabled, None);
            assert!(!unknown.editable);
            assert!(set_enabled(&locations, &unknown.id, &unknown.revision, true).is_err());
        });
        drop(run);
        drop(key);
        root(false).delete_subkey_all(&key_path).unwrap();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }

    #[test]
    fn isolated_shortcuts_are_added_without_launching_or_overwriting_and_can_be_toggled() {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
        }
        let _apartment = Apartment;
        let key_path = format!(
            r"Software\ConvenientWindowStartupTests-{}",
            uuid::Uuid::new_v4()
        );
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/startup-manager-tests")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&directory).unwrap();
        let startup = directory.join("startup");
        let executable = directory.join("example.exe");
        fs::write(&executable, b"fixture only; never executable").unwrap();
        let locations = Locations {
            registry: vec![],
            folders: vec![FolderLocation {
                source: "userStartup",
                machine: false,
                path: startup.clone(),
                approved: key_path.clone(),
            }],
        };
        let result = std::panic::catch_unwind(|| {
            assert!(list(&locations).items.is_empty());
            add(
                &locations,
                &executable.to_string_lossy(),
                Some("Example test"),
            )
            .unwrap();
            let first = list(&locations).items.remove(0);
            assert!(first.command.contains("example.exe"));
            assert!(first.editable);
            let original = fs::read(startup.join("Example test.lnk")).unwrap();
            assert!(add(
                &locations,
                &executable.to_string_lossy(),
                Some("Example test")
            )
            .is_err());
            assert_eq!(
                fs::read(startup.join("Example test.lnk")).unwrap(),
                original
            );
            set_enabled(&locations, &first.id, &first.revision, false).unwrap();
            let disabled = list(&locations).items.remove(0);
            assert_eq!(disabled.enabled, Some(false));
            assert_eq!(
                fs::read(startup.join("Example test.lnk")).unwrap(),
                original
            );
            set_enabled(&locations, &disabled.id, &disabled.revision, true).unwrap();
            assert_eq!(list(&locations).items[0].enabled, Some(true));
            assert!(add(&locations, r"\\server\share\example.exe", Some("Network")).is_err());
            assert!(add(&locations, &executable.to_string_lossy(), Some("../escape")).is_err());
        });
        let _ = root(false).delete_subkey_all(&key_path);
        fs::remove_dir_all(directory).unwrap();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}
