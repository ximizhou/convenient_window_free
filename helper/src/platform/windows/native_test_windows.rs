//! Test-only windows on a private desktop. Never switch the interactive desktop
//! or enumerate, move, close or otherwise modify the user's existing windows.

use std::ffi::c_void;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

use crate::platform::WindowHandle;

// These bindings are only linked in tests, so the product does not need the
// unrelated desktop-management feature from the Windows bindings crate.
#[link(name = "user32")]
extern "system" {
    fn CreateDesktopW(
        name: *const u16,
        device: *const u16,
        mode: *const c_void,
        flags: u32,
        access: u32,
        security: *const c_void,
    ) -> *mut c_void;
    fn GetThreadDesktop(thread: u32) -> *mut c_void;
    fn SetThreadDesktop(desktop: *mut c_void) -> i32;
    fn CloseDesktop(desktop: *mut c_void) -> i32;
}

/// Create this guard first, then the test windows, on a fresh test thread. All
/// windows must be destroyed before the guard restores the original desktop.
/// No call to SwitchDesktop is made; the private desktop is never displayed.
pub(crate) struct IsolatedDesktop {
    original: *mut c_void,
    temporary: *mut c_void,
}

impl IsolatedDesktop {
    pub(crate) fn new() -> Self {
        unsafe {
            let name: Vec<u16> = format!(
                "ConvenientWindowNativeTest-{}-{}",
                GetCurrentProcessId(),
                GetCurrentThreadId()
            )
            .encode_utf16()
            .chain(Some(0))
            .collect();
            let original = GetThreadDesktop(GetCurrentThreadId());
            assert!(!original.is_null(), "read the test thread desktop");
            let temporary = CreateDesktopW(
                name.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                0x1000_0000, // GENERIC_ALL on this newly created desktop only.
                std::ptr::null(),
            );
            assert!(!temporary.is_null(), "create isolated native test desktop");
            if SetThreadDesktop(temporary) == 0 {
                let _ = CloseDesktop(temporary);
                panic!("attach native test thread to its isolated desktop");
            }
            Self {
                original,
                temporary,
            }
        }
    }
}

impl Drop for IsolatedDesktop {
    fn drop(&mut self) {
        unsafe {
            let _ = SetThreadDesktop(self.original);
            let _ = CloseDesktop(self.temporary);
        }
    }
}

pub(crate) struct TestWindow(HWND);

impl TestWindow {
    pub(crate) fn new(title: &str) -> Self {
        let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        Self(unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                w!("STATIC"),
                PCWSTR(title.as_ptr()),
                WS_POPUP | WS_VISIBLE,
                100,
                100,
                240,
                160,
                HWND::default(),
                None,
                None,
                None,
            )
            .unwrap()
        })
    }

    pub(crate) fn handle(&self) -> WindowHandle {
        WindowHandle(self.0 .0 as isize)
    }

    pub(crate) fn topmost(&self) -> bool {
        (unsafe { GetWindowLongPtrW(self.0, GWL_EXSTYLE) }) as u32 & WS_EX_TOPMOST.0 != 0
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}
