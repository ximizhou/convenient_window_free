#[path = "input.rs"]
mod input;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod linux;
#[cfg(target_os = "linux")]
#[path = "linux_brightness.rs"]
mod linux_brightness;
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
// Unix 输入层只观察事件、不吞掉原始抬起，因此"放弃一次尚未接管的拖拽"（discard）
// 与"中止一次已经接管的拖拽"（cancel）在这里是同一件事：都只是清空待取的采集。
// Windows 侧刻意把两者分开——那里的 cancel 会额外置 suppress 位以吞掉抬起，
// 而在目标软件里留下一次多余点击正是 discard 必须避免的后果；若将来 Unix 输入层
// 也获得事件吞噬能力，必须参照 Windows 拆成两个独立实现，不能继续复用这个别名。
pub use input::cancel_window_drag_capture as discard_window_drag_capture;

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

pub fn draggable_window_at(
    point: Point,
    paused_apps: &[String],
) -> Result<Option<super::WindowInfo>> {
    Ok(
        backend::draggable_window_at(point)?
            .filter(|window| !is_paused_window(paused_apps, window)),
    )
}

/// 与 `core::engine` 的应用名单语义保持一致：按进程名、窗口标题或窗口类名做小写包含匹配，
/// 命中任一项即视为该窗口属于名单内的应用。行为与 Windows 侧的同名函数对称。
///
/// 注意各平台填入 `WindowInfo` 的程序标识并不统一：Windows 的 `process_name` 取自
/// `QueryFullProcessImageNameW`，形如 `photoshop.exe`；Linux 读取 `/proc/<pid>/comm`，
/// 是不带扩展名的 `photoshop`（且受 15 字符上限截断）；macOS 使用应用名。因此按进程名
/// 配置名单时应写明稳定且尽量完整的标识，避免跨平台或跨版本失效。
pub fn is_paused_window(paused_apps: &[String], window: &super::WindowInfo) -> bool {
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

#[cfg(target_os = "linux")]
pub(crate) use super::linux_audio::adjust_system_volume;
#[cfg(target_os = "macos")]
pub(crate) use super::macos_audio::adjust_system_volume;

#[cfg(target_os = "linux")]
pub(crate) fn adjust_monitor_brightness(
    monitor: &super::Monitor,
    delta: f32,
) -> Result<super::adjustment::Level> {
    linux_brightness::adjust(monitor, delta)
}

#[cfg(target_os = "macos")]
pub(crate) fn adjust_monitor_brightness(
    monitor: &super::Monitor,
    delta: f32,
) -> Result<super::adjustment::Level> {
    super::macos_brightness::adjust(monitor, delta)
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

    fn named_window(process: &str, title: &str, class_name: &str) -> super::super::WindowInfo {
        super::super::WindowInfo {
            handle: WindowHandle(1),
            rect: Rect {
                left: 0,
                top: 0,
                right: 800,
                bottom: 600,
            },
            title: title.to_string(),
            class_name: class_name.to_string(),
            process_name: process.to_string(),
            maximized: false,
            transient: false,
            arranged: false,
            topmost: false,
        }
    }

    /// 空名单必须完全不命中，保证默认路径零影响。
    #[test]
    fn empty_paused_list_never_matches() {
        assert!(!is_paused_window(
            &[],
            &named_window("photoshop", "未命名-1", "Photoshop")
        ));
    }

    /// 与 Windows 侧对称：按进程名、标题、类名分别命中，且忽略大小写与前后空白。
    #[test]
    fn paused_list_matches_process_title_and_class() {
        let by_process = vec!["photoshop".to_string()];
        assert!(is_paused_window(
            &by_process,
            &named_window("photoshop", "未命名-1", "Photoshop")
        ));
        assert!(!is_paused_window(
            &by_process,
            &named_window("gedit", "无标题", "gedit")
        ));

        let by_title = vec!["illustrator".to_string()];
        assert!(is_paused_window(
            &by_title,
            &named_window("wine", "Adobe Illustrator 2024", "SomeClass")
        ));

        let by_class = vec!["blender".to_string()];
        assert!(is_paused_window(
            &by_class,
            &named_window("wine", "无标题", "BlenderWindow")
        ));

        let padded = vec!["  sketchup  ".to_string()];
        assert!(is_paused_window(
            &padded,
            &named_window("SketchUp", "模型", "SketchUp")
        ));
    }

    /// 空项与纯空白项不得命中任何窗口。
    #[test]
    fn blank_paused_entries_never_match() {
        let blank = vec!["   ".to_string(), String::new()];
        assert!(!is_paused_window(
            &blank,
            &named_window("SketchUp", "模型", "SketchUp")
        ));
    }

    /// 名单项走的是子串包含匹配：短项会连带命中名字里包含它的其他程序。
    /// 这里把上游既有的宽松语义固定下来，同时提醒按进程名配置时应给完整标识。
    #[test]
    fn paused_matching_is_substring_and_may_overmatch() {
        let configured = vec!["code".to_string()];
        assert!(is_paused_window(
            &configured,
            &named_window("code", "项目", "Code")
        ));
        assert!(is_paused_window(
            &configured,
            &named_window("codeblocks", "无标题", "codeblocks")
        ));
    }
}
