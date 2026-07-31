use anyhow::{bail, Result};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowArranged, IsWindowVisible, IsZoomed, SetWindowPos, ShowWindow, WindowFromPoint,
    GA_ROOT, GWL_EXSTYLE, GW_OWNER, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SW_RESTORE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};

use crate::platform::{Point, Rect as AppRect, WindowHandle, WindowInfo};

pub fn lock_screen() -> Result<()> {
    unsafe {
        LockWorkStation()?;
    }

    Ok(())
}

pub fn foreground_window() -> Result<Option<WindowInfo>> {
    let hwnd = unsafe { GetForegroundWindow() };
    window_info(hwnd)
}

pub fn window_exists(handle: WindowHandle) -> bool {
    unsafe { IsWindow(HWND(handle.0 as *mut core::ffi::c_void)).as_bool() }
}

pub fn window_is_minimized(handle: WindowHandle) -> bool {
    unsafe { IsIconic(HWND(handle.0 as *mut core::ffi::c_void)).as_bool() }
}

fn window_info(hwnd: HWND) -> Result<Option<WindowInfo>> {
    if hwnd.0.is_null() || !unsafe { IsWindowVisible(hwnd).as_bool() } {
        return Ok(None);
    }

    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
        return Ok(None);
    }

    let rect = window_rect(hwnd)?;
    if rect.width() <= 0 || rect.height() <= 0 {
        return Ok(None);
    }

    let mut process_id = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    }

    let class_name = class_name(hwnd);
    let has_owner = unsafe { GetWindow(hwnd, GW_OWNER) }
        .ok()
        .is_some_and(|owner| !owner.0.is_null());

    Ok(Some(WindowInfo {
        handle: WindowHandle(hwnd.0 as isize),
        rect,
        title: window_text(hwnd),
        transient: has_owner || class_name == "#32768",
        class_name,
        process_name: cached_process_name(process_id),
        maximized: unsafe { IsZoomed(hwnd).as_bool() },
        arranged: unsafe { IsWindowArranged(hwnd).as_bool() },
        topmost: (ex_style & WS_EX_TOPMOST.0) != 0,
    }))
}

pub fn window_info_for_handle(handle: WindowHandle) -> Result<Option<WindowInfo>> {
    window_info(HWND(handle.0 as *mut core::ffi::c_void))
}

pub fn draggable_window_at(point: Point) -> Result<Option<WindowInfo>> {
    let hwnd = unsafe {
        GetAncestor(
            WindowFromPoint(POINT {
                x: point.x,
                y: point.y,
            }),
            GA_ROOT,
        )
    };
    let mut window = match window_info(hwnd)? {
        Some(window) => window,
        None => return Ok(None),
    };
    if window.transient
        || matches!(
            window.class_name.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "#32768"
        )
        || (!window.maximized && window_covers_monitor(hwnd, window.rect))
    {
        return Ok(None);
    }
    if window.maximized {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        window = match window_info(hwnd)? {
            Some(window) => window,
            None => return Ok(None),
        };
    }
    Ok(Some(window))
}

fn window_covers_monitor(hwnd: HWND, rect: AppRect) -> bool {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.0.is_null() {
        return false;
    }
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() } {
        return false;
    }
    rect.left <= info.rcMonitor.left
        && rect.top <= info.rcMonitor.top
        && rect.right >= info.rcMonitor.right
        && rect.bottom >= info.rcMonitor.bottom
}

pub fn set_window_rect(handle: WindowHandle, rect: AppRect) -> Result<()> {
    let hwnd = HWND(handle.0 as *mut core::ffi::c_void);
    unsafe {
        SetWindowPos(
            hwnd,
            HWND::default(),
            rect.left,
            rect.top,
            rect.width().max(1),
            rect.height().max(1),
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER,
        )?;
    }
    Ok(())
}

pub fn toggle_window_topmost_at(point: Option<Point>) -> Result<(String, bool)> {
    let hwnd = unsafe {
        match point {
            Some(point) => {
                let child = WindowFromPoint(POINT {
                    x: point.x,
                    y: point.y,
                });
                GetAncestor(child, GA_ROOT)
            }
            None => GetForegroundWindow(),
        }
    };
    let Some(window) = window_info(hwnd)? else {
        bail!("未找到可置顶的窗口");
    };
    if window.transient
        || matches!(
            window.class_name.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "#32768"
        )
    {
        bail!("该窗口不支持置顶切换");
    }

    let topmost = !window.topmost;
    set_window_topmost(window.handle, topmost)?;
    super::topmost_pin::set_topmost_pin_target(window.handle, topmost);
    Ok((window.title, topmost))
}

pub fn set_window_topmost(handle: WindowHandle, topmost: bool) -> Result<()> {
    let hwnd = HWND(handle.0 as *mut core::ffi::c_void);
    unsafe {
        SetWindowPos(
            hwnd,
            if topmost {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        )?;
    }
    Ok(())
}

fn cached_process_name(process_id: u32) -> String {
    static LAST_PROCESS: OnceLock<Mutex<Option<(u32, String)>>> = OnceLock::new();
    let cache = LAST_PROCESS.get_or_init(|| Mutex::new(None));
    let Ok(mut cache) = cache.lock() else {
        return process_name(process_id).unwrap_or_default();
    };
    if let Some((cached_id, name)) = cache.as_ref() {
        if *cached_id == process_id {
            return name.clone();
        }
    }

    let name = process_name(process_id).unwrap_or_default();
    *cache = Some((process_id, name.clone()));
    name
}

pub fn set_window_rect_topmost(handle: WindowHandle, rect: AppRect, topmost: bool) -> Result<()> {
    let hwnd = HWND(handle.0 as *mut core::ffi::c_void);
    let width = rect.width().max(1);
    let height = rect.height().max(1);
    let insert_after = if topmost {
        let taskbars = taskbar_window_rects();
        select_taskbar_anchor(rect, &taskbars)
            .map(|handle| HWND(handle.0 as *mut core::ffi::c_void))
            .unwrap_or(HWND_TOPMOST)
    } else {
        HWND_NOTOPMOST
    };
    unsafe {
        SetWindowPos(
            hwnd,
            insert_after,
            rect.left,
            rect.top,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        )?;
    }

    Ok(())
}

fn window_rect(hwnd: HWND) -> Result<AppRect> {
    let mut rect = RECT::default();
    unsafe {
        GetWindowRect(hwnd, &mut rect)?;
    }

    Ok(AppRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

fn select_taskbar_anchor(
    target: AppRect,
    taskbars: &[(WindowHandle, AppRect)],
) -> Option<WindowHandle> {
    taskbars
        .iter()
        .filter_map(|(handle, rect)| {
            let width = (target.right.min(rect.right) - target.left.max(rect.left)).max(0) as i64;
            let height = (target.bottom.min(rect.bottom) - target.top.max(rect.top)).max(0) as i64;
            let area = width * height;
            (area > 0).then_some((*handle, area))
        })
        .max_by_key(|(_, area)| *area)
        .map(|(handle, _)| handle)
}

fn taskbar_window_rects() -> Vec<(WindowHandle, AppRect)> {
    let mut taskbars = Vec::new();

    unsafe {
        if let Ok(hwnd) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
            push_taskbar_rect(&mut taskbars, hwnd);
        }

        let mut previous = HWND::default();
        while let Ok(hwnd) = FindWindowExW(
            HWND::default(),
            previous,
            w!("Shell_SecondaryTrayWnd"),
            PCWSTR::null(),
        ) {
            push_taskbar_rect(&mut taskbars, hwnd);
            previous = hwnd;
        }
    }

    taskbars
}

fn push_taskbar_rect(taskbars: &mut Vec<(WindowHandle, AppRect)>, hwnd: HWND) {
    if !unsafe { IsWindowVisible(hwnd).as_bool() } {
        return;
    }
    if let Ok(rect) = window_rect(hwnd) {
        taskbars.push((WindowHandle(hwnd.0 as isize), rect));
    }
}

fn window_text(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

fn process_name(process_id: u32) -> Option<String> {
    if process_id == 0 {
        return None;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()? };
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    result.ok()?;

    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .or(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP,
    };

    #[test]
    fn bottom_hidden_window_is_anchored_behind_overlapping_taskbar() {
        let target = AppRect {
            left: 300,
            top: 1028,
            right: 1100,
            bottom: 1604,
        };
        let taskbars = [(
            WindowHandle(42),
            AppRect {
                left: 0,
                top: 1040,
                right: 1920,
                bottom: 1080,
            },
        )];

        assert_eq!(
            select_taskbar_anchor(target, &taskbars),
            Some(WindowHandle(42))
        );
    }

    #[test]
    fn window_without_taskbar_overlap_keeps_regular_topmost_order() {
        let target = AppRect {
            left: 300,
            top: 100,
            right: 1100,
            bottom: 700,
        };
        let taskbars = [(
            WindowHandle(42),
            AppRect {
                left: 0,
                top: 1040,
                right: 1920,
                bottom: 1080,
            },
        )];

        assert_eq!(select_taskbar_anchor(target, &taskbars), None);
    }

    #[test]
    fn explicit_topmost_change_does_not_move_or_resize_the_window() {
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("ConvenientWindowTopmostTest"),
                WS_POPUP,
                20,
                30,
                160,
                90,
                HWND::default(),
                None,
                None,
                None,
            )
            .unwrap()
        };
        let handle = WindowHandle(hwnd.0 as isize);

        set_window_topmost(handle, true).unwrap();
        assert_ne!(
            unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0,
            0
        );
        assert_eq!(
            window_rect(hwnd).unwrap(),
            AppRect {
                left: 20,
                top: 30,
                right: 180,
                bottom: 120
            }
        );

        set_window_topmost(handle, false).unwrap();
        assert_eq!(
            unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0,
            0
        );
        let _ = unsafe { DestroyWindow(hwnd) };
    }
}
