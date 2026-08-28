#[path = "input.rs"]
mod input;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod linux;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod macos;

use crate::config::{OcrConfig, ScreenshotResultMode};
use crate::platform::{Point, Rect, WindowHandle};
use anyhow::{bail, Result};
use std::sync::{Mutex, OnceLock};

pub use input::{
    active_gesture_points, cancel_window_drag_capture, configure_gesture_capture,
    configure_window_drag_capture, input_state, install_mouse_hook, mouse_hook_is_healthy,
    send_trigger_click, stop_mouse_hook, take_gesture_capture, take_window_drag_capture,
};

#[cfg(target_os = "linux")]
use linux as backend;
#[cfg(target_os = "macos")]
use macos as backend;

pub fn preflight_input() -> Result<bool> {
    backend::preflight_input()
}

pub fn capabilities() -> super::PlatformCapabilities {
    backend::capabilities()
}

pub fn cursor_position() -> Result<Point> {
    backend::cursor_position()
}

pub fn monitors() -> Result<Vec<super::Monitor>> {
    backend::monitors()
}

pub fn foreground_window() -> Result<Option<super::WindowInfo>> {
    backend::foreground_window()
}

pub fn window_exists(handle: WindowHandle) -> bool {
    backend::window_exists(handle)
}

pub fn window_is_minimized(handle: WindowHandle) -> bool {
    backend::window_is_minimized(handle)
}

pub fn window_info_for_handle(handle: WindowHandle) -> Result<Option<super::WindowInfo>> {
    backend::window_info_for_handle(handle)
}

pub fn draggable_window_at(point: Point) -> Result<Option<super::WindowInfo>> {
    backend::draggable_window_at(point)
}

pub fn set_window_rect(handle: WindowHandle, rect: Rect) -> Result<()> {
    backend::set_window_rect(handle, rect)
}

pub fn toggle_window_topmost_at(point: Option<Point>) -> Result<(String, bool)> {
    backend::toggle_window_topmost_at(point)
}

pub fn set_window_topmost(handle: WindowHandle, topmost: bool) -> Result<()> {
    backend::set_window_topmost(handle, topmost)
}

pub fn set_window_rect_topmost(handle: WindowHandle, rect: Rect, topmost: bool) -> Result<()> {
    backend::set_window_rect_topmost(handle, rect, topmost)
}

pub fn lock_screen() -> Result<()> {
    backend::lock_screen()
}

pub fn adjust_volume(_delta: f32) -> Result<()> {
    bail!(unsupported_message("audio"))
}

pub fn send_shortcut_with_modifiers(shortcut: &str, routing_modifiers: u8) -> Result<()> {
    input::send_shortcut(shortcut, routing_modifiers)
}

pub fn show_desktop_with_modifiers(routing_modifiers: u8) -> Result<()> {
    backend::show_desktop_with_modifiers(routing_modifiers)
}

pub fn configure_topmost_pins(_enabled: bool) {}
pub fn set_topmost_pin_target(_handle: WindowHandle, _topmost: bool) {}
pub fn clear_topmost_pins() {}
pub fn update_hotzone_hints(_rect: Option<Rect>) {}
pub fn update_edge_hide_preview(_rect: Option<Rect>) {}
pub fn update_strip_hints(_strips: &[Rect]) {}
pub fn hide_hotzone_hints() {}
pub fn update_gesture_overlay(_points: &[Point], _label: Option<&str>) {}
pub fn hide_gesture_overlay() {}
pub fn hide_gesture_overlay_before_capture() {}
pub fn show_ocr_toast(_owner: isize, _text: String, _success: bool) {}

#[derive(Clone, Debug)]
pub enum OcrCompletion {
    Copied(usize),
    Failed(String),
}

pub fn take_ocr_completion() -> Option<OcrCompletion> {
    None
}

pub fn ocr_worker_error() -> Option<String> {
    None
}

pub fn available_ocr_languages() -> &'static [String] {
    &[]
}

pub fn capture_and_pin(rect: Rect, end_point: Option<Point>, ocr: &OcrConfig) -> Result<()> {
    if matches!(
        ocr.screenshot_result,
        ScreenshotResultMode::CopyText | ScreenshotResultMode::PinAndCopy
    ) {
        bail!(unsupported_message("ocr"));
    }
    let path = backend::capture_and_save(rect, end_point, ocr)?;
    if let Ok(mut slot) = last_capture().lock() {
        *slot = Some(path);
    }
    Ok(())
}

pub fn take_capture_result() -> Option<String> {
    last_capture().lock().ok()?.take()
}

fn last_capture() -> &'static Mutex<Option<String>> {
    static CAPTURE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    CAPTURE.get_or_init(|| Mutex::new(None))
}

pub(crate) fn unsupported_message(capability: &str) -> String {
    let info = super::platform_info();
    let session = info.session.as_deref().unwrap_or("unknown");
    format!(
        "unsupported: {capability} on {} {} session {session}",
        info.system, info.architecture
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_requests_are_rejected_instead_of_returning_a_fake_result() {
        let mut config = OcrConfig::default();
        config.screenshot_result = ScreenshotResultMode::CopyText;
        let error = capture_and_pin(
            Rect {
                left: 0,
                top: 0,
                right: 8,
                bottom: 8,
            },
            None,
            &config,
        )
        .expect_err("Unix OCR must be explicitly unsupported");
        assert!(error.to_string().starts_with("unsupported: ocr on "));
    }
}
