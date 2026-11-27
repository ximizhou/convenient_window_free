use crate::platform::WindowHandle;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::{BOOL, COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, CreateSolidBrush, DeleteObject, Ellipse, EndPaint, FillRect,
    InvalidateRect, LineTo, MoveToEx, ScreenToClient, SelectObject, HBRUSH, PAINTSTRUCT, PS_SOLID,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows, GetClassNameW,
    GetCursorPos, GetWindow, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsIconic,
    IsWindow, IsWindowVisible, LoadCursorW, PeekMessageW, RegisterClassW, SetCursor,
    SetLayeredWindowAttributes, SetWindowDisplayAffinity, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA,
    GWL_EXSTYLE, GW_HWNDPREV, GW_OWNER, HWND_NOTOPMOST, HWND_TOPMOST, IDC_HAND, LWA_ALPHA,
    LWA_COLORKEY, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
    SWP_SHOWWINDOW, SW_HIDE, WDA_EXCLUDEFROMCAPTURE, WINDOW_EX_STYLE, WM_DESTROY, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_PAINT, WM_SETCURSOR, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TOPMOST as TOPMOST_STYLE, WS_POPUP,
};

const CLASS_NAME: windows::core::PCWSTR = w!("ConvenientWindowTopmostPin");
const PIN_HIT_DIP: i32 = 22;
const PIN_VISUAL_DIP: i32 = 12;
const PIN_MARGIN_DIP: i32 = 8;
const PIN_TRANSPARENT: COLORREF = COLORREF(0x00000000);
const PIN_NORMAL: COLORREF = COLORREF(0x004D48E5);
const PIN_HOVER: COLORREF = COLORREF(0x006F6BFF);
const PIN_PRESSED: COLORREF = COLORREF(0x003A34C9);
const PIN_FOLLOW_INTERVAL: Duration = Duration::from_millis(8);
const PIN_IDLE_INTERVAL: Duration = Duration::from_millis(80);
const WM_MOUSELEAVE_MESSAGE: u32 = 0x02A3;
static PIN_COMMANDS: OnceLock<mpsc::Sender<PinCommand>> = OnceLock::new();
static PIN_ENABLED: AtomicBool = AtomicBool::new(true);

struct ManagedPin {
    marker: isize,
    rect: Option<RECT>,
    shown: bool,
}

impl ManagedPin {
    fn new(marker: isize) -> Self {
        Self {
            marker,
            rect: None,
            shown: false,
        }
    }
}

enum PinCommand {
    SetTarget(WindowHandle, bool),
    Configure(bool),
    Clear,
}

pub fn configure_topmost_pins(enabled: bool) {
    let changed = PIN_ENABLED.swap(enabled, Ordering::AcqRel) != enabled;
    // 首次配置时就创建图钉线程：helper 启动后要能接管已经在置顶状态的窗口，
    // 否则重启前遗留的置顶窗口就永远没有图钉可点。
    let sender = pin_commands();
    if changed {
        let _ = sender.send(PinCommand::Configure(enabled));
    }
}

pub fn set_topmost_pin_target(handle: WindowHandle, topmost: bool) {
    let _ = pin_commands().send(PinCommand::SetTarget(handle, topmost));
}

pub fn clear_topmost_pins() {
    let _ = pin_commands().send(PinCommand::Clear);
}

fn pin_commands() -> &'static mpsc::Sender<PinCommand> {
    PIN_COMMANDS.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("topmost-pin-ui".into())
            .spawn(move || run_pin_thread(receiver))
            .expect("start topmost pin UI thread");
        sender
    })
}

fn run_pin_thread(receiver: mpsc::Receiver<PinCommand>) {
    register_pin_class();
    let mut enabled = PIN_ENABLED.load(Ordering::Acquire);
    let mut pins: HashMap<WindowHandle, ManagedPin> = HashMap::new();
    if enabled {
        adopt_existing_topmost_windows(&mut pins);
    }
    let mut last_activity = Instant::now();
    loop {
        let has_visible_pin = pins.values().any(|pin| pin.shown);
        let timeout = pin_refresh_interval(has_visible_pin, last_activity.elapsed());
        let command = receiver.recv_timeout(timeout);
        if command.is_ok() {
            last_activity = Instant::now();
        }
        match command {
            Ok(PinCommand::SetTarget(handle, topmost)) => {
                if topmost {
                    let pin = pins.entry(handle).or_insert_with(|| ManagedPin::new(0));
                    if enabled {
                        ensure_pin_window(handle, pin);
                    }
                } else {
                    remove_pin(&mut pins, handle);
                }
            }
            Ok(PinCommand::Configure(next)) => {
                enabled = next;
                if enabled {
                    adopt_existing_topmost_windows(&mut pins);
                    for (&target, pin) in pins.iter_mut() {
                        ensure_pin_window(target, pin);
                    }
                } else {
                    hide_pins(&mut pins);
                }
            }
            Ok(PinCommand::Clear) => clear_pins(&mut pins),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        pump_messages();
        if enabled && refresh_pins(&mut pins) {
            last_activity = Instant::now();
        }
    }
    clear_pins(&mut pins);
}

/// 接管本次会话开始前就已经置顶的普通窗口。
///
/// 置顶是窗口自身的 `WS_EX_TOPMOST` 属性：helper 退出或重启后图钉窗口消失，但目标窗口
/// 仍保持置顶，用户就失去了「点图钉取消置顶」的入口，只能再触发一次切换动作。这里在
/// 图钉线程启动（或功能重新启用）时枚举当前置顶的普通窗口，为它们补上图钉，让置顶状态
/// 重新可见、可取消。图钉只负责指示与取消，不会改变这些窗口的置顶状态。
fn adopt_existing_topmost_windows(pins: &mut HashMap<WindowHandle, ManagedPin>) {
    let exclude_process = unsafe { GetCurrentProcessId() };
    for handle in collect_adoptable_topmost(exclude_process) {
        if pins.contains_key(&handle) {
            continue;
        }
        let mut pin = ManagedPin::new(0);
        ensure_pin_window(handle, &mut pin);
        pins.insert(handle, pin);
    }
}

struct AdoptContext {
    exclude_process: u32,
    windows: Vec<WindowHandle>,
}

fn collect_adoptable_topmost(exclude_process: u32) -> Vec<WindowHandle> {
    let mut context = AdoptContext {
        exclude_process,
        windows: Vec::new(),
    };
    let parameter = LPARAM((&mut context as *mut AdoptContext) as isize);
    unsafe {
        let _ = EnumWindows(Some(enum_adoptable_topmost), parameter);
    }
    context.windows
}

unsafe extern "system" fn enum_adoptable_topmost(hwnd: HWND, data: LPARAM) -> BOOL {
    let context = &mut *(data.0 as *mut AdoptContext);
    if is_adoptable_topmost(hwnd, context.exclude_process) {
        context.windows.push(WindowHandle(hwnd.0 as isize));
    }
    true.into()
}

/// 只接管用户能看见、能操作的普通置顶窗口：排除 helper 自己的浮层（按进程）、
/// tool window、带 owner 的弹窗、Shell 表面和过小的窗口。
fn is_adoptable_topmost(hwnd: HWND, exclude_process: u32) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex_style & TOPMOST_STYLE.0 == 0 || ex_style & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }
        if exclude_process != 0 {
            let mut process_id = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut process_id));
            if process_id == exclude_process {
                return false;
            }
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null()) {
            return false;
        }
        if is_shell_surface(hwnd) {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }
        rect.right - rect.left >= 120 && rect.bottom - rect.top >= 80
    }
}

fn is_shell_surface(hwnd: HWND) -> bool {
    let mut buffer = [0u16; 128];
    let length = (unsafe { GetClassNameW(hwnd, &mut buffer) }.max(0) as usize).min(buffer.len());
    let class_name = String::from_utf16_lossy(&buffer[..length]);
    matches!(
        class_name.as_str(),
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "#32768"
    )
}

fn pin_refresh_interval(has_visible_pin: bool, stationary: Duration) -> Duration {
    if !has_visible_pin {
        return PIN_IDLE_INTERVAL;
    }
    match stationary.as_millis() {
        0..250 => PIN_FOLLOW_INTERVAL,
        250..500 => Duration::from_millis(16),
        500..750 => Duration::from_millis(32),
        _ => Duration::from_millis(48),
    }
}

fn register_pin_class() {
    unsafe {
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(pin_window_proc),
            hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        let _ = RegisterClassW(&class);
    }
}

fn create_pin_window(target: WindowHandle) -> isize {
    register_pin_class();
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(
                WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0 | WS_EX_NOACTIVATE.0 | WS_EX_LAYERED.0,
            ),
            CLASS_NAME,
            None,
            WS_POPUP,
            0,
            0,
            PIN_HIT_DIP,
            PIN_HIT_DIP,
            HWND::default(),
            None,
            None,
            Some(target.0 as *const c_void),
        )
        .unwrap_or_default()
    };
    if !hwnd.0.is_null() {
        unsafe {
            let _ =
                SetLayeredWindowAttributes(hwnd, PIN_TRANSPARENT, 255, LWA_ALPHA | LWA_COLORKEY);
            let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        }
    }
    hwnd.0 as isize
}

fn refresh_pins(pins: &mut HashMap<WindowHandle, ManagedPin>) -> bool {
    let mut changed = false;
    let mut stale = Vec::new();
    for (&target, pin_state) in pins.iter_mut() {
        let target_hwnd = HWND(target.0 as *mut c_void);
        let valid = unsafe { IsWindow(target_hwnd).as_bool() };
        let topmost = valid
            && (unsafe { GetWindowLongPtrW(target_hwnd, GWL_EXSTYLE) } as u32 & TOPMOST_STYLE.0)
                != 0;
        if !valid || !topmost {
            stale.push(target);
            continue;
        }
        if !unsafe { IsWindowVisible(target_hwnd).as_bool() }
            || unsafe { IsIconic(target_hwnd).as_bool() }
        {
            if pin_state.shown && pin_state.marker != 0 {
                unsafe {
                    let _ = ShowWindow(HWND(pin_state.marker as *mut c_void), SW_HIDE);
                }
                pin_state.shown = false;
                pin_state.rect = None;
                changed = true;
            }
            continue;
        }
        if pin_state.marker == 0 {
            ensure_pin_window(target, pin_state);
        }
        if pin_state.marker == 0 {
            continue;
        }
        let marker_hwnd = HWND(pin_state.marker as *mut c_void);
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(target_hwnd, &mut rect) }.is_err() {
            stale.push(target);
            continue;
        }
        let dpi = unsafe { GetDpiForWindow(target_hwnd) }.max(96);
        let pin = pin_rect(rect, dpi);
        // Activation can raise a stationary target above its independent pin window.
        // Keep the pin directly above its target, below windows that cover the target.
        let insert_after = unsafe { GetWindow(target_hwnd, GW_HWNDPREV) }.unwrap_or(HWND_TOPMOST);
        let z_order_changed = insert_after != marker_hwnd;
        if pin_state.rect != Some(pin) || z_order_changed || !pin_state.shown {
            changed = true;
            let mut flags = SWP_NOACTIVATE | SWP_NOOWNERZORDER;
            if !pin_state.shown {
                flags |= SWP_SHOWWINDOW;
            }
            if pin_state.rect.is_some_and(|previous| {
                previous.right - previous.left == pin.right - pin.left
                    && previous.bottom - previous.top == pin.bottom - pin.top
            }) {
                flags |= SWP_NOSIZE;
            }
            if !z_order_changed {
                flags |= SWP_NOZORDER;
            }
            let moved = unsafe {
                SetWindowPos(
                    marker_hwnd,
                    insert_after,
                    pin.left,
                    pin.top,
                    pin.right - pin.left,
                    pin.bottom - pin.top,
                    flags,
                )
            };
            if moved.is_err() {
                continue;
            }
            pin_state.rect = Some(pin);
            pin_state.shown = true;
        }
    }
    changed |= !stale.is_empty();
    for handle in stale {
        remove_pin(pins, handle);
    }
    changed
}

fn ensure_pin_window(target: WindowHandle, pin_state: &mut ManagedPin) {
    if pin_state.marker == 0 {
        pin_state.marker = create_pin_window(target);
    }
}

fn hide_pins(pins: &mut HashMap<WindowHandle, ManagedPin>) {
    for pin in pins.values_mut() {
        if pin.marker != 0 && pin.shown {
            unsafe {
                let _ = ShowWindow(HWND(pin.marker as *mut c_void), SW_HIDE);
            }
        }
        pin.shown = false;
        pin.rect = None;
    }
}

fn pin_rect(target: RECT, dpi: u32) -> RECT {
    let size = scale_dip(PIN_HIT_DIP, dpi).clamp(18, 88);
    let margin = scale_dip(PIN_MARGIN_DIP, dpi).clamp(6, 32);
    RECT {
        left: target.left + margin,
        top: target.top + margin,
        right: target.left + margin + size,
        bottom: target.top + margin + size,
    }
}

fn scale_dip(value: i32, dpi: u32) -> i32 {
    ((i64::from(value) * i64::from(dpi.max(96)) + 95) / 96) as i32
}

fn pin_color(hovered: bool, pressed: bool) -> COLORREF {
    if pressed {
        PIN_PRESSED
    } else if hovered {
        PIN_HOVER
    } else {
        PIN_NORMAL
    }
}

fn remove_pin(pins: &mut HashMap<WindowHandle, ManagedPin>, target: WindowHandle) {
    if let Some(pin) = pins.remove(&target) {
        if pin.marker != 0 {
            unsafe {
                let _ = DestroyWindow(HWND(pin.marker as *mut c_void));
            }
        }
    }
}

fn clear_pins(pins: &mut HashMap<WindowHandle, ManagedPin>) {
    for (_, pin) in pins.drain() {
        if pin.marker != 0 {
            unsafe {
                let _ = DestroyWindow(HWND(pin.marker as *mut c_void));
            }
        }
    }
}

fn pump_messages() {
    let mut message = MSG::default();
    unsafe {
        while PeekMessageW(&mut message, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn pin_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        return LRESULT(1);
    }
    match message {
        WM_PAINT => {
            paint_pin(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tracking);
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_MOUSELEAVE_MESSAGE => {
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
            let _ = SetCursor(cursor);
            LRESULT(1)
        }
        WM_LBUTTONDOWN => {
            let _ = SetCapture(hwnd);
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let captured = GetCapture() == hwnd;
            if captured {
                let _ = ReleaseCapture();
            }
            if captured && point_inside_pin(hwnd, point_from_lparam(lparam)) {
                let target = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if target != 0 {
                    let _ = SetWindowPos(
                        HWND(target as *mut c_void),
                        HWND_NOTOPMOST,
                        0,
                        0,
                        0,
                        0,
                        windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                            | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                            | SWP_NOACTIVATE
                            | SWP_NOOWNERZORDER,
                    );
                }
                let _ = DestroyWindow(hwnd);
            } else {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

fn point_from_lparam(lparam: LPARAM) -> POINT {
    let packed = lparam.0 as u32;
    POINT {
        x: (packed as u16 as i16) as i32,
        y: ((packed >> 16) as u16 as i16) as i32,
    }
}

unsafe fn point_inside_pin(hwnd: HWND, point: POINT) -> bool {
    let mut client = RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client);
    point.x >= client.left
        && point.x < client.right
        && point.y >= client.top
        && point.y < client.bottom
}

unsafe fn cursor_inside_pin(hwnd: HWND) -> bool {
    let mut point = POINT::default();
    if GetCursorPos(&mut point).is_err() || !ScreenToClient(hwnd, &mut point).as_bool() {
        return false;
    }
    point_inside_pin(hwnd, point)
}

unsafe fn paint_pin(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let dc = BeginPaint(hwnd, &mut paint);
    let mut client = RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client);
    let background: HBRUSH = CreateSolidBrush(PIN_TRANSPARENT);
    FillRect(dc, &client, background);
    let _ = DeleteObject(background);

    let hovered = cursor_inside_pin(hwnd);
    let pressed = GetCapture() == hwnd;
    let color = pin_color(hovered, pressed);
    let width = client.right.max(1);
    let height = client.bottom.max(1);
    let visual =
        ((PIN_VISUAL_DIP * width + PIN_HIT_DIP - 1) / PIN_HIT_DIP).clamp(8, width.min(height));
    let left = (width - visual) / 2;
    let top = (height - visual) / 2 + if pressed { 1 } else { 0 };
    let radius = (visual * 3 / 10).max(2);
    let cx = left + visual / 2;
    let cy = top + radius + 1;
    let pen_width = ((2 * width + PIN_HIT_DIP - 1) / PIN_HIT_DIP).max(1);
    let pen = CreatePen(PS_SOLID, pen_width, color);
    let brush = CreateSolidBrush(color);
    let old_pen = SelectObject(dc, pen);
    let old_brush = SelectObject(dc, brush);
    let _ = Ellipse(dc, cx - radius, cy - radius, cx + radius, cy + radius);
    let _ = MoveToEx(dc, cx, cy + radius - 1, None);
    let _ = LineTo(dc, cx, top + visual);
    let _ = MoveToEx(dc, cx - radius - 1, cy + radius + 1, None);
    let _ = LineTo(dc, cx + radius + 1, cy + radius + 1);
    SelectObject(dc, old_brush);
    SelectObject(dc, old_pen);
    let _ = DeleteObject(brush);
    let _ = DeleteObject(pen);
    let _ = EndPaint(hwnd, &paint);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, FindWindowW, GetWindow, GetWindowDisplayAffinity, GetWindowLongPtrW,
        SendMessageW, SetWindowPos, GWL_EXSTYLE, GW_HWNDPREV, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, WM_SETCURSOR, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    };

    static PIN_WINDOW_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct TestWindow(HWND);

    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    fn test_topmost_window() -> TestWindow {
        TestWindow(unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                w!("STATIC"),
                None,
                WS_POPUP | WS_VISIBLE,
                300,
                240,
                420,
                260,
                HWND::default(),
                None,
                None,
                None,
            )
            .unwrap()
        })
    }

    fn window_is_above(upper: HWND, lower: HWND) -> bool {
        let mut current = lower;
        while let Ok(previous) = unsafe { GetWindow(current, GW_HWNDPREV) } {
            if previous == upper {
                return true;
            }
            current = previous;
        }
        false
    }

    #[test]
    fn topmost_pin_adoption_only_covers_existing_ordinary_topmost_windows() {
        let _guard = PIN_WINDOW_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let topmost = test_topmost_window();
        let plain = TestWindow(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                None,
                WS_POPUP | WS_VISIBLE,
                80,
                60,
                420,
                260,
                HWND::default(),
                None,
                None,
                None,
            )
            .unwrap()
        });
        let topmost_handle = WindowHandle(topmost.0 .0 as isize);
        let plain_handle = WindowHandle(plain.0 .0 as isize);

        let adopted = collect_adoptable_topmost(0);
        assert!(adopted.contains(&topmost_handle));
        assert!(!adopted.contains(&plain_handle));

        // 正常启动路径按进程排除：helper 自己的图钉/提示浮层不能被接管。
        let adopted_without_self = collect_adoptable_topmost(unsafe { GetCurrentProcessId() });
        assert!(!adopted_without_self.contains(&topmost_handle));
    }

    #[test]
    fn stationary_pins_follow_their_targets_z_order() {
        let _guard = PIN_WINDOW_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let first = test_topmost_window();
        let second = test_topmost_window();
        let first_handle = WindowHandle(first.0 .0 as isize);
        let second_handle = WindowHandle(second.0 .0 as isize);
        let first_marker = TestWindow(HWND(create_pin_window(first_handle) as *mut c_void));
        let second_marker = TestWindow(HWND(create_pin_window(second_handle) as *mut c_void));
        let mut pins = HashMap::from([
            (first_handle, ManagedPin::new(first_marker.0 .0 as isize)),
            (second_handle, ManagedPin::new(second_marker.0 .0 as isize)),
        ]);
        refresh_pins(&mut pins);
        assert_eq!(
            unsafe { GetWindow(first.0, GW_HWNDPREV).unwrap() },
            first_marker.0
        );
        assert_eq!(
            unsafe { GetWindow(second.0, GW_HWNDPREV).unwrap() },
            second_marker.0
        );
        let initial_rects = (pins[&first_handle].rect, pins[&second_handle].rect);
        for (target, marker, covered_marker) in [
            (first.0, first_marker.0, second_marker.0),
            (second.0, second_marker.0, first_marker.0),
            (first.0, first_marker.0, second_marker.0),
        ] {
            unsafe {
                SetWindowPos(
                    target,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
                .unwrap();
            }
            refresh_pins(&mut pins);
            assert_eq!(
                unsafe { GetWindow(target, GW_HWNDPREV).unwrap() },
                marker,
                "a stationary target must not cover its own pin"
            );
            assert_eq!(
                (pins[&first_handle].rect, pins[&second_handle].rect),
                initial_rects
            );
            assert_eq!(
                unsafe { GetWindow(first.0, GW_HWNDPREV).unwrap() },
                first_marker.0
            );
            assert_eq!(
                unsafe { GetWindow(second.0, GW_HWNDPREV).unwrap() },
                second_marker.0,
                "a covered target must keep its pin below the covering target"
            );
            assert!(
                window_is_above(target, covered_marker),
                "refreshing pins must preserve the targets' relative z-order"
            );
        }
        unsafe {
            let _ = ShowWindow(first.0, SW_HIDE);
        }
        refresh_pins(&mut pins);
        assert!(!pins[&first_handle].shown);
        assert!(!unsafe { IsWindowVisible(first_marker.0).as_bool() });
        unsafe {
            SetWindowPos(
                first.0,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .unwrap();
        }
        refresh_pins(&mut pins);
        assert!(pins[&first_handle].shown);
        assert!(unsafe { IsWindowVisible(first_marker.0).as_bool() });
        assert_eq!(
            unsafe { GetWindow(first.0, GW_HWNDPREV).unwrap() },
            first_marker.0
        );
    }

    #[test]
    fn visible_pins_back_off_after_quiet_and_resume_interactive_cadence() {
        for (stationary_ms, expected_ms) in
            [(0, 8), (249, 8), (250, 16), (500, 32), (750, 48), (0, 8)]
        {
            assert_eq!(
                pin_refresh_interval(true, Duration::from_millis(stationary_ms)),
                Duration::from_millis(expected_ms)
            );
        }
        assert_eq!(
            pin_refresh_interval(false, Duration::ZERO),
            PIN_IDLE_INTERVAL
        );
    }

    #[test]
    fn pin_metrics_scale_from_a_twenty_two_dip_hit_target() {
        let target = RECT {
            left: -800,
            top: 120,
            right: -200,
            bottom: 620,
        };
        assert_eq!(
            pin_rect(target, 96),
            RECT {
                left: -792,
                top: 128,
                right: -770,
                bottom: 150,
            }
        );
        assert_eq!(pin_rect(target, 192).right - pin_rect(target, 192).left, 44);
        assert_eq!(pin_color(false, false), PIN_NORMAL);
        assert_eq!(pin_color(true, false), PIN_HOVER);
        assert_eq!(pin_color(true, true), PIN_PRESSED);
        let packed = LPARAM((((-7_i16 as u16 as u32) << 16) | 12_u32) as isize);
        assert_eq!(point_from_lparam(packed), POINT { x: 12, y: -7 });
    }

    #[test]
    fn pin_window_is_excluded_from_capture() {
        let _guard = PIN_WINDOW_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let marker = create_pin_window(WindowHandle(0));
        let hwnd = HWND(marker as *mut c_void);
        let mut affinity = 0;
        unsafe {
            GetWindowDisplayAffinity(hwnd, &mut affinity).unwrap();
            let _ = DestroyWindow(hwnd);
        }
        super::super::assert_capture_exclusion_affinity(affinity);
    }

    #[test]
    fn enabling_pins_replays_targets_seen_while_disabled() {
        let _guard = PIN_WINDOW_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_topmost_pins();
        std::thread::sleep(Duration::from_millis(120));
        configure_topmost_pins(false);
        let target = test_topmost_window();
        set_topmost_pin_target(WindowHandle(target.0 .0 as isize), true);
        std::thread::sleep(Duration::from_millis(120));
        let marker_while_disabled =
            unsafe { FindWindowW(CLASS_NAME, PCWSTR::null()).unwrap_or_default() };
        assert!(marker_while_disabled.0.is_null());

        configure_topmost_pins(true);
        std::thread::sleep(Duration::from_millis(260));
        let marker = unsafe { FindWindowW(CLASS_NAME, PCWSTR::null()).unwrap_or_default() };
        assert!(!marker.0.is_null());

        clear_topmost_pins();
        configure_topmost_pins(true);
        std::thread::sleep(Duration::from_millis(120));
    }

    #[test]
    fn managed_pin_follows_target_and_click_cancels_topmost() {
        let _guard = PIN_WINDOW_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let target = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("ConvenientWindowPinTarget"),
                WS_POPUP | WS_VISIBLE,
                300,
                240,
                420,
                260,
                HWND::default(),
                None,
                None,
                None,
            )
            .unwrap()
        };
        unsafe {
            SetWindowPos(target, HWND_TOPMOST, 300, 240, 420, 260, SWP_NOACTIVATE).unwrap();
        }
        let handle = WindowHandle(target.0 as isize);
        configure_topmost_pins(true);
        set_topmost_pin_target(handle, true);
        std::thread::sleep(Duration::from_millis(260));
        let marker = unsafe { FindWindowW(CLASS_NAME, PCWSTR::null()).unwrap_or_default() };
        assert!(!marker.0.is_null());
        let mut target_before = RECT::default();
        let mut before = RECT::default();
        unsafe {
            GetWindowRect(target, &mut target_before).unwrap();
            GetWindowRect(marker, &mut before).unwrap();
        }
        let expected_before = pin_rect(target_before, unsafe { GetDpiForWindow(target) });
        assert_eq!(before, expected_before);
        for step in 1..=6 {
            let left = 520 + step * 24;
            let top = 360 + step * 16;
            unsafe {
                SetWindowPos(target, HWND_TOPMOST, left, top, 420, 260, SWP_NOACTIVATE).unwrap();
            }
            std::thread::sleep(Duration::from_millis(32));
            let mut target_after = RECT::default();
            let mut after = RECT::default();
            unsafe {
                GetWindowRect(target, &mut target_after).unwrap();
                GetWindowRect(marker, &mut after).unwrap();
            }
            assert_eq!(
                after,
                pin_rect(target_after, unsafe { GetDpiForWindow(target) })
            );
        }
        let mut after = RECT::default();
        unsafe {
            GetWindowRect(marker, &mut after).unwrap();
        }

        let click_x = (after.right - after.left) / 2;
        let click_y = (after.bottom - after.top) / 2;
        let click = LPARAM((((click_y as u16 as u32) << 16) | click_x as u16 as u32) as isize);
        unsafe {
            let cursor_result = SendMessageW(marker, WM_SETCURSOR, WPARAM(0), LPARAM(0));
            assert_eq!(cursor_result, LRESULT(1));
            let _ = SendMessageW(marker, WM_LBUTTONDOWN, WPARAM(0), click);
            let _ = SendMessageW(marker, WM_LBUTTONUP, WPARAM(0), click);
        }
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(
            unsafe { GetWindowLongPtrW(target, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0,
            0
        );
        clear_topmost_pins();
        unsafe {
            let _ = DestroyWindow(target);
        }
    }
}
