use anyhow::Result;

#[cfg(target_os = "windows")]
mod imp {
    use super::Result;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
    use windows::Win32::System::Threading::CreateMutexW;

    pub struct SingleInstance {
        handle: HANDLE,
    }

    const INSTANCE_MUTEX_NAME: PCWSTR = w!("Global\\ConvenientWindowHelper");

    impl SingleInstance {
        pub fn acquire() -> Result<Option<Self>> {
            let handle = unsafe { CreateMutexW(None, false, INSTANCE_MUTEX_NAME)? };
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                let _ = unsafe { CloseHandle(handle) };
                return Ok(None);
            }

            Ok(Some(Self { handle }))
        }
    }

    impl Drop for SingleInstance {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.handle) };
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::Result;

    pub struct SingleInstance;

    impl SingleInstance {
        pub fn acquire() -> Result<Option<Self>> {
            Ok(Some(Self))
        }
    }
}

pub use imp::SingleInstance;
