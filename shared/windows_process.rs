//! Process handles used across the desktop/helper privilege boundary.
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, GetProcessId, GetProcessTimes, OpenProcess,
    OpenProcessToken, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};

pub struct ProcessHandle(isize);

impl ProcessHandle {
    pub fn new(handle: HANDLE) -> Self {
        Self(handle.0 as isize)
    }

    pub fn raw(&self) -> HANDLE {
        HANDLE(self.0 as *mut _)
    }

    pub fn open(pid: u32) -> Result<Self, String> {
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )
        }
        .map(Self::new)
        .map_err(|error| error.to_string())
    }

    pub fn id(&self) -> u32 {
        unsafe { GetProcessId(self.raw()) }
    }

    pub fn birth(&self) -> Result<u64, String> {
        process_birth(self.raw())
    }

    pub fn elevated(&self) -> Result<bool, String> {
        is_elevated(self.raw())
    }

    pub fn try_wait(&self) -> Result<Option<i32>, String> {
        unsafe {
            match WaitForSingleObject(self.raw(), 0) {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => {
                    let mut code = 0;
                    GetExitCodeProcess(self.raw(), &mut code).map_err(|error| error.to_string())?;
                    Ok(Some(code as i32))
                }
                _ => Err("Unable to wait for helper process".into()),
            }
        }
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.raw());
        }
    }
}

pub fn current_elevated() -> Result<bool, String> {
    is_elevated(unsafe { GetCurrentProcess() })
}

pub fn current_birth() -> Result<u64, String> {
    process_birth(unsafe { GetCurrentProcess() })
}

fn is_elevated(process: HANDLE) -> Result<bool, String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).map_err(|error| error.to_string())?;
        let token = ProcessHandle::new(token);
        let mut elevation = TOKEN_ELEVATION::default();
        let mut length = 0;
        GetTokenInformation(
            token.raw(),
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        )
        .map_err(|error| error.to_string())?;
        Ok(elevation.TokenIsElevated != 0)
    }
}

fn process_birth(process: HANDLE) -> Result<u64, String> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) }
        .map_err(|error| error.to_string())?;
    Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

pub fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

pub fn ptr(value: &[u16]) -> PCWSTR {
    PCWSTR(value.as_ptr())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opened_process_keeps_identity_and_elevation() {
        let process = ProcessHandle::open(std::process::id()).unwrap();
        assert_eq!(process.id(), std::process::id());
        assert_eq!(process.birth().unwrap(), current_birth().unwrap());
        assert_eq!(process.elevated().unwrap(), current_elevated().unwrap());
        assert_eq!(process.try_wait().unwrap(), None);
    }
}
