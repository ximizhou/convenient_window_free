use crate::platform::WindowHandle;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, CreateSolidBrush, DeleteObject, Ellipse, EndPaint, FillRect,
    InvalidateRect, LineTo, MoveToEx, ScreenToClient, SelectObject, HBRUSH, PAINTSTRUCT, PS_SOLID,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos,
    GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindow, IsWindowVisible, LoadCursorW,
    PeekMessageW, RegisterClassW, SetCursor, SetLayeredWindowAttributes, SetWindowDisplayAffinity,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, CREATESTRUCTW, CS_HREDRAW,
    CS_VREDRAW, GWLP_USERDATA, GWL_EXSTYLE, HWND_NOTOPMOST, HWND_TOPMOST, IDC_HAND, LWA_ALPHA,
    LWA_COLORKEY, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SW_HIDE, SW_SHOWNA,
    WDA_EXCLUDEFROMCAPTURE, WINDOW_EX_STYLE, WM_DESTROY, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_NCCREATE, WM_PAINT, WM_SETCURSOR, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TOPMOST as TOPMOST_STYLE, WS_POPUP,
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
static PIN_COMMANDS: OnceLock<mpsc::SyncSender<PinCommand>> = OnceLock::new();
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
    if PIN_ENABLED.swap(enabled, Ordering::AcqRel) != enabled {
        let _ = pin_commands().try_send(PinCommand::Configure(enabled));
    }
}

pub fn set_topmost_pin_target(handle: WindowHandle, topmost: bool) {
    if !PIN_ENABLED.load(Ordering::Acquire) && topmost {
        return;
    }
    let _ = pin_commands().try_send(PinCommand::SetTarget(handle, topmost));
}

pub fn clear_topmost_pins() {
    let _ = pin_commands().try_send(PinCommand::Clear);
}

fn pin_commands() -> &'static mpsc::SyncSender<PinCommand> {
    PIN_COMMANDS.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel(32);
        std::thread::Builder::new()
            .name("topmost-pin-ui".into())
            .spawn(move || run_pin_thread(receiver))
            .expect("start topmost pin UI thread");
        sender
    })
}

fn run_pin_thread(receiver: mpsc::Receiver<PinCommand>) {
    register_pin_class();
    let mut enabled = true;
    let mut pins: HashMap<WindowHandle, ManagedPin> = HashMap::new();
    loop {
        match receiver.recv_timeout(pin_refresh_interval(pins.values().any(|pin| pin.shown))) {
            Ok(PinCommand::SetTarget(handle, topmost)) => {
                if topmost && enabled {
                    pins.entry(handle)
                        .or_insert_with(|| ManagedPin::new(create_pin_window(handle)));
                } else {
                    remove_pin(&mut pins, handle);
                }
            }
            Ok(PinCommand::Configure(next)) => {
                enabled = next;
                if !enabled {
                    clear_pins(&mut pins);
                }
            }
            Ok(PinCommand::Clear) => clear_pins(&mut pins),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if enabled {
            refresh_pins(&mut pins);
        }
        pump_messages();
    }
    clear_pins(&mut pins);
}

fn pin_refresh_interval(has_visible_pin: bool) -> Duration {
    if has_visible_pin {
        PIN_FOLLOW_INTERVAL
    } else {
        PIN_IDLE_INTERVAL
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

fn refresh_pins(pins: &mut HashMap<WindowHandle, ManagedPin>) {
    let mut stale = Vec::new();
    for (&target, pin_state) in pins.iter_mut() {
        let target_hwnd = HWND(target.0 as *mut c_void);
        let marker_hwnd = HWND(pin_state.marker as *mut c_void);
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
            if pin_state.shown {
                unsafe {
                    let _ = ShowWindow(marker_hwnd, SW_HIDE);
                }
                pin_state.shown = false;
                pin_state.rect = None;
            }
            continue;
        }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(target_hwnd, &mut rect) }.is_err() {
            stale.push(target);
            continue;
        }
        let dpi = unsafe { GetDpiForWindow(target_hwnd) }.max(96);
        let pin = pin_rect(rect, dpi);
        if pin_state.rect != Some(pin) {
            let moved = unsafe {
                SetWindowPos(
                    marker_hwnd,
                    HWND_TOPMOST,
                    pin.left,
                    pin.top,
                    pin.right - pin.left,
                    pin.bottom - pin.top,
                    SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                )
            };
            if moved.is_err() {
                continue;
            }
            pin_state.rect = Some(pin);
        }
        if !pin_state.shown {
            unsafe {
                let _ = ShowWindow(marker_hwnd, SW_SHOWNA);
            }
            pin_state.shown = true;
        }
    }
    for handle in stale {
        remove_pin(pins, handle);
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
        unsafe {
            let _ = DestroyWindow(HWND(pin.marker as *mut c_void));
        }
    }
}

fn clear_pins(pins: &mut HashMap<WindowHandle, ManagedPin>) {
    for (_, pin) in pins.drain() {
        unsafe {
            let _ = DestroyWindow(HWND(pin.marker as *mut c_void));
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
            if captured && cursor_inside_pin(hwnd) {
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

unsafe fn cursor_inside_pin(hwnd: HWND) -> bool {
    let mut point = POINT::default();
    if GetCursorPos(&mut point).is_err() || !ScreenToClient(hwnd, &mut point).as_bool() {
        return false;
    }
    let mut client = RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client);
    point.x >= client.left
        && point.x < client.right
        && point.y >= client.top
        && point.y < client.bottom
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
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, FindWindowW, GetWindowDisplayAffinity, GetWindowLongPtrW, SendMessageW,
        SetCursorPos, SetWindowPos, GWL_EXSTYLE, SWP_NOACTIVATE, WM_SETCURSOR, WS_EX_TOPMOST,
        WS_POPUP, WS_VISIBLE,
    };

    #[test]
    fn pin_refreshes_at_interactive_cadence_only_while_visible() {
        assert_eq!(pin_refresh_interval(true), Duration::from_millis(8));
        assert_eq!(pin_refresh_interval(false), Duration::from_millis(80));
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
    }

    #[test]
    fn pin_window_is_excluded_from_capture() {
        let marker = create_pin_window(WindowHandle(0));
        let hwnd = HWND(marker as *mut c_void);
        let mut affinity = 0;
        unsafe {
            GetWindowDisplayAffinity(hwnd, &mut affinity).unwrap();
            let _ = DestroyWindow(hwnd);
        }
        assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE.0);
    }

    #[test]
    fn managed_pin_follows_target_and_click_cancels_topmost() {
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

        unsafe {
            SetCursorPos(
                (after.left + after.right) / 2,
                (after.top + after.bottom) / 2,
            )
            .unwrap();
            let cursor_result = SendMessageW(marker, WM_SETCURSOR, WPARAM(0), LPARAM(0));
            assert_eq!(cursor_result, LRESULT(1));
            let _ = SendMessageW(marker, WM_LBUTTONDOWN, WPARAM(0), LPARAM(0));
            let _ = SendMessageW(marker, WM_LBUTTONUP, WPARAM(0), LPARAM(0));
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
