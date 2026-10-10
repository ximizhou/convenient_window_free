use crate::windows_process::{ptr, wide, ProcessHandle};
use std::ffi::OsStr;
use std::path::Path;
use windows::core::w;
use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

pub struct ElevatedProcess {
    process: ProcessHandle,
    stop: ProcessHandle,
}

impl ElevatedProcess {
    pub fn id(&self) -> u32 {
        self.process.id()
    }
    pub fn elevated(&self) -> Result<bool, String> {
        self.process.elevated()
    }
    pub fn try_wait(&self) -> Result<Option<i32>, String> {
        self.process.try_wait()
    }
    pub fn kill(&self) -> Result<(), String> {
        if matches!(self.try_wait(), Ok(Some(_))) {
            return Ok(());
        }
        unsafe {
            let signal = SetEvent(self.stop.raw());
            if WaitForSingleObject(self.process.raw(), 5000) != WAIT_OBJECT_0 {
                return Err(signal.err().map(|e| e.to_string()).unwrap_or_else(|| {
                    "Administrator helper did not stop; close the desktop app before retrying"
                        .into()
                }));
            }
        }
        Ok(())
    }
}

// ShellExecute receives a command line, so preserve quotes and trailing backslashes.
pub fn quote_argument(value: &str) -> String {
    let mut result = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            '\\',
            if ch == '"' { slashes * 2 + 1 } else { slashes },
        ));
        result.push(ch);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n('\\', slashes * 2));
    result.push('"');
    result
}

pub fn launch(
    executable: &Path,
    data_dir: &Path,
    owner_birth: u64,
) -> Result<ElevatedProcess, String> {
    let stop_name = format!(
        "Local\\ConvenientWindow.HelperStop.{}",
        uuid::Uuid::new_v4()
    );
    let stop_wide = wide(OsStr::new(&stop_name));
    let stop = unsafe { CreateEventW(None, true, false, ptr(&stop_wide)) }
        .map(ProcessHandle::new)
        .map_err(|error| error.to_string())?;
    let parameters = format!(
        "--data-dir {} --desktop-owner {} {} --desktop-stop-event {}",
        quote_argument(&data_dir.to_string_lossy()),
        std::process::id(),
        owner_birth,
        quote_argument(&stop_name)
    );
    match run_as_admin(executable, &parameters) {
        Ok(process) => Ok(ElevatedProcess { process, stop }),
        Err(error) => {
            // The shell may have launched a process without returning its handle.
            // Request shutdown, but never treat this as proof that it exited.
            let _ = unsafe { SetEvent(stop.raw()) };
            Err(error)
        }
    }
}

pub fn run_as_admin(executable: &Path, parameters: &str) -> Result<ProcessHandle, String> {
    // ShellExecute may invoke COM shell extensions on this worker thread.
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(|error| error.to_string())?;
    let _apartment = Apartment;
    let file = wide(executable.as_os_str());
    let parameters = wide(OsStr::new(parameters));
    let directory = wide(
        executable
            .parent()
            .ok_or("Helper directory is missing")?
            .as_os_str(),
    );
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: ptr(&file),
        lpParameters: ptr(&parameters),
        lpDirectory: ptr(&directory),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }.map_err(launch_error)?;
    if info.hProcess.is_invalid() {
        return Err("adminLaunchUnconfirmed".into());
    }
    Ok(ProcessHandle::new(info.hProcess))
}

fn launch_error(error: windows::core::Error) -> String {
    if error.code().0 as u32 == 0x800704c7 {
        "adminCancelled".into()
    } else {
        format!("adminLaunchFailed: {error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_uac_and_denied_launch_are_distinguished_without_prompting() {
        use windows::core::{Error, HRESULT};
        assert_eq!(
            launch_error(Error::from_hresult(HRESULT(0x800704c7_u32 as i32))),
            "adminCancelled"
        );
        let denied = launch_error(Error::from_hresult(HRESULT(0x80070005_u32 as i32)));
        assert!(denied.starts_with("adminLaunchFailed:"));
        assert_ne!(denied, "adminCancelled");
    }

    #[test]
    fn command_line_preserves_spaces_unicode_and_trailing_slash() {
        assert_eq!(
            quote_argument("C:\\测试 folder\\"),
            "\"C:\\测试 folder\\\\\""
        );
        assert_eq!(quote_argument("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote_argument(""), "\"\"");
    }
}
