use crate::config::{GestureTriggerButton, MouseButton};
use crate::platform::{GestureCapture, InputState, Point, WindowDragCapture, WindowDragMode};
use anyhow::{bail, Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_LBUTTON, VK_LWIN, VK_MBUTTON, VK_MENU, VK_RBUTTON,
    VK_RETURN, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PeekMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    HHOOK, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, WH_MOUSE_LL, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

const HOOK_NOT_STARTED: u8 = 0;
const HOOK_STARTING: u8 = 1;
const HOOK_HEALTHY: u8 = 2;
const HOOK_FAILED: u8 = 3;
const HOOK_STOPPING: u8 = 4;
const HOOK_STOPPED: u8 = 5;

static HOOK_STATE: AtomicU8 = AtomicU8::new(HOOK_NOT_STARTED);
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static WHEEL_DELTA: AtomicI64 = AtomicI64::new(0);
static LEFT_CLICK_COUNT: AtomicU64 = AtomicU64::new(0);
static RIGHT_CLICK_COUNT: AtomicU64 = AtomicU64::new(0);
static INPUT_EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static LEFT_CLICK_MODIFIER_SEQUENCES: [AtomicU64; 16] = [const { AtomicU64::new(0) }; 16];
static RIGHT_CLICK_MODIFIER_SEQUENCES: [AtomicU64; 16] = [const { AtomicU64::new(0) }; 16];
static WHEEL_MODIFIER_SEQUENCES: [AtomicU64; 16] = [const { AtomicU64::new(0) }; 16];
static GESTURE_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);
static GESTURE_CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
static GESTURE_TRIGGER: AtomicU8 = AtomicU8::new(0);
static EXPECTED_HELPER_INJECTED_EVENTS: AtomicU8 = AtomicU8::new(0);
static WINDOW_DRAG_ENABLED: AtomicBool = AtomicBool::new(false);
static WINDOW_DRAG_MOVE_BUTTON: AtomicU8 = AtomicU8::new(0);
static WINDOW_DRAG_RESIZE_BUTTON: AtomicU8 = AtomicU8::new(1);
static WINDOW_DRAG_MOVE_MODIFIERS: AtomicU8 = AtomicU8::new(2);
static WINDOW_DRAG_RESIZE_MODIFIERS: AtomicU8 = AtomicU8::new(2);
static WINDOW_DRAG_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static SUPPRESSED_WINDOW_DRAG_UPS: AtomicU8 = AtomicU8::new(0);

#[derive(Default)]
struct GestureHookState {
    active_trigger: Option<GestureTriggerButton>,
    active_modifiers: u8,
    points: Vec<Point>,
    completed: VecDeque<GestureCapture>,
    cancelled: bool,
}

fn gesture_state() -> &'static Mutex<GestureHookState> {
    static STATE: OnceLock<Mutex<GestureHookState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(GestureHookState::default()))
}

#[derive(Default)]
struct WindowDragHookState {
    capture: Option<(WindowDragCapture, MouseButton)>,
}

fn window_drag_state() -> &'static Mutex<WindowDragHookState> {
    static STATE: OnceLock<Mutex<WindowDragHookState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(WindowDragHookState::default()))
}

pub fn install_mouse_hook() -> Result<()> {
    match HOOK_STATE.compare_exchange(
        HOOK_NOT_STARTED,
        HOOK_STARTING,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => {
            if let Err(error) = std::thread::Builder::new()
                .name("magic-corners-mouse-hook".to_string())
                .spawn(run_mouse_hook_thread)
            {
                HOOK_STATE.store(HOOK_FAILED, Ordering::Release);
                return Err(error.into());
            }
        }
        Err(HOOK_HEALTHY) => return Ok(()),
        Err(state) => bail!("mouse hook cannot start from state {state}"),
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        match HOOK_STATE.load(Ordering::Acquire) {
            HOOK_HEALTHY => return Ok(()),
            HOOK_FAILED | HOOK_STOPPED => bail!("mouse hook thread failed during startup"),
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    bail!("mouse hook thread did not become healthy in time")
}

pub fn mouse_hook_is_healthy() -> bool {
    HOOK_STATE.load(Ordering::Acquire) == HOOK_HEALTHY
}

pub fn stop_mouse_hook() {
    if HOOK_STATE
        .compare_exchange(
            HOOK_HEALTHY,
            HOOK_STOPPING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return;
    }
    let thread_id = HOOK_THREAD_ID.load(Ordering::Acquire);
    if thread_id != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while std::time::Instant::now() < deadline
        && HOOK_STATE.load(Ordering::Acquire) == HOOK_STOPPING
    {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn run_mouse_hook_thread() {
    let result = std::panic::catch_unwind(|| unsafe {
        HOOK_THREAD_ID.store(GetCurrentThreadId(), Ordering::Release);
        let mut message = MSG::default();
        let _ = PeekMessageW(&mut message, HWND::default(), 0, 0, PM_NOREMOVE);
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), HINSTANCE::default(), 0)?;
        HOOK_STATE.store(HOOK_HEALTHY, Ordering::Release);
        message_loop();
        let _ = UnhookWindowsHookEx(hook);
        Ok::<(), windows::core::Error>(())
    });
    let stopping = HOOK_STATE.load(Ordering::Acquire) == HOOK_STOPPING;
    HOOK_THREAD_ID.store(0, Ordering::Release);
    HOOK_STATE.store(
        if stopping { HOOK_STOPPED } else { HOOK_FAILED },
        Ordering::Release,
    );
    if result.is_err() || result.is_ok_and(|inner| inner.is_err()) {
        crate::logging::write_line("input: mouse hook thread failed");
    }
}

pub fn configure_gesture_capture(enabled: bool, trigger: GestureTriggerButton) {
    GESTURE_TRIGGER.store(trigger_to_u8(trigger), Ordering::Release);
    GESTURE_CAPTURE_ENABLED.store(enabled, Ordering::Release);
    if !enabled {
        cancel_gesture_capture();
    }
}

fn cancel_gesture_capture() {
    if let Ok(mut state) = gesture_state().lock() {
        if state.active_trigger.is_some() {
            state.cancelled = true;
            state.points.clear();
        }
    }
}

pub fn configure_window_drag_capture(
    enabled: bool,
    move_button: MouseButton,
    move_modifiers: u8,
    resize_button: MouseButton,
    resize_modifiers: u8,
) {
    WINDOW_DRAG_MOVE_BUTTON.store(mouse_button_to_u8(move_button), Ordering::Release);
    WINDOW_DRAG_RESIZE_BUTTON.store(mouse_button_to_u8(resize_button), Ordering::Release);
    WINDOW_DRAG_MOVE_MODIFIERS.store(move_modifiers, Ordering::Release);
    WINDOW_DRAG_RESIZE_MODIFIERS.store(resize_modifiers, Ordering::Release);
    WINDOW_DRAG_ENABLED.store(enabled, Ordering::Release);
    if !enabled {
        cancel_window_drag_capture();
    }
}

pub fn take_window_drag_capture() -> Option<WindowDragCapture> {
    let mut state = window_drag_state().lock().ok()?;
    let capture = state.capture.map(|(capture, _)| capture)?;
    if capture.finished {
        state.capture = None;
    }
    Some(capture)
}

pub fn cancel_window_drag_capture() {
    if let Ok(mut state) = window_drag_state().lock() {
        if let Some((_, button)) = state.capture.take() {
            SUPPRESSED_WINDOW_DRAG_UPS.fetch_or(mouse_button_bit(button), Ordering::AcqRel);
        }
    }
}

pub fn take_gesture_capture() -> Option<GestureCapture> {
    gesture_state().lock().ok()?.completed.pop_front()
}

pub fn active_gesture_points() -> Vec<Point> {
    if !GESTURE_CAPTURE_ACTIVE.load(Ordering::Acquire) {
        return Vec::new();
    }
    gesture_state()
        .lock()
        .map(|state| state.points.clone())
        .unwrap_or_default()
}

pub fn mark_helper_injected_events(count: u8) {
    EXPECTED_HELPER_INJECTED_EVENTS.store(count, Ordering::Release);
}

pub fn input_state() -> InputState {
    InputState {
        left_down: key_down(VK_LBUTTON),
        right_down: key_down(VK_RBUTTON),
        middle_down: key_down(VK_MBUTTON),
        escape_down: key_down(VK_ESCAPE),
        enter_down: key_down(VK_RETURN),
        modifiers: current_modifier_mask(),
        left_click_count: LEFT_CLICK_COUNT.load(Ordering::Relaxed),
        right_click_count: RIGHT_CLICK_COUNT.load(Ordering::Relaxed),
        wheel_delta: WHEEL_DELTA.load(Ordering::Relaxed),
        left_click_modifier_sequences: std::array::from_fn(|index| {
            LEFT_CLICK_MODIFIER_SEQUENCES[index].load(Ordering::Acquire)
        }),
        right_click_modifier_sequences: std::array::from_fn(|index| {
            RIGHT_CLICK_MODIFIER_SEQUENCES[index].load(Ordering::Acquire)
        }),
        wheel_modifier_sequences: std::array::from_fn(|index| {
            WHEEL_MODIFIER_SEQUENCES[index].load(Ordering::Acquire)
        }),
    }
}

fn record_modifier_event(sequences: &[AtomicU64; 16], modifiers: u8) {
    let sequence = INPUT_EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;
    sequences[usize::from(modifiers & 0x0f)].store(sequence, Ordering::Release);
}

fn current_modifier_mask() -> u8 {
    let mut mask = 0;
    if key_down(VK_CONTROL) {
        mask |= 1;
    }
    if key_down(VK_MENU) {
        mask |= 2;
    }
    if key_down(VK_SHIFT) {
        mask |= 4;
    }
    if key_down(VK_LWIN) || key_down(VK_RWIN) {
        mask |= 8;
    }
    mask
}

fn key_down(key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(key.0 as i32) & 0x8000u16 as i16) != 0 }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let hook = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let message = wparam.0 as u32;
        if handle_window_drag_message(message, hook) {
            return LRESULT(1);
        }
        let modifiers = current_modifier_mask();
        match message {
            WM_LBUTTONDOWN => {
                record_modifier_event(&LEFT_CLICK_MODIFIER_SEQUENCES, modifiers);
                LEFT_CLICK_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            WM_RBUTTONDOWN => {
                record_modifier_event(&RIGHT_CLICK_MODIFIER_SEQUENCES, modifiers);
                RIGHT_CLICK_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            WM_MOUSEWHEEL => {
                record_modifier_event(&WHEEL_MODIFIER_SEQUENCES, modifiers);
                let delta = ((hook.mouseData >> 16) as u16) as i16 as i64;
                WHEEL_DELTA.fetch_add(delta, Ordering::Relaxed);
            }
            _ => {}
        }
        if handle_gesture_message(message, hook) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
}

fn handle_window_drag_message(message: u32, hook: &MSLLHOOKSTRUCT) -> bool {
    if hook.flags & LLMHF_INJECTED != 0 && hook.dwExtraInfo == super::mouse::GESTURE_INJECT_TAG {
        return false;
    }
    let point = Point {
        x: hook.pt.x,
        y: hook.pt.y,
    };
    let event = mouse_button_event(message, hook.mouseData);
    if let Some((button, false)) = event {
        if SUPPRESSED_WINDOW_DRAG_UPS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                clear_suppressed_release(pending, button)
            })
            .is_ok()
        {
            return true;
        }
    }
    let mut state = match window_drag_state().lock() {
        Ok(state) => state,
        Err(_) => return false,
    };

    if let Some((capture, button)) = state.capture.as_mut() {
        if message == WM_MOUSEMOVE {
            capture.current = point;
            return false;
        }
        if event == Some((*button, false)) {
            capture.current = point;
            capture.finished = true;
            return true;
        }
        return false;
    }

    if !WINDOW_DRAG_ENABLED.load(Ordering::Acquire) {
        return false;
    }
    let Some((button, true)) = event else {
        return false;
    };
    let modifiers = current_modifier_mask();
    let move_match = button == u8_to_mouse_button(WINDOW_DRAG_MOVE_BUTTON.load(Ordering::Acquire))
        && modifiers == WINDOW_DRAG_MOVE_MODIFIERS.load(Ordering::Acquire);
    let resize_match = button
        == u8_to_mouse_button(WINDOW_DRAG_RESIZE_BUTTON.load(Ordering::Acquire))
        && modifiers == WINDOW_DRAG_RESIZE_MODIFIERS.load(Ordering::Acquire);
    let mode = match (move_match, resize_match) {
        (true, _) => WindowDragMode::Move,
        (false, true) => WindowDragMode::Resize,
        _ => return false,
    };
    let sequence = WINDOW_DRAG_SEQUENCE.fetch_add(1, Ordering::AcqRel) + 1;
    state.capture = Some((
        WindowDragCapture {
            sequence,
            mode,
            start: point,
            current: point,
            finished: false,
        },
        button,
    ));
    true
}

fn mouse_button_event(message: u32, mouse_data: u32) -> Option<(MouseButton, bool)> {
    match message {
        WM_LBUTTONDOWN => Some((MouseButton::Left, true)),
        WM_LBUTTONUP => Some((MouseButton::Left, false)),
        WM_RBUTTONDOWN => Some((MouseButton::Right, true)),
        WM_RBUTTONUP => Some((MouseButton::Right, false)),
        WM_MBUTTONDOWN => Some((MouseButton::Middle, true)),
        WM_MBUTTONUP => Some((MouseButton::Middle, false)),
        WM_XBUTTONDOWN | WM_XBUTTONUP => Some((
            if (mouse_data >> 16) as u16 == 1 {
                MouseButton::X1
            } else {
                MouseButton::X2
            },
            message == WM_XBUTTONDOWN,
        )),
        _ => None,
    }
}

fn clear_suppressed_release(pending: u8, button: MouseButton) -> Option<u8> {
    let bit = mouse_button_bit(button);
    (pending & bit != 0).then_some(pending & !bit)
}

fn mouse_button_bit(button: MouseButton) -> u8 {
    1 << mouse_button_to_u8(button)
}

fn mouse_button_to_u8(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Middle => 2,
        MouseButton::X1 => 3,
        MouseButton::X2 => 4,
    }
}

fn u8_to_mouse_button(value: u8) -> MouseButton {
    match value {
        1 => MouseButton::Right,
        2 => MouseButton::Middle,
        3 => MouseButton::X1,
        4 => MouseButton::X2,
        _ => MouseButton::Left,
    }
}

fn handle_gesture_message(message: u32, hook: &MSLLHOOKSTRUCT) -> bool {
    if should_ignore_injected_event(hook.flags, hook.dwExtraInfo) {
        return false;
    }
    let point = Point {
        x: hook.pt.x,
        y: hook.pt.y,
    };
    let event_trigger = trigger_for_message(message, hook.mouseData);
    let active = GESTURE_CAPTURE_ACTIVE.load(Ordering::Acquire);
    let enabled = GESTURE_CAPTURE_ENABLED.load(Ordering::Acquire);
    if !active && (!enabled || message == WM_MOUSEMOVE || event_trigger.is_none()) {
        return false;
    }
    let mut state = match gesture_state().lock() {
        Ok(state) => state,
        Err(_) => return false,
    };

    if message == WM_MOUSEMOVE {
        if state.active_trigger.is_some() {
            let should_push = state.points.last().is_none_or(|last| {
                let dx = last.x - point.x;
                let dy = last.y - point.y;
                dx * dx + dy * dy >= 4
            });
            if should_push && state.points.len() < 512 {
                state.points.push(point);
            }
        }
        return false;
    }

    let Some((trigger, down)) = event_trigger else {
        return false;
    };
    if down {
        if GESTURE_CAPTURE_ENABLED.load(Ordering::Acquire)
            && trigger == u8_to_trigger(GESTURE_TRIGGER.load(Ordering::Acquire))
            && state.active_trigger.is_none()
        {
            state.cancelled = false;
            state.active_trigger = Some(trigger);
            state.active_modifiers = current_modifier_mask();
            GESTURE_CAPTURE_ACTIVE.store(true, Ordering::Release);
            state.points.clear();
            state.points.reserve(128);
            state.points.push(point);
            return true;
        }
        return false;
    }

    if state.active_trigger == Some(trigger) {
        let capture = finish_gesture_capture(&mut state, trigger, point);
        GESTURE_CAPTURE_ACTIVE.store(false, Ordering::Release);
        if let Some(capture) = capture {
            if state.completed.len() >= 8 {
                state.completed.pop_front();
            }
            state.completed.push_back(capture);
        }
        return true;
    }
    false
}

fn finish_gesture_capture(
    state: &mut GestureHookState,
    trigger: GestureTriggerButton,
    point: Point,
) -> Option<GestureCapture> {
    if state.points.last().copied() != Some(point) && state.points.len() < 512 {
        state.points.push(point);
    }
    let points = std::mem::take(&mut state.points);
    let modifiers = state.active_modifiers;
    let cancelled = state.cancelled;
    state.active_trigger = None;
    state.active_modifiers = 0;
    state.cancelled = false;
    (!cancelled).then_some(GestureCapture {
        trigger,
        modifiers,
        points,
    })
}

fn should_ignore_injected_event(flags: u32, extra_info: usize) -> bool {
    if flags & LLMHF_INJECTED == 0 {
        return false;
    }
    if extra_info == super::mouse::GESTURE_INJECT_TAG {
        return true;
    }
    if EXPECTED_HELPER_INJECTED_EVENTS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            count.checked_sub(1)
        })
        .is_ok()
    {
        return true;
    }
    false
}

fn trigger_for_message(message: u32, mouse_data: u32) -> Option<(GestureTriggerButton, bool)> {
    match message {
        WM_RBUTTONDOWN => Some((GestureTriggerButton::Right, true)),
        WM_RBUTTONUP => Some((GestureTriggerButton::Right, false)),
        WM_MBUTTONDOWN => Some((GestureTriggerButton::Middle, true)),
        WM_MBUTTONUP => Some((GestureTriggerButton::Middle, false)),
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            let trigger = if (mouse_data >> 16) as u16 == 1 {
                GestureTriggerButton::X1
            } else {
                GestureTriggerButton::X2
            };
            Some((trigger, message == WM_XBUTTONDOWN))
        }
        _ => None,
    }
}

fn trigger_to_u8(trigger: GestureTriggerButton) -> u8 {
    match trigger {
        GestureTriggerButton::Right => 0,
        GestureTriggerButton::Middle => 1,
        GestureTriggerButton::X1 => 2,
        GestureTriggerButton::X2 => 3,
    }
}

fn u8_to_trigger(value: u8) -> GestureTriggerButton {
    match value {
        1 => GestureTriggerButton::Middle,
        2 => GestureTriggerButton::X1,
        3 => GestureTriggerButton::X2,
        _ => GestureTriggerButton::Right,
    }
}

unsafe fn message_loop() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG};
    let mut message = MSG::default();
    loop {
        let result = GetMessageW(&mut message, HWND::default(), 0, 0).0;
        if result <= 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_gesture_swallows_its_release_without_queuing_a_short_click() {
        let mut state = GestureHookState {
            active_trigger: Some(GestureTriggerButton::Right),
            active_modifiers: 1,
            points: vec![Point { x: 10, y: 20 }],
            completed: VecDeque::new(),
            cancelled: true,
        };

        let capture = finish_gesture_capture(
            &mut state,
            GestureTriggerButton::Right,
            Point { x: 12, y: 22 },
        );

        assert!(capture.is_none());
        assert!(state.active_trigger.is_none());
        assert!(state.points.is_empty());
        assert!(!state.cancelled);
    }

    #[test]
    fn cancelled_window_drag_suppresses_only_the_matching_release() {
        let pending = mouse_button_bit(MouseButton::Left) | mouse_button_bit(MouseButton::X1);
        let after_left = clear_suppressed_release(pending, MouseButton::Left).unwrap();

        assert_eq!(after_left, mouse_button_bit(MouseButton::X1));
        assert_eq!(
            clear_suppressed_release(after_left, MouseButton::Right),
            None
        );
        assert_eq!(
            clear_suppressed_release(after_left, MouseButton::X1),
            Some(0)
        );
    }

    #[test]
    fn all_supported_trigger_buttons_map_to_down_and_up_messages() {
        assert_eq!(
            trigger_for_message(WM_RBUTTONDOWN, 0),
            Some((GestureTriggerButton::Right, true))
        );
        assert_eq!(
            trigger_for_message(WM_MBUTTONUP, 0),
            Some((GestureTriggerButton::Middle, false))
        );
        assert_eq!(
            trigger_for_message(WM_XBUTTONDOWN, 1 << 16),
            Some((GestureTriggerButton::X1, true))
        );
        assert_eq!(
            trigger_for_message(WM_XBUTTONUP, 2 << 16),
            Some((GestureTriggerButton::X2, false))
        );
    }

    #[test]
    fn helper_injected_events_are_always_ignored() {
        assert!(should_ignore_injected_event(
            LLMHF_INJECTED,
            super::super::mouse::GESTURE_INJECT_TAG
        ));
        assert!(!should_ignore_injected_event(0, 0));
    }

    #[test]
    fn driver_injected_events_are_accepted_when_they_are_not_ours() {
        EXPECTED_HELPER_INJECTED_EVENTS.store(0, Ordering::Release);
        assert!(!should_ignore_injected_event(LLMHF_INJECTED, 0));
    }
}
