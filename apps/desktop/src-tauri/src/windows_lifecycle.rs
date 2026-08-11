use std::ffi::c_void;
use std::iter;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::process::Child;
use std::{ffi::OsStr, mem, thread};
use tauri::AppHandle;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
#[cfg(test)]
use windows::Win32::System::Threading::{OpenEventW, SetEvent, EVENT_MODIFY_STATE};

pub const UNINSTALL_SHUTDOWN_EVENT_NAME: &str = "Local\\com.ximizhou.convenientwindow.shutdown";

struct OwnedHandle(isize);

unsafe impl Send for OwnedHandle {}

impl OwnedHandle {
    fn new(handle: HANDLE) -> Self {
        Self(handle.0 as isize)
    }

    fn raw(&self) -> HANDLE {
        HANDLE(self.0 as *mut c_void)
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.raw());
        }
    }
}

pub struct ChildJob {
    _handle: OwnedHandle,
}

impl ChildJob {
    pub fn assign(child: &Child) -> Result<Self, String> {
        let job = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
            .map(OwnedHandle::new)
            .map_err(|error| format!("无法创建 helper 生命周期 Job Object：{error}"))?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        unsafe {
            SetInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const c_void,
                mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .map_err(|error| format!("无法配置 helper 生命周期 Job Object：{error}"))?;
            AssignProcessToJobObject(job.raw(), HANDLE(child.as_raw_handle()))
                .map_err(|error| format!("无法把 helper 加入生命周期 Job Object：{error}"))?;
        }
        Ok(Self { _handle: job })
    }
}

struct ShutdownEvent {
    handle: OwnedHandle,
}

impl ShutdownEvent {
    fn create(name: &str) -> Result<Self, String> {
        let name = wide(name);
        let handle = unsafe { CreateEventW(None, true, false, PCWSTR(name.as_ptr())) }
            .map(OwnedHandle::new)
            .map_err(|error| format!("无法创建卸载退出事件：{error}"))?;
        Ok(Self { handle })
    }

    fn wait(self) -> bool {
        unsafe { WaitForSingleObject(self.handle.raw(), INFINITE) == WAIT_OBJECT_0 }
    }
}

pub fn listen_for_uninstall(app: AppHandle) -> Result<(), String> {
    let event = ShutdownEvent::create(UNINSTALL_SHUTDOWN_EVENT_NAME)?;
    thread::Builder::new()
        .name("desktop-uninstall-listener".to_string())
        .spawn(move || {
            if event.wait() {
                app.exit(0);
            }
        })
        .map(|_| ())
        .map_err(|error| format!("无法启动卸载退出监听：{error}"))
}

#[cfg(test)]
fn signal_event(name: &str) -> Result<(), String> {
    let name = wide(name);
    let handle = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())) }
        .map(OwnedHandle::new)
        .map_err(|error| format!("无法打开卸载退出事件：{error}"))?;
    unsafe { SetEvent(handle.raw()) }.map_err(|error| format!("无法触发卸载退出事件：{error}"))
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    #[test]
    fn named_uninstall_event_can_be_signaled() {
        let name = format!(
            "Local\\com.ximizhou.convenientwindow.shutdown.test.{}",
            uuid::Uuid::new_v4()
        );
        let event = ShutdownEvent::create(&name).unwrap();

        signal_event(&name).unwrap();

        assert!(event.wait());
    }

    #[test]
    fn closing_job_terminates_the_assigned_child() {
        let mut child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .unwrap();
        let job = ChildJob::assign(&child).unwrap();

        drop(job);

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("assigned child survived after its Job Object closed");
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    fn shutdown_event_name_is_stable_for_the_nsis_hook() {
        assert_eq!(
            UNINSTALL_SHUTDOWN_EVENT_NAME,
            "Local\\com.ximizhou.convenientwindow.shutdown"
        );
        assert!(
            include_str!("../windows/installer-hooks.nsh").contains(UNINSTALL_SHUTDOWN_EVENT_NAME)
        );
    }
}
