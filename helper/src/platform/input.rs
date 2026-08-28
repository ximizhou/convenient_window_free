use crate::config::{GestureTriggerButton, MouseButton};
use crate::platform::{GestureCapture, InputState, Point, WindowDragCapture, WindowDragMode};
use anyhow::{bail, Result};
use rdev::{listen, simulate, Button, Event, EventType, Key, SimulateError};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

const HOOK_NOT_STARTED: u8 = 0;
const HOOK_STARTING: u8 = 1;
const HOOK_HEALTHY: u8 = 2;
const HOOK_FAILED: u8 = 3;
const HOOK_STOPPING: u8 = 4;
const HOOK_STOPPED: u8 = 5;

static HOOK_STATE: AtomicU8 = AtomicU8::new(HOOK_NOT_STARTED);
static EXPECTED_INJECTED_EVENTS: AtomicU8 = AtomicU8::new(0);
static DRAG_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static GESTURE_ENABLED: AtomicBool = AtomicBool::new(false);
static DRAG_ENABLED: AtomicBool = AtomicBool::new(false);
static GESTURE_TRIGGER: AtomicU8 = AtomicU8::new(0);
static MOVE_BUTTON: AtomicU8 = AtomicU8::new(0);
static RESIZE_BUTTON: AtomicU8 = AtomicU8::new(1);
static MOVE_MODIFIERS: AtomicU8 = AtomicU8::new(2);
static RESIZE_MODIFIERS: AtomicU8 = AtomicU8::new(2);

#[derive(Default)]
struct InputStore {
    state: InputState,
    cursor: Point,
    modifiers: u8,
    gesture: Option<GestureInProgress>,
    completed_gestures: VecDeque<GestureCapture>,
    drag: Option<(WindowDragCapture, MouseButton)>,
}

#[derive(Default)]
struct GestureInProgress {
    trigger: GestureTriggerButton,
    modifiers: u8,
    points: Vec<Point>,
}

fn store() -> &'static Mutex<InputStore> {
    static STORE: OnceLock<Mutex<InputStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(InputStore::default()))
}

pub fn install_mouse_hook() -> Result<()> {
    if !super::backend::preflight_input()? {
        bail!(super::unsupported_message("globalInput"));
    }
    match HOOK_STATE.compare_exchange(
        HOOK_NOT_STARTED,
        HOOK_STARTING,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => {}
        Err(HOOK_HEALTHY) => return Ok(()),
        Err(state) => bail!("global input listener cannot start from state {state}"),
    }

    std::thread::Builder::new()
        .name("magic-corners-global-input".to_string())
        .spawn(|| {
            HOOK_STATE.store(HOOK_HEALTHY, Ordering::Release);
            let result = listen(handle_event);
            let stopping = HOOK_STATE.load(Ordering::Acquire) == HOOK_STOPPING;
            HOOK_STATE.store(
                if stopping || result.is_ok() {
                    HOOK_STOPPED
                } else {
                    HOOK_FAILED
                },
                Ordering::Release,
            );
            if let Err(error) = result {
                crate::logging::write_line(format!("input: global listener failed: {error:?}"));
            }
        })
        .map_err(anyhow::Error::from)?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        match HOOK_STATE.load(Ordering::Acquire) {
            HOOK_HEALTHY => return Ok(()),
            HOOK_FAILED | HOOK_STOPPED => bail!(super::unsupported_message("globalInput")),
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    bail!("global input listener did not become healthy in time")
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
        .is_ok()
    {
        cancel_window_drag_capture();
        cancel_gesture_capture();
    }
}

pub fn configure_gesture_capture(enabled: bool, trigger: GestureTriggerButton) {
    GESTURE_TRIGGER.store(trigger_to_u8(trigger), Ordering::Release);
    GESTURE_ENABLED.store(enabled, Ordering::Release);
    if !enabled {
        cancel_gesture_capture();
    }
}

pub fn configure_window_drag_capture(
    enabled: bool,
    move_button: MouseButton,
    move_modifiers: u8,
    resize_button: MouseButton,
    resize_modifiers: u8,
) {
    MOVE_BUTTON.store(mouse_button_to_u8(move_button), Ordering::Release);
    RESIZE_BUTTON.store(mouse_button_to_u8(resize_button), Ordering::Release);
    MOVE_MODIFIERS.store(move_modifiers, Ordering::Release);
    RESIZE_MODIFIERS.store(resize_modifiers, Ordering::Release);
    DRAG_ENABLED.store(enabled, Ordering::Release);
    if !enabled {
        cancel_window_drag_capture();
    }
}

pub fn take_window_drag_capture() -> Option<WindowDragCapture> {
    let mut state = store().lock().ok()?;
    let capture = state.drag.map(|(capture, _)| capture)?;
    if capture.finished {
        state.drag = None;
    }
    Some(capture)
}

pub fn cancel_window_drag_capture() {
    if let Ok(mut state) = store().lock() {
        state.drag = None;
    }
}

pub fn take_gesture_capture() -> Option<GestureCapture> {
    store().lock().ok()?.completed_gestures.pop_front()
}

pub fn active_gesture_points() -> Vec<Point> {
    store()
        .lock()
        .ok()
        .and_then(|state| state.gesture.as_ref().map(|gesture| gesture.points.clone()))
        .unwrap_or_default()
}

pub fn mark_helper_injected_events(count: u8) {
    EXPECTED_INJECTED_EVENTS.store(count, Ordering::Release);
}

pub fn input_state() -> InputState {
    store().lock().map(|state| state.state).unwrap_or_default()
}

pub fn send_trigger_click(trigger: GestureTriggerButton) -> Result<()> {
    // rdev's listen API observes events but cannot consume them. The original
    // trigger click already reaches the application, so injecting it again would
    // duplicate the click on Unix hosts.
    let _ = trigger;
    Ok(())
}

pub fn send_shortcut(shortcut: &str, routing_modifiers: u8) -> Result<()> {
    let keys = parse_shortcut(shortcut)?;
    let routing = active_routing_keys(routing_modifiers);
    let mut events = Vec::with_capacity((keys.len() + routing.len()) * 2);
    events.extend(routing.iter().rev().copied().map(EventType::KeyRelease));
    events.extend(keys.iter().copied().map(EventType::KeyPress));
    events.extend(keys.iter().rev().copied().map(EventType::KeyRelease));
    events.extend(routing.iter().copied().map(EventType::KeyPress));
    mark_helper_injected_events(events.len().min(u8::MAX as usize) as u8);
    for event in events {
        simulate(&event).map_err(|error| anyhow::anyhow!("shortcut injection failed: {error}"))?;
        std::thread::sleep(std::time::Duration::from_millis(4));
    }
    Ok(())
}

fn handle_event(event: Event) {
    if EXPECTED_INJECTED_EVENTS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            count.checked_sub(1)
        })
        .is_ok()
    {
        return;
    }
    let Ok(mut state) = store().lock() else {
        return;
    };
    match event.event_type {
        EventType::MouseMove { x, y } => {
            if x.is_finite() && y.is_finite() {
                let cursor = Point {
                    x: x.round() as i32,
                    y: y.round() as i32,
                };
                state.cursor = cursor;
                if let Some((capture, _)) = state.drag.as_mut() {
                    capture.current = cursor;
                }
                if let Some(gesture) = state.gesture.as_mut() {
                    let should_push = gesture.points.last().is_none_or(|last| {
                        let dx = last.x - cursor.x;
                        let dy = last.y - cursor.y;
                        dx * dx + dy * dy >= 4
                    });
                    if should_push && gesture.points.len() < 512 {
                        gesture.points.push(cursor);
                    }
                }
            }
        }
        EventType::ButtonPress(button) => handle_button(&mut state, button, true),
        EventType::ButtonRelease(button) => handle_button(&mut state, button, false),
        EventType::Wheel { delta_y, .. } => {
            let modifiers = state.modifiers;
            state.state.wheel_delta = state
                .state
                .wheel_delta
                .saturating_add(delta_y.saturating_mul(120));
            record_modifier_event(&mut state.state.wheel_modifier_sequences, modifiers);
        }
        EventType::KeyPress(key) => update_key(&mut state, key, true),
        EventType::KeyRelease(key) => update_key(&mut state, key, false),
    }
}

fn handle_button(state: &mut InputStore, button: Button, down: bool) {
    let Some(button) = map_button(button) else {
        return;
    };
    match button {
        MouseButton::Left => {
            state.state.left_down = down;
            if down {
                state.state.left_click_count = state.state.left_click_count.saturating_add(1);
                record_modifier_event(
                    &mut state.state.left_click_modifier_sequences,
                    state.modifiers,
                );
            }
        }
        MouseButton::Right => {
            state.state.right_down = down;
            if down {
                state.state.right_click_count = state.state.right_click_count.saturating_add(1);
                record_modifier_event(
                    &mut state.state.right_click_modifier_sequences,
                    state.modifiers,
                );
            }
        }
        MouseButton::Middle => state.state.middle_down = down,
        MouseButton::X1 | MouseButton::X2 => return,
    }

    if handle_drag_event(state, button, down) {
        return;
    }
    let Some(trigger) = gesture_trigger(button) else {
        return;
    };
    if down {
        if GESTURE_ENABLED.load(Ordering::Acquire)
            && trigger_to_u8(trigger) == GESTURE_TRIGGER.load(Ordering::Acquire)
            && state.gesture.is_none()
        {
            state.gesture = Some(GestureInProgress {
                trigger,
                modifiers: state.modifiers,
                points: vec![state.cursor],
            });
        }
    } else if let Some(gesture) = state.gesture.take() {
        if gesture.trigger == trigger {
            if state.completed_gestures.len() >= 8 {
                state.completed_gestures.pop_front();
            }
            state.completed_gestures.push_back(GestureCapture {
                trigger,
                modifiers: gesture.modifiers,
                points: gesture.points,
            });
        } else {
            state.gesture = Some(gesture);
        }
    }
}

fn handle_drag_event(state: &mut InputStore, button: MouseButton, down: bool) -> bool {
    if let Some((capture, captured_button)) = state.drag.as_mut() {
        if !down && *captured_button == button {
            capture.current = state.cursor;
            capture.finished = true;
            return true;
        }
        return true;
    }
    if !DRAG_ENABLED.load(Ordering::Acquire) || !down {
        return false;
    }
    let move_match = button as u8 == MOVE_BUTTON.load(Ordering::Acquire)
        && state.modifiers == MOVE_MODIFIERS.load(Ordering::Acquire);
    let resize_match = button as u8 == RESIZE_BUTTON.load(Ordering::Acquire)
        && state.modifiers == RESIZE_MODIFIERS.load(Ordering::Acquire);
    let mode = match (move_match, resize_match) {
        (true, _) => WindowDragMode::Move,
        (false, true) => WindowDragMode::Resize,
        _ => return false,
    };
    let sequence = DRAG_SEQUENCE.fetch_add(1, Ordering::AcqRel) + 1;
    state.drag = Some((
        WindowDragCapture {
            sequence,
            mode,
            start: state.cursor,
            current: state.cursor,
            finished: false,
        },
        button,
    ));
    true
}

fn update_key(state: &mut InputStore, key: Key, down: bool) {
    match key {
        Key::Escape => state.state.escape_down = down,
        Key::Return | Key::KpReturn => state.state.enter_down = down,
        Key::ControlLeft | Key::ControlRight => set_modifier(&mut state.modifiers, 1, down),
        Key::Alt | Key::AltGr => set_modifier(&mut state.modifiers, 2, down),
        Key::ShiftLeft | Key::ShiftRight => set_modifier(&mut state.modifiers, 4, down),
        Key::MetaLeft | Key::MetaRight => set_modifier(&mut state.modifiers, 8, down),
        _ => {}
    }
    state.state.modifiers = state.modifiers;
}

fn set_modifier(mask: &mut u8, bit: u8, down: bool) {
    if down {
        *mask |= bit;
    } else {
        *mask &= !bit;
    }
}

fn record_modifier_event(sequence: &mut [u64; 16], modifiers: u8) {
    let next = sequence
        .iter()
        .copied()
        .max()
        .unwrap_or_default()
        .saturating_add(1);
    sequence[usize::from(modifiers & 0x0f)] = next;
}

fn cancel_gesture_capture() {
    if let Ok(mut state) = store().lock() {
        state.gesture = None;
    }
}

fn map_button(button: Button) -> Option<MouseButton> {
    match button {
        Button::Left => Some(MouseButton::Left),
        Button::Right => Some(MouseButton::Right),
        Button::Middle => Some(MouseButton::Middle),
        Button::Unknown(value) => match value {
            8 => Some(MouseButton::X1),
            9 => Some(MouseButton::X2),
            _ => None,
        },
    }
}

fn gesture_trigger(button: MouseButton) -> Option<GestureTriggerButton> {
    match button {
        MouseButton::Right => Some(GestureTriggerButton::Right),
        MouseButton::Middle => Some(GestureTriggerButton::Middle),
        MouseButton::X1 => Some(GestureTriggerButton::X1),
        MouseButton::X2 => Some(GestureTriggerButton::X2),
        MouseButton::Left => None,
    }
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

fn trigger_to_u8(trigger: GestureTriggerButton) -> u8 {
    match trigger {
        GestureTriggerButton::Right => 0,
        GestureTriggerButton::Middle => 1,
        GestureTriggerButton::X1 => 2,
        GestureTriggerButton::X2 => 3,
    }
}

fn active_routing_keys(mask: u8) -> Vec<Key> {
    let mut keys = Vec::with_capacity(4);
    if mask & 1 != 0 {
        keys.push(Key::ControlLeft);
    }
    if mask & 2 != 0 {
        keys.push(Key::Alt);
    }
    if mask & 4 != 0 {
        keys.push(Key::ShiftLeft);
    }
    if mask & 8 != 0 {
        keys.push(Key::MetaLeft);
    }
    keys
}

fn parse_shortcut(shortcut: &str) -> Result<Vec<Key>> {
    let mut keys = Vec::new();
    for part in shortcut
        .split('+')
        .map(|item| item.trim().to_ascii_lowercase())
    {
        if part.is_empty() {
            continue;
        }
        let key =
            parse_key(&part).ok_or_else(|| anyhow::anyhow!("unsupported shortcut key: {part}"))?;
        keys.push(key);
    }
    if keys.is_empty() {
        bail!("shortcut is empty");
    }
    Ok(keys)
}

fn parse_key(part: &str) -> Option<Key> {
    Some(match part {
        "ctrl" | "control" => Key::ControlLeft,
        "shift" => Key::ShiftLeft,
        "alt" | "option" => Key::Alt,
        "win" | "meta" | "command" | "cmd" => Key::MetaLeft,
        "enter" | "return" => Key::Return,
        "esc" | "escape" => Key::Escape,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        _ => parse_ascii_key(part)?,
    })
}

fn parse_ascii_key(part: &str) -> Option<Key> {
    let value = part.as_bytes();
    if value.len() != 1 {
        return None;
    }
    Some(match value[0].to_ascii_uppercase() {
        b'A' => Key::KeyA,
        b'B' => Key::KeyB,
        b'C' => Key::KeyC,
        b'D' => Key::KeyD,
        b'E' => Key::KeyE,
        b'F' => Key::KeyF,
        b'G' => Key::KeyG,
        b'H' => Key::KeyH,
        b'I' => Key::KeyI,
        b'J' => Key::KeyJ,
        b'K' => Key::KeyK,
        b'L' => Key::KeyL,
        b'M' => Key::KeyM,
        b'N' => Key::KeyN,
        b'O' => Key::KeyO,
        b'P' => Key::KeyP,
        b'Q' => Key::KeyQ,
        b'R' => Key::KeyR,
        b'S' => Key::KeyS,
        b'T' => Key::KeyT,
        b'U' => Key::KeyU,
        b'V' => Key::KeyV,
        b'W' => Key::KeyW,
        b'X' => Key::KeyX,
        b'Y' => Key::KeyY,
        b'Z' => Key::KeyZ,
        b'0' => Key::Num0,
        b'1' => Key::Num1,
        b'2' => Key::Num2,
        b'3' => Key::Num3,
        b'4' => Key::Num4,
        b'5' => Key::Num5,
        b'6' => Key::Num6,
        b'7' => Key::Num7,
        b'8' => Key::Num8,
        b'9' => Key::Num9,
        _ => return None,
    })
}

#[allow(dead_code)]
fn simulate_error_text(error: SimulateError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_shortcuts_without_windows_api() {
        assert_eq!(parse_shortcut("Ctrl+Alt+T").unwrap().len(), 3);
        assert_eq!(parse_shortcut("Meta+D").unwrap().len(), 2);
        assert!(parse_shortcut("Ctrl+Nope").is_err());
    }

    #[test]
    fn modifier_sequences_are_monotonic_per_mask() {
        let mut sequence = [0_u64; 16];
        record_modifier_event(&mut sequence, 1);
        record_modifier_event(&mut sequence, 6);
        assert_eq!(sequence[1], 1);
        assert_eq!(sequence[6], 2);
    }
}
