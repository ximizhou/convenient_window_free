use super::gesture_overlay::GestureHintManager;
use super::input::{take_gesture_trail, GestureTrail};
#[cfg(test)]
use crate::platform::Point as AppPoint;
use crate::platform::Rect;
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::RECT;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Gdi::CreateCompatibleBitmap;
use windows::Win32::Graphics::Gdi::GetDC;
use windows::Win32::Graphics::Gdi::ReleaseDC;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateFontW, CreateSolidBrush, DeleteDC, DeleteObject, FillRect,
    SelectObject, SetBkMode, SetTextColor, TextOutW, AC_SRC_OVER, BLENDFUNCTION, TRANSPARENT,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, GetCursorPos, GetSystemMetrics, GetWindowRect, IsWindow,
    PeekMessageW, SetWindowDisplayAffinity, SetWindowPos, ShowWindow, TranslateMessage,
    UpdateLayeredWindow, MSG, PM_REMOVE, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOWNA, ULW_ALPHA, WDA_EXCLUDEFROMCAPTURE, WINDOW_STYLE,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    WS_VISIBLE,
};

const HOTZONE_HINT_COLOR: COLORREF = COLORREF(0x00FFFFFF);
// Product accent #5B8CFF in COLORREF's 0x00BBGGRR byte order.
const EDGE_HIDE_PREVIEW_COLOR: COLORREF = COLORREF(0x00FF8C5B);

static HINT_COMMANDS: OnceLock<mpsc::SyncSender<HintCommand>> = OnceLock::new();
static HINT_REQUESTS: OnceLock<Mutex<HintRequestState>> = OnceLock::new();

#[derive(Default)]
struct HintRequestState {
    hotzone: Option<Rect>,
    edge_hide_preview: Option<Rect>,
    strips: Vec<Rect>,
    gesture: Option<GestureTrail>,
    gesture_label: Option<String>,
    gesture_dirty: bool,
}

enum HintCommand {
    UpdateHotzone(Option<Rect>),
    UpdateEdgeHidePreview(Option<Rect>),
    UpdateStrips(Vec<Rect>),
    UpdateGesture,
    HideGestureAndReport(mpsc::Sender<()>),
    ShowOcrToast {
        owner: isize,
        text: String,
        success: bool,
    },
    HideAll,
    #[cfg(test)]
    ReportThread(mpsc::Sender<u32>),
}

pub fn update_hotzone_hints(rect: Option<Rect>) {
    let Ok(mut state) = HINT_REQUESTS
        .get_or_init(|| Mutex::new(HintRequestState::default()))
        .lock()
    else {
        return;
    };
    if state.hotzone == rect {
        return;
    }
    if send_hint_command(HintCommand::UpdateHotzone(rect)) {
        state.hotzone = rect;
    }
}

pub fn update_edge_hide_preview(rect: Option<Rect>) {
    let Ok(mut state) = HINT_REQUESTS
        .get_or_init(|| Mutex::new(HintRequestState::default()))
        .lock()
    else {
        return;
    };
    if state.edge_hide_preview == rect {
        return;
    }
    if send_hint_command(HintCommand::UpdateEdgeHidePreview(rect)) {
        state.edge_hide_preview = rect;
    }
}

pub fn update_strip_hints(strips: &[Rect]) {
    let Ok(mut state) = HINT_REQUESTS
        .get_or_init(|| Mutex::new(HintRequestState::default()))
        .lock()
    else {
        return;
    };
    if state.strips.as_slice() == strips {
        return;
    }
    if send_hint_command(HintCommand::UpdateStrips(strips.to_vec())) {
        state.strips.clear();
        state.strips.extend_from_slice(strips);
    }
}

pub fn hide_hotzone_hints() {
    let Ok(mut state) = HINT_REQUESTS
        .get_or_init(|| Mutex::new(HintRequestState::default()))
        .lock()
    else {
        return;
    };
    if state.hotzone.is_none() && state.edge_hide_preview.is_none() && state.strips.is_empty() {
        return;
    }
    if send_hint_command(HintCommand::HideAll) {
        state.hotzone = None;
        state.edge_hide_preview = None;
        state.strips.clear();
    }
}

pub fn update_gesture_overlay(label: Option<&str>) {
    let Some(trail) = take_gesture_trail() else {
        return;
    };
    let Ok(mut state) = HINT_REQUESTS
        .get_or_init(|| Mutex::new(HintRequestState::default()))
        .lock()
    else {
        return;
    };
    let label = label.map(str::to_string);
    state.append_trail(trail, label);
    // The mailbox retains every segment if the wake-up queue is full.
    let _ = send_hint_command(HintCommand::UpdateGesture);
}

impl HintRequestState {
    fn append_trail(&mut self, trail: GestureTrail, label: Option<String>) {
        let same = self.gesture.as_ref().is_some_and(|old| old.id == trail.id);
        self.gesture_dirty |= !same || !trail.points.is_empty() || self.gesture_label != label;
        if same {
            self.gesture.as_mut().unwrap().points.extend(trail.points);
        } else {
            self.gesture = Some(trail);
        }
        self.gesture_label = label;
    }
}

pub fn hide_gesture_overlay() {
    let Ok(mut state) = HINT_REQUESTS
        .get_or_init(|| Mutex::new(HintRequestState::default()))
        .lock()
    else {
        return;
    };
    if state.gesture.is_none() {
        return;
    }
    state.gesture = None;
    state.gesture_label = None;
    state.gesture_dirty = true;
    let _ = send_hint_command(HintCommand::UpdateGesture);
}

pub fn hide_gesture_overlay_before_capture() {
    if let Ok(mut state) = HINT_REQUESTS
        .get_or_init(|| Mutex::new(HintRequestState::default()))
        .lock()
    {
        state.gesture = None;
        state.gesture_label = None;
        state.gesture_dirty = true;
    }
    let (reply, response) = mpsc::channel();
    if hint_commands()
        .send(HintCommand::HideGestureAndReport(reply))
        .is_ok()
    {
        let _ = response.recv_timeout(Duration::from_millis(500));
    }
}

pub fn show_ocr_toast(owner: isize, text: String, success: bool) {
    let _ = send_hint_command(HintCommand::ShowOcrToast {
        owner,
        text,
        success,
    });
}

fn send_hint_command(command: HintCommand) -> bool {
    hint_commands().try_send(command).is_ok()
}

fn hint_commands() -> &'static mpsc::SyncSender<HintCommand> {
    HINT_COMMANDS.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel(8);
        std::thread::Builder::new()
            .name("magic-corners-hints".to_string())
            .spawn(move || run_hint_thread(receiver))
            .expect("start hint UI thread");
        sender
    })
}

fn run_hint_thread(receiver: mpsc::Receiver<HintCommand>) {
    let mut hotzone_hints = HintManager::new(62, HOTZONE_HINT_COLOR, None);
    let mut edge_hide_preview = HintManager::new(210, EDGE_HIDE_PREVIEW_COLOR, None);
    let mut strip_hints = StripHintManager::new();
    let mut gesture_hint = GestureHintManager::new();
    let mut ocr_toast = OcrToastManager::new();

    loop {
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(command) => match command {
                HintCommand::UpdateHotzone(rect) => hotzone_hints.update(rect),
                HintCommand::UpdateEdgeHidePreview(rect) => edge_hide_preview.update(rect),
                HintCommand::UpdateStrips(strips) => strip_hints.update(&strips),
                HintCommand::UpdateGesture => {}
                HintCommand::HideGestureAndReport(reply) => {
                    gesture_hint.hide();
                    let _ = reply.send(());
                }
                HintCommand::ShowOcrToast {
                    owner,
                    text,
                    success,
                } => ocr_toast.show(owner, &text, success),
                HintCommand::HideAll => {
                    hotzone_hints.hide_all();
                    edge_hide_preview.hide_all();
                    strip_hints.hide_all();
                }
                #[cfg(test)]
                HintCommand::ReportThread(reply) => {
                    let _ = reply.send(unsafe { GetCurrentThreadId() });
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        let pending = HINT_REQUESTS.get().and_then(|state| {
            let mut state = state.lock().ok()?;
            if !std::mem::take(&mut state.gesture_dirty) {
                return None;
            }
            let trail = state.gesture.as_mut().map(|trail| GestureTrail {
                id: trail.id,
                points: std::mem::take(&mut trail.points),
            });
            Some((trail, state.gesture_label.clone()))
        });
        if let Some((trail, label)) = pending {
            if let Some(trail) = trail {
                gesture_hint.update(trail.id, &trail.points, label.as_deref());
            } else {
                gesture_hint.hide();
            }
        }
        ocr_toast.tick();
        pump_window_messages();
    }
}

struct OcrToastManager {
    window: Option<HintWindow>,
    hide_at: Option<Instant>,
}

impl OcrToastManager {
    fn new() -> Self {
        Self {
            window: None,
            hide_at: None,
        }
    }

    fn show(&mut self, owner: isize, text: &str, success: bool) {
        const WIDTH: i32 = 340;
        const HEIGHT: i32 = 52;
        let mut left;
        let mut top;
        let owner = HWND(owner as *mut core::ffi::c_void);
        let mut owner_rect = RECT::default();
        if !owner.0.is_null()
            && unsafe { IsWindow(owner).as_bool() }
            && unsafe { GetWindowRect(owner, &mut owner_rect).is_ok() }
        {
            left = owner_rect.right - WIDTH - 16;
            top = owner_rect.top + 52;
        } else if owner.0.is_null() {
            let virtual_left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
            let virtual_top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
            let virtual_width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
            let virtual_height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
            left = virtual_left + (virtual_width - WIDTH) / 2;
            top = virtual_top + virtual_height - HEIGHT - 48;
        } else {
            let mut cursor = POINT::default();
            let _ = unsafe { GetCursorPos(&mut cursor) };
            left = cursor.x + 18;
            top = cursor.y + 20;
        }
        let virtual_left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let virtual_top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let virtual_width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }.max(WIDTH);
        let virtual_height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) }.max(HEIGHT);
        left = left.clamp(virtual_left, virtual_left + virtual_width - WIDTH);
        top = top.clamp(virtual_top, virtual_top + virtual_height - HEIGHT);
        let rect = Rect {
            left,
            top,
            right: left + WIDTH,
            bottom: top + HEIGHT,
        };
        let mut display = text.chars().take(42).collect::<String>();
        if text.chars().count() > 42 {
            display.push_str("...");
        }
        self.window
            .get_or_insert_with(HintWindow::create)
            .show_toast(rect, &display, success);
        self.hide_at = Some(Instant::now() + Duration::from_millis(2200));
    }
    fn tick(&mut self) {
        if self
            .hide_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            if let Some(window) = self.window {
                window.hide();
            }
            self.hide_at = None;
        }
    }
}

fn pump_window_messages() {
    let mut message = MSG::default();
    unsafe {
        while PeekMessageW(&mut message, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

// --- shared primitive ---

#[derive(Clone, Copy)]
pub(super) struct HintWindow {
    pub(super) hwnd: isize,
    owner_thread_id: u32,
}

impl HintWindow {
    pub(super) fn create() -> Self {
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST
                    | WS_EX_NOACTIVATE,
                w!("STATIC"),
                None,
                WS_POPUP | WS_VISIBLE | WINDOW_STYLE(0),
                0,
                0,
                1,
                1,
                HWND::default(),
                None,
                None,
                None,
            )
            .unwrap_or_default()
        };
        let win = Self {
            hwnd: hwnd.0 as isize,
            owner_thread_id: unsafe { GetCurrentThreadId() },
        };
        unsafe {
            let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        }
        win.hide();
        win
    }

    fn show(&self, rect: Rect, alpha: u8) {
        self.show_colored(rect, HOTZONE_HINT_COLOR, None, alpha);
    }

    fn show_colored(
        &self,
        rect: Rect,
        fill_color: COLORREF,
        border_color: Option<COLORREF>,
        alpha: u8,
    ) {
        debug_assert_eq!(self.owner_thread_id, unsafe { GetCurrentThreadId() });
        let hwnd = HWND(self.hwnd as *mut core::ffi::c_void);
        let w = rect.width().max(1);
        let h = rect.height().max(1);
        unsafe {
            // Position the window first
            let _ = SetWindowPos(
                hwnd,
                HWND::default(),
                rect.left,
                rect.top,
                w,
                h,
                windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER
                    | windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE
                    | windows::Win32::UI::WindowsAndMessaging::SWP_NOOWNERZORDER,
            );

            // Draw white with per-pixel alpha using UpdateLayeredWindow
            let screen_dc = GetDC(HWND::default());
            let mem_dc = CreateCompatibleDC(screen_dc);
            let bmp = CreateCompatibleBitmap(screen_dc, w, h);
            let old = SelectObject(mem_dc, bmp);

            let fill_rect = RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            };
            if let Some(border_color) = border_color {
                let border = CreateSolidBrush(border_color);
                FillRect(mem_dc, &fill_rect, border);
                let _ = DeleteObject(border);
                let inset = 3.min(w.saturating_sub(1) / 2).min(h.saturating_sub(1) / 2);
                let inner = RECT {
                    left: inset,
                    top: inset,
                    right: w - inset,
                    bottom: h - inset,
                };
                let fill = CreateSolidBrush(fill_color);
                FillRect(mem_dc, &inner, fill);
                let _ = DeleteObject(fill);
            } else {
                let fill = CreateSolidBrush(fill_color);
                FillRect(mem_dc, &fill_rect, fill);
                let _ = DeleteObject(fill);
            }

            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: alpha,
                AlphaFormat: 0, // no per-pixel alpha, use SourceConstantAlpha
            };
            let dst = POINT {
                x: rect.left,
                y: rect.top,
            };
            let sz = SIZE { cx: w, cy: h };
            let src = POINT { x: 0, y: 0 };
            let _ = UpdateLayeredWindow(
                hwnd,
                screen_dc,
                Some(&dst),
                Some(&sz),
                mem_dc,
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );

            SelectObject(mem_dc, old);
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(mem_dc);
            ReleaseDC(HWND::default(), screen_dc);

            let _ = ShowWindow(hwnd, SW_SHOWNA);
        }
    }

    fn show_toast(&self, rect: Rect, text: &str, success: bool) {
        debug_assert_eq!(self.owner_thread_id, unsafe { GetCurrentThreadId() });
        let hwnd = HWND(self.hwnd as *mut core::ffi::c_void);
        let width = rect.width().max(1);
        let height = rect.height().max(1);
        unsafe {
            let screen_dc = GetDC(HWND::default());
            let dc = CreateCompatibleDC(screen_dc);
            let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
            let old_bitmap = SelectObject(dc, bitmap);
            let background =
                CreateSolidBrush(COLORREF(if success { 0x003A461F } else { 0x002D2D58 }));
            FillRect(
                dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                },
                background,
            );
            let _ = DeleteObject(background);
            let accent = CreateSolidBrush(COLORREF(if success { 0x0090C34F } else { 0x00666EE8 }));
            FillRect(
                dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: 5,
                    bottom: height,
                },
                accent,
            );
            let _ = DeleteObject(accent);
            let font = CreateFontW(
                18,
                0,
                0,
                0,
                500,
                0,
                0,
                0,
                1,
                0,
                0,
                5,
                0,
                w!("Microsoft YaHei UI"),
            );
            let old_font = SelectObject(dc, font);
            let _ = SetBkMode(dc, TRANSPARENT);
            let _ = SetTextColor(dc, COLORREF(0x00FFFFFF));
            let wide = text.encode_utf16().collect::<Vec<_>>();
            let _ = TextOutW(dc, 18, 15, &wide);
            let destination = POINT {
                x: rect.left,
                y: rect.top,
            };
            let size = SIZE {
                cx: width,
                cy: height,
            };
            let source = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 238,
                AlphaFormat: 0,
            };
            let _ = UpdateLayeredWindow(
                hwnd,
                screen_dc,
                Some(&destination),
                Some(&size),
                dc,
                Some(&source),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            SelectObject(dc, old_font);
            SelectObject(dc, old_bitmap);
            let _ = DeleteObject(font);
            let _ = DeleteObject(bitmap);
            let _ = DeleteDC(dc);
            ReleaseDC(HWND::default(), screen_dc);
            let _ = ShowWindow(hwnd, SW_SHOWNA);
        }
    }
    pub(super) fn hide(&self) {
        debug_assert_eq!(self.owner_thread_id, unsafe { GetCurrentThreadId() });
        unsafe {
            let _ = ShowWindow(HWND(self.hwnd as *mut core::ffi::c_void), SW_HIDE);
        }
    }
}

// --- hotzone hints ---

struct HintManager {
    window: Option<HintWindow>,
    current_rect: Option<Rect>,
    alpha: u8,
    fill_color: COLORREF,
    border_color: Option<COLORREF>,
}

impl HintManager {
    fn new(alpha: u8, fill_color: COLORREF, border_color: Option<COLORREF>) -> Self {
        Self {
            window: None,
            current_rect: None,
            alpha,
            fill_color,
            border_color,
        }
    }

    fn update(&mut self, rect: Option<Rect>) {
        if !replace_if_changed(&mut self.current_rect, rect) {
            return;
        }
        let Some(rect) = rect else {
            if let Some(window) = &self.window {
                window.hide();
            }
            return;
        };
        let window = self.window.get_or_insert_with(HintWindow::create);
        window.show_colored(rect, self.fill_color, self.border_color, self.alpha);
    }

    fn hide_all(&mut self) {
        self.current_rect = None;
        if let Some(window) = &self.window {
            window.hide();
        }
    }
}

// --- strip hints ---

struct StripHintManager {
    windows: Vec<HintWindow>,
    current_strips: Vec<Rect>,
}

impl StripHintManager {
    fn new() -> Self {
        Self {
            windows: Vec::new(),
            current_strips: Vec::new(),
        }
    }

    fn update(&mut self, strips: &[Rect]) {
        if self.current_strips == strips {
            return;
        }
        while self.windows.len() < strips.len() {
            self.windows.push(HintWindow::create());
        }
        for (i, rect) in strips.iter().enumerate() {
            self.windows[i].show(rect.inflate(1), 80);
        }
        for w in self.windows.iter().skip(strips.len()) {
            w.hide();
        }
        self.current_strips.clear();
        self.current_strips.extend_from_slice(strips);
    }

    fn hide_all(&mut self) {
        self.current_strips.clear();
        for w in &self.windows {
            w.hide();
        }
    }
}

fn replace_if_changed<T: PartialEq>(current: &mut T, next: T) -> bool {
    if *current == next {
        return false;
    }
    *current = next;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gesture_mailbox_merges_pending_segments_and_replaces_previous_capture() {
        let mut state = HintRequestState::default();
        for x in 0..2000 {
            state.append_trail(
                GestureTrail {
                    id: 1,
                    points: vec![AppPoint { x, y: 10 }],
                },
                None,
            );
        }
        assert!(state.gesture_dirty);
        let trail = state.gesture.as_mut().unwrap();
        let delivered = std::mem::take(&mut trail.points);
        assert_eq!(delivered.len(), 2000);
        for (x, point) in delivered.iter().enumerate() {
            assert_eq!(point.x, x as i32);
        }
        state.gesture_dirty = false;
        state.append_trail(
            GestureTrail {
                id: 1,
                points: vec![],
            },
            None,
        );
        assert!(!state.gesture_dirty);
        state.append_trail(
            GestureTrail {
                id: 2,
                points: vec![AppPoint { x: 4, y: 8 }],
            },
            Some("复制".into()),
        );
        assert_eq!(
            state.gesture.as_ref().unwrap().points,
            vec![AppPoint { x: 4, y: 8 }]
        );
        assert!(state.gesture_dirty);
    }
    use std::time::Duration;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, GetWindowDisplayAffinity};

    #[test]
    fn edge_hide_preview_uses_the_product_accent_blue() {
        assert_eq!(EDGE_HIDE_PREVIEW_COLOR.0, 0x00FF8C5B);
    }

    #[test]
    fn unchanged_hint_geometry_does_not_require_another_paint() {
        let rect = Rect {
            left: 10,
            top: 20,
            right: 30,
            bottom: 40,
        };
        let mut previous = None;

        assert!(replace_if_changed(&mut previous, Some(rect)));
        assert!(!replace_if_changed(&mut previous, Some(rect)));
        assert!(replace_if_changed(&mut previous, None));
        assert!(!replace_if_changed(&mut previous, None));
    }

    #[test]
    fn hotzone_hint_hides_and_reuses_its_window_without_hiding_other_hints() {
        use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;
        let rect = Rect {
            left: 0,
            top: 0,
            right: 10,
            bottom: 10,
        };
        let mut hotzone = HintManager::new(62, HOTZONE_HINT_COLOR, None);
        let mut edge = HintManager::new(210, EDGE_HIDE_PREVIEW_COLOR, None);
        hotzone.update(None);
        assert!(
            hotzone.window.is_none(),
            "disabled hints must not create a window"
        );
        hotzone.update(Some(rect));
        edge.update(Some(rect));
        let hwnd = HWND(hotzone.window.as_ref().unwrap().hwnd as *mut core::ffi::c_void);
        let edge_hwnd = HWND(edge.window.as_ref().unwrap().hwnd as *mut core::ffi::c_void);
        assert!(unsafe { IsWindowVisible(hwnd).as_bool() });
        hotzone.update(None);
        assert!(!unsafe { IsWindowVisible(hwnd).as_bool() });
        assert!(unsafe { IsWindowVisible(edge_hwnd).as_bool() });
        for _ in 0..200 {
            hotzone.update(None);
        }
        hotzone.update(Some(rect));
        assert_eq!(hotzone.window.as_ref().unwrap().hwnd, hwnd.0 as isize);
        assert!(unsafe { IsWindowVisible(hwnd).as_bool() });
        unsafe {
            DestroyWindow(hwnd).unwrap();
            DestroyWindow(edge_hwnd).unwrap();
        }
    }

    #[test]
    fn hint_windows_are_excluded_from_screen_capture() {
        let window = HintWindow::create();
        let hwnd = HWND(window.hwnd as *mut core::ffi::c_void);
        let mut affinity = 0;

        unsafe {
            GetWindowDisplayAffinity(hwnd, &mut affinity).unwrap();
            let _ = DestroyWindow(hwnd);
        }

        super::super::assert_capture_exclusion_affinity(affinity);
    }

    #[test]
    fn hint_commands_stay_on_one_dedicated_os_thread() {
        let caller_thread = unsafe { GetCurrentThreadId() };
        for _ in 0..200 {
            update_hotzone_hints(Some(Rect {
                left: 0,
                top: 0,
                right: 10,
                bottom: 10,
            }));
            update_strip_hints(&[Rect {
                left: 0,
                top: 120,
                right: 14,
                bottom: 700,
            }]);
        }

        let worker_thread = report_hint_thread();
        let same_worker_thread = report_hint_thread();

        assert_ne!(worker_thread, caller_thread);
        assert_eq!(worker_thread, same_worker_thread);
    }

    fn report_hint_thread() -> u32 {
        let (reply, response) = mpsc::channel();
        hint_commands()
            .send(HintCommand::ReportThread(reply))
            .expect("hint worker should be running");
        response
            .recv_timeout(Duration::from_secs(2))
            .expect("hint worker should answer")
    }
}
