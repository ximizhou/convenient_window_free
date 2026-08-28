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
    use fs2::FileExt;
    use std::fs::{File, OpenOptions};
    use std::path::PathBuf;

    pub struct SingleInstance {
        file: File,
    }

    impl SingleInstance {
        pub fn acquire() -> Result<Option<Self>> {
            let path = lock_path()?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(path)?;
            match file.try_lock_exclusive() {
                Ok(()) => Ok(Some(Self { file })),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
                Err(error) => Err(error.into()),
            }
        }
    }

    impl Drop for SingleInstance {
        fn drop(&mut self) {
            let _ = FileExt::unlock(&self.file);
        }
    }

    fn lock_path() -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
            if path.is_absolute() {
                return Ok(path.join("convenient-window-helper.lock"));
            }
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| anyhow::anyhow!("HOME is required for the Unix helper instance lock"))?;
        Ok(home
            .join(".cache")
            .join("convenient-window")
            .join("helper.lock"))
    }
}

pub use imp::SingleInstance;

#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    use super::*;

    #[test]
    fn unix_instance_lock_rejects_a_second_owner_and_recovers() {
        let first = SingleInstance::acquire().unwrap().unwrap();
        assert!(SingleInstance::acquire().unwrap().is_none());
        drop(first);
        assert!(SingleInstance::acquire().unwrap().is_some());
    }
}
