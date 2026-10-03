//! A managed helper exits when the exact desktop process that launched it exits.
use anyhow::{bail, Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;

static MANAGED: AtomicBool = AtomicBool::new(false);

pub fn managed() -> bool {
    MANAGED.load(Ordering::Acquire)
}

pub fn initialize(shutdown: broadcast::Sender<()>) -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let Some(index) = args.iter().position(|arg| arg == "--desktop-owner") else {
        return Ok(());
    };
    let pid: u32 = args
        .get(index + 1)
        .context("Missing desktop PID")?
        .parse()?;
    let birth: u64 = args
        .get(index + 2)
        .context("Missing desktop creation time")?
        .parse()?;
    #[cfg(target_os = "windows")]
    {
        let owner = crate::windows_process::ProcessHandle::open(pid).map_err(anyhow::Error::msg)?;
        if owner.birth().map_err(anyhow::Error::msg)? != birth
            || owner.try_wait().map_err(anyhow::Error::msg)?.is_some()
        {
            bail!("Desktop owner has exited or its PID was reused");
        }
        let stop = if let Some(index) = args.iter().position(|arg| arg == "--desktop-stop-event") {
            use windows::Win32::System::Threading::{OpenEventW, SYNCHRONIZATION_ACCESS_RIGHTS};
            let name = args.get(index + 1).context("Missing desktop stop event")?;
            let name = crate::windows_process::wide(std::ffi::OsStr::new(name));
            Some(
                unsafe {
                    OpenEventW(
                        SYNCHRONIZATION_ACCESS_RIGHTS(0x00100000),
                        false,
                        crate::windows_process::ptr(&name),
                    )
                }
                .map(crate::windows_process::ProcessHandle::new)?,
            )
        } else {
            None
        };
        MANAGED.store(true, Ordering::Release);
        std::thread::Builder::new()
            .name("desktop-owner".into())
            .spawn(move || {
                while matches!(owner.try_wait(), Ok(None)) {
                    if let Some(stop) = &stop {
                        use windows::Win32::Foundation::WAIT_OBJECT_0;
                        use windows::Win32::System::Threading::WaitForSingleObject;
                        if unsafe { WaitForSingleObject(stop.raw(), 0) } == WAIT_OBJECT_0 {
                            break;
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                crate::logging::write_line(
                    "main: desktop owner exited or requested stop; stopping managed helper",
                );
                let _ = shutdown.send(());
                // Bound cleanup even when a native hook or runtime task is stalled.
                std::thread::sleep(std::time::Duration::from_secs(3));
                std::process::exit(0);
            })?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (pid, birth, shutdown);
        bail!("Desktop owner monitoring requires Windows");
    }
    Ok(())
}

pub fn elevated() -> bool {
    #[cfg(target_os = "windows")]
    {
        crate::windows_process::current_elevated().unwrap_or(true)
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}
