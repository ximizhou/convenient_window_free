use anyhow::{bail, Result};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GetAncestor, GetClassNameW, GetForegroundWindow,
    GetGUIThreadInfo, GetWindow, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowArranged,
    IsWindowVisible, IsZoomed, SetWindowPlacement, SetWindowPos, ShowWindow, WindowFromPoint,
    GA_ROOT, GUITHREADINFO, GUI_INMOVESIZE, GWL_EXSTYLE, GW_OWNER, HWND_NOTOPMOST, HWND_TOPMOST,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SW_MAXIMIZE,
    SW_MINIMIZE, SW_RESTORE, SW_SHOWMAXIMIZED, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE,
    WINDOWPLACEMENT, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
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
    // Screenshot and other capture surfaces often use WS_EX_TOOLWINDOW. They
    // are still useful foreground signals for edge-hide, but must not become
    // draggable/candidate windows.
    window_info(hwnd, true)
}

pub fn window_exists(handle: WindowHandle) -> bool {
    unsafe { IsWindow(HWND(handle.0 as *mut core::ffi::c_void)).as_bool() }
}

pub fn window_is_minimized(handle: WindowHandle) -> bool {
    unsafe { IsIconic(HWND(handle.0 as *mut core::ffi::c_void)).as_bool() }
}

fn window_info(hwnd: HWND, include_tool_window: bool) -> Result<Option<WindowInfo>> {
    if hwnd.0.is_null() || !unsafe { IsWindowVisible(hwnd).as_bool() } {
        return Ok(None);
    }

    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let tool_window = (ex_style & WS_EX_TOOLWINDOW.0) != 0;
    if tool_window && !include_tool_window {
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
        transient: tool_window || has_owner || class_name == "#32768",
        class_name,
        process_name: cached_process_name(process_id),
        maximized: unsafe { IsZoomed(hwnd).as_bool() },
        arranged: unsafe { IsWindowArranged(hwnd).as_bool() },
        topmost: (ex_style & WS_EX_TOPMOST.0) != 0,
    }))
}

pub fn window_info_for_handle(handle: WindowHandle) -> Result<Option<WindowInfo>> {
    window_info(HWND(handle.0 as *mut core::ffi::c_void), false)
}

/// Enumerate ordinary visible top-level windows in z-order. Shell surfaces,
/// owned/tool windows and transient menus are intentionally excluded because
/// they cannot be restored as independent user windows.
pub fn list_window_layouts() -> Result<Vec<crate::platform::WindowLayoutTarget>> {
    let mut windows = Vec::<crate::platform::WindowLayoutTarget>::new();
    let parameter =
        LPARAM((&mut windows as *mut Vec<crate::platform::WindowLayoutTarget>) as isize);
    unsafe {
        EnumWindows(Some(enum_layout_target), parameter)?;
    }
    Ok(windows)
}

unsafe extern "system" fn enum_layout_target(hwnd: HWND, data: LPARAM) -> BOOL {
    let windows = &mut *(data.0 as *mut Vec<crate::platform::WindowLayoutTarget>);
    let Some(window) = window_info(hwnd, false).ok().flatten() else {
        return true.into();
    };
    if !is_layout_candidate(&window) {
        return true.into();
    }
    let minimized = unsafe { IsIconic(hwnd).as_bool() };
    // A snapped ("arranged") window keeps its pre-snap `rcNormalPosition`, so a
    // placement-based restore would revert the Snap size. Keep only the visible
    // `GetWindowRect` rectangle for those snapshots and let the restore fallback
    // reproduce it exactly.
    let placement_rect = if capture_normal_placement(minimized, window.arranged) {
        match normal_window_rect(hwnd) {
            Ok(rect) => Some(rect),
            Err(_) if minimized => return true.into(),
            Err(_) => None,
        }
    } else {
        None
    };
    // EnumWindows yields windows in z-order (topmost first), and only candidates
    // reach this point, so the length before push is the rank we publish.
    let z_index = windows.len() as u32;
    windows.push(crate::platform::WindowLayoutTarget {
        handle: window.handle,
        z_index,
        rect: if minimized {
            placement_rect.unwrap_or(window.rect)
        } else {
            window.rect
        },
        placement_rect,
        title: window.title,
        class_name: window.class_name,
        process_name: window.process_name,
        minimized,
        maximized: window.maximized,
        transient: window.transient,
        topmost: window.topmost,
    });
    true.into()
}

/// Whether a snapshot should keep `GetWindowPlacement().rcNormalPosition`.
///
/// For a visible snapped window that rectangle describes the geometry from
/// *before* the snap; restoring through `SetWindowPlacement` therefore undoes
/// the Snap resize. Minimized windows are excluded from the rule because their
/// visible rectangle is the off-screen minimized position, while the normal
/// placement rectangle is still the only reliable target geometry.
fn capture_normal_placement(minimized: bool, arranged: bool) -> bool {
    !(arranged && !minimized)
}

fn is_layout_candidate(window: &WindowInfo) -> bool {
    !window.transient
        && !matches!(
            window.class_name.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "#32768"
        )
}

pub fn apply_window_layout(
    handle: WindowHandle,
    layout: &crate::platform::WindowLayout,
) -> Result<()> {
    let hwnd = HWND(handle.0 as *mut core::ffi::c_void);
    let Some(window) = window_info(hwnd, false)? else {
        bail!("window layout target is no longer available")
    };
    if !is_layout_candidate(&window) {
        bail!("window layout target is not an ordinary window")
    }

    if layout_identity(&window.process_name, &window.class_name, &window.title)
        != layout_identity(&layout.process_name, &layout.class_name, &layout.title)
    {
        bail!("window layout target identity changed")
    }

    if let Some(rect) = layout.placement_rect {
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        unsafe {
            GetWindowPlacement(hwnd, &mut placement)?;
        }
        placement.rcNormalPosition = RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        placement.showCmd = if layout.minimized {
            SW_SHOWMINNOACTIVE.0 as u32
        } else if layout.maximized {
            SW_SHOWMAXIMIZED.0 as u32
        } else {
            SW_SHOWNOACTIVATE.0 as u32
        };
        unsafe {
            SetWindowPlacement(hwnd, &placement)?;
        }
        set_window_topmost(handle, layout.topmost)?;
        return Ok(());
    }

    // Fallback for snapshots without a usable native placement: legacy layouts
    // and visible Aero Snap captures, whose `rcNormalPosition` predates the snap.
    // `SW_RESTORE` must run first so a minimized/maximized target can be placed
    // by `set_window_rect` before its final show state is re-applied below.
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }
    set_window_rect(handle, layout.rect)?;
    if layout.maximized {
        unsafe {
            let _ = ShowWindow(hwnd, SW_MAXIMIZE);
        }
    } else if layout.minimized {
        unsafe {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        }
    }
    set_window_topmost(handle, layout.topmost)?;
    Ok(())
}

fn normal_window_rect(hwnd: HWND) -> Result<AppRect> {
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    unsafe {
        GetWindowPlacement(hwnd, &mut placement)?;
    }
    let rect = placement.rcNormalPosition;
    Ok(AppRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

fn layout_identity(process: &str, class_name: &str, title: &str) -> (String, String, String) {
    (
        process.to_lowercase(),
        class_name.to_lowercase(),
        title.to_string(),
    )
}

/// Preserve the keyboard meaning of the existing presets, but use the click's
/// original target instead of whichever window receives foreground afterwards.
pub fn window_shortcut_at(shortcut: &str, target: WindowHandle) -> Result<bool> {
    let command = match shortcut.to_ascii_lowercase().replace(' ', "").as_str() {
        "win+up" => SW_MAXIMIZE,
        "win+down" => SW_MINIMIZE,
        _ => return Ok(false),
    };
    let hwnd = HWND(target.0 as *mut core::ffi::c_void);
    let Some(window) = window_info(hwnd, false)? else {
        bail!("window action target is no longer available");
    };
    if window.transient
        || matches!(
            window.class_name.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "#32768"
        )
    {
        bail!("window action target is not an ordinary window");
    }
    let command = if command == SW_MINIMIZE && window.maximized {
        SW_RESTORE
    } else {
        command
    };
    unsafe {
        let _ = ShowWindow(hwnd, command);
    }
    let applied = unsafe {
        if command == SW_MAXIMIZE {
            IsZoomed(hwnd).as_bool()
        } else if command == SW_MINIMIZE {
            IsIconic(hwnd).as_bool()
        } else {
            !IsZoomed(hwnd).as_bool() && !IsIconic(hwnd).as_bool()
        }
    };
    if !applied {
        bail!("window state change did not apply");
    }
    Ok(true)
}

/// 找出鼠标下可用于拖拽的窗口。
///
/// 应用名单必须在这里、且在 `SW_RESTORE` **之前**判定：命中名单时要让目标程序自己处理
/// `Alt + 鼠标`，如果先把最大化的窗口摘下来再发现应该放手，窗口状态已经被改掉，
/// 用户看到的就是「名单似乎没生效，窗口还是从全屏缩回了小窗」。
/// 名单为空时不做额外查询，保持默认路径的行为与开销不变。
pub fn draggable_window_at(point: Point, paused_apps: &[String]) -> Result<Option<WindowInfo>> {
    let Some((hwnd, mut window)) = window_at_point_for_drag(point)? else {
        return Ok(None);
    };
    if !is_drag_candidate(hwnd, &window) || is_paused_window(paused_apps, &window) {
        return Ok(None);
    }
    if window.maximized {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        window = match window_info(hwnd, false)? {
            Some(window) => window,
            None => return Ok(None),
        };
    }
    Ok(Some(window))
}

/// Read-only admission check used by the low-level mouse hook. It must not
/// restore maximized windows or otherwise mutate the target before the engine
/// has accepted the capture.
pub fn window_drag_target_allowed_at(point: Point, paused_apps: &[String]) -> Result<bool> {
    let Some((hwnd, window)) = window_at_point_for_drag(point)? else {
        return Ok(false);
    };
    Ok(is_drag_candidate(hwnd, &window) && !is_paused_window(paused_apps, &window))
}

fn window_at_point_for_drag(point: Point) -> Result<Option<(HWND, WindowInfo)>> {
    let hwnd = unsafe {
        GetAncestor(
            WindowFromPoint(POINT {
                x: point.x,
                y: point.y,
            }),
            GA_ROOT,
        )
    };
    Ok(window_info(hwnd, false)?.map(|window| (hwnd, window)))
}

fn is_drag_candidate(hwnd: HWND, window: &WindowInfo) -> bool {
    !window.transient
        && !matches!(
            window.class_name.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "#32768"
        )
        && (window.maximized || !window_covers_monitor(hwnd, window.rect))
}

/// 与 `core::engine` 的应用名单语义保持一致：按进程名、窗口标题或窗口类名做小写包含匹配，
/// 命中任一项即视为该窗口属于名单内的应用。
pub fn is_paused_window(paused_apps: &[String], window: &WindowInfo) -> bool {
    if paused_apps.is_empty() {
        return false;
    }
    let process = window.process_name.to_lowercase();
    let title = window.title.to_lowercase();
    let class_name = window.class_name.to_lowercase();
    paused_apps.iter().any(|item| {
        let item = item.trim().to_lowercase();
        !item.is_empty()
            && (process.contains(&item) || title.contains(&item) || class_name.contains(&item))
    })
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

pub fn native_window_move_size_active(handle: WindowHandle) -> bool {
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    let hwnd = HWND(handle.0 as *mut core::ffi::c_void);
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, None) };
    thread_id != 0
        && unsafe { GetGUIThreadInfo(thread_id, &mut info).is_ok() }
        && info.flags.contains(GUI_INMOVESIZE)
        && info.hwndMoveSize == hwnd
}

pub fn prepare_top_edge_drop(handle: WindowHandle) -> Result<()> {
    let hwnd = HWND(handle.0 as *mut core::ffi::c_void);
    if !unsafe { IsWindow(hwnd).as_bool() } {
        bail!("Window no longer exists");
    }
    if unsafe { IsZoomed(hwnd).as_bool() || IsWindowArranged(hwnd).as_bool() } {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
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
    let Some(window) = window_info(hwnd, false)? else {
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
        CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP, WS_VISIBLE,
    };

    use super::super::native_test_windows::{IsolatedDesktop, TestWindow};

    #[test]
    fn isolated_non_topmost_restore_does_not_inherit_topmost_from_neighbours() {
        let _desktop = IsolatedDesktop::new();
        let first = TestWindow::new("Native topmost restore first");
        let second = TestWindow::new("Native topmost restore second");
        let pinned = TestWindow::new("Native topmost restore pinned neighbour");
        set_window_topmost(pinned.handle(), true).unwrap();

        for iteration in 0..32 {
            for window in [&first, &second] {
                let rect = AppRect {
                    left: 100 + iteration,
                    top: 100,
                    right: 340 + iteration,
                    bottom: 260,
                };
                set_window_rect_topmost(window.handle(), rect, true).unwrap();
                assert!(window.topmost());
                if iteration == 0 {
                    let captured = list_window_layouts()
                        .unwrap()
                        .into_iter()
                        .find(|candidate| candidate.handle == window.handle())
                        .unwrap();
                    assert!(
                        captured.topmost,
                        "native capture reports the current temporary style; the engine must restore user intent"
                    );
                }
                set_window_rect_topmost(window.handle(), rect, false).unwrap();
                assert!(!window.topmost(), "explicit false must demote its target");
                assert!(pinned.topmost(), "another pinned window must stay pinned");
            }
            assert!(!first.topmost());
            assert!(!second.topmost());
        }

        for placement in [true, false] {
            for order in [[&first, &second], [&second, &first]] {
                for window in order {
                    set_window_topmost(window.handle(), true).unwrap();
                    let info = window_info_for_handle(window.handle()).unwrap().unwrap();
                    let layout = crate::platform::WindowLayout {
                        process_name: info.process_name,
                        class_name: info.class_name,
                        title: info.title,
                        occurrence: 0,
                        z_index: 0,
                        rect: info.rect,
                        placement_rect: placement.then_some(info.rect),
                        minimized: false,
                        maximized: false,
                        topmost: false,
                        edge_hidden: None,
                    };
                    apply_window_layout(window.handle(), &layout).unwrap();
                    assert!(
                        !window.topmost(),
                        "both layout paths must demote a false snapshot"
                    );
                    assert!(pinned.topmost());
                }
            }
        }
    }

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
    fn minimized_layout_round_trip_keeps_normal_placement_and_show_state() {
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("ConvenientWindowMinimizedLayoutTest"),
                WS_POPUP | WS_VISIBLE,
                46,
                58,
                180,
                120,
                HWND::default(),
                None,
                None,
                None,
            )
            .unwrap()
        };
        let handle = WindowHandle(hwnd.0 as isize);
        let before = normal_window_rect(hwnd).unwrap();
        unsafe {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        }
        assert!(window_is_minimized(handle));

        let targets = list_window_layouts().unwrap();
        let target = targets
            .iter()
            .find(|window| window.handle == handle)
            .expect("a visible minimized ordinary window is a layout target");
        assert!(target.minimized);
        assert_eq!(target.placement_rect, Some(before));
        // 枚举快照里的顺序即 z-order：0 = 最顶层，push 前长度就是名次。
        assert_eq!(
            target.z_index,
            targets
                .iter()
                .position(|window| window.handle == handle)
                .unwrap() as u32
        );

        let mut layout = crate::platform::WindowLayout {
            process_name: target.process_name.clone(),
            class_name: target.class_name.clone(),
            title: target.title.clone(),
            occurrence: 0,
            z_index: target.z_index,
            rect: target.rect,
            placement_rect: target.placement_rect,
            minimized: true,
            maximized: false,
            topmost: false,
            edge_hidden: None,
        };
        apply_window_layout(handle, &layout).unwrap();
        assert!(window_is_minimized(handle));
        assert_eq!(normal_window_rect(hwnd).unwrap(), before);

        layout.minimized = false;
        apply_window_layout(handle, &layout).unwrap();
        assert!(!window_is_minimized(handle));
        assert_eq!(normal_window_rect(hwnd).unwrap(), before);
        let _ = unsafe { DestroyWindow(hwnd) };
    }

    /// 回归：Aero Snap 窗口的 `rcNormalPosition` 是 Snap 之前的正常矩形，快照若携带它，
    /// 恢复时 `SetWindowPlacement` 会把 Snap 尺寸撤销成 Snap 前的大小。
    #[test]
    fn snap_capture_drops_normal_placement_but_keeps_it_for_minimized_windows() {
        // 可见 Snap 窗口：只保留 GetWindowRect 的可见矩形，恢复走 fallback。
        assert!(!capture_normal_placement(false, true));
        // 普通与最大化窗口维持原有 placement 捕获。
        assert!(capture_normal_placement(false, false));
        // 最小化窗口的可见矩形是屏幕外的图标位置，必须保留正常矩形。
        assert!(capture_normal_placement(true, false));
        // Windows 文档保证 arranged/minimized 互斥；这里钉住「最小化优先」的稳健语义。
        assert!(capture_normal_placement(true, true));
    }

    /// Snap 快照没有 placement，恢复必须按可见矩形精确落位 —— 这正是修复后的路径。
    /// 同时核对 minimized 快照的 fallback 顺序：先 SW_RESTORE、再定位、最后重新最小化。
    #[test]
    fn snap_layout_fallback_restores_visible_rect_then_reapplies_minimized_state() {
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("ConvenientWindowSnapLayoutTest"),
                WS_POPUP | WS_VISIBLE,
                40,
                50,
                180,
                120,
                HWND::default(),
                None,
                None,
                None,
            )
            .unwrap()
        };
        let handle = WindowHandle(hwnd.0 as isize);
        let target = list_window_layouts()
            .unwrap()
            .into_iter()
            .find(|window| window.handle == handle)
            .expect("a visible ordinary window is a layout target");
        // “左半屏”可见矩形，模拟 Windows Snap 后的快照形状：rect 可见、placement 被丢弃。
        let snap_rect = AppRect {
            left: 0,
            top: 0,
            right: 960,
            bottom: 1040,
        };
        let mut layout = crate::platform::WindowLayout {
            process_name: target.process_name,
            class_name: target.class_name,
            title: target.title,
            occurrence: 0,
            z_index: target.z_index,
            rect: snap_rect,
            placement_rect: None,
            minimized: false,
            maximized: false,
            topmost: false,
            edge_hidden: None,
        };

        apply_window_layout(handle, &layout).unwrap();
        assert!(!window_is_minimized(handle));
        assert_eq!(window_rect(hwnd).unwrap(), snap_rect);

        layout.minimized = true;
        apply_window_layout(handle, &layout).unwrap();
        assert!(window_is_minimized(handle));

        layout.minimized = false;
        apply_window_layout(handle, &layout).unwrap();
        assert!(!window_is_minimized(handle));
        assert_eq!(window_rect(hwnd).unwrap(), snap_rect);
        let _ = unsafe { DestroyWindow(hwnd) };
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

    fn named_window(process: &str, title: &str, class_name: &str) -> WindowInfo {
        WindowInfo {
            handle: WindowHandle(1),
            rect: AppRect {
                left: 0,
                top: 0,
                right: 800,
                bottom: 600,
            },
            title: title.into(),
            class_name: class_name.into(),
            process_name: process.into(),
            maximized: false,
            transient: false,
            arranged: false,
            topmost: false,
        }
    }

    /// 名单匹配必须发生在 `draggable_window_at` 摘除最大化状态**之前**。
    ///
    /// 回归背景：名单检查原先放在引擎侧、`SW_RESTORE` 之后，于是即使命中名单，
    /// 最大化窗口也已经被摘成浮动小窗 —— 用户看到的是「加了名单窗口还是从全屏缩回了小窗」，
    /// 而关闭拖拽功能（那条路径在摘除前就返回）却一切正常。修法是把判定前移进本函数。
    /// 这条测试保证判定函数本身正确；顺序约束同时写在上面的函数文档里。
    #[test]
    fn paused_apps_match_process_title_or_class_and_keep_the_rest_untouched() {
        let by_process = vec!["photoshop.exe".to_string()];
        assert!(is_paused_window(
            &by_process,
            &named_window("Photoshop.exe", "未命名-1", "Photoshop")
        ));
        assert!(!is_paused_window(
            &by_process,
            &named_window("notepad.exe", "无标题", "Notepad")
        ));

        let by_title = vec!["illustrator".to_string()];
        assert!(is_paused_window(
            &by_title,
            &named_window("unknown.exe", "Adobe Illustrator 2024", "SomeClass")
        ));

        let by_class = vec!["blender".to_string()];
        assert!(is_paused_window(
            &by_class,
            &named_window("unknown.exe", "无标题", "BlenderWindow")
        ));

        // 前后空白要被忽略；空项不得命中任何窗口。
        let padded = vec!["  sketchup.exe  ".to_string()];
        assert!(is_paused_window(
            &padded,
            &named_window("SketchUp.exe", "模型", "SketchUp")
        ));
        let blank = vec!["   ".to_string(), String::new()];
        assert!(!is_paused_window(
            &blank,
            &named_window("SketchUp.exe", "模型", "SketchUp")
        ));

        // 空名单必须完全不命中，保证默认路径零影响。
        assert!(!is_paused_window(
            &[],
            &named_window("photoshop.exe", "文档", "Photoshop")
        ));
    }
}
