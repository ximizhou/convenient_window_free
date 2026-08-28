#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(not(target_os = "windows"))]
pub mod unix;

#[cfg(target_os = "windows")]
pub use self::windows::*;

#[cfg(not(target_os = "windows"))]
pub use self::unix::*;

use crate::config::AppConfig;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCapabilities {
    pub global_input: bool,
    pub window_control: bool,
    pub window_topmost: bool,
    pub screen_capture: bool,
    pub ocr: bool,
    pub audio: bool,
    pub system_actions: bool,
    pub edge_hide: bool,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub system: &'static str,
    pub architecture: &'static str,
    pub session: Option<String>,
    pub capabilities: PlatformCapabilities,
}

pub fn platform_info() -> PlatformInfo {
    PlatformInfo {
        system: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        session: session_name(),
        capabilities: platform_capabilities(),
    }
}

pub fn preflight() -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let input_ready = unix::preflight_input()?;
        let capabilities = platform_capabilities();
        if !input_ready || !capabilities.global_input {
            anyhow::bail!(unix::unsupported_message("globalInput"));
        }
        if !capabilities.window_control {
            anyhow::bail!(unix::unsupported_message("windowControl"));
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn session_name() -> Option<String> {
    Some("windows-desktop".to_string())
}

#[cfg(not(target_os = "windows"))]
fn session_name() -> Option<String> {
    std::env::var("XDG_SESSION_TYPE")
        .ok()
        .or_else(|| {
            std::env::var("WAYLAND_DISPLAY")
                .ok()
                .map(|_| "wayland".to_string())
        })
        .or_else(|| std::env::var("DISPLAY").ok().map(|_| "x11".to_string()))
}

#[cfg(target_os = "windows")]
fn platform_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        global_input: true,
        window_control: true,
        window_topmost: true,
        screen_capture: true,
        ocr: true,
        audio: true,
        system_actions: true,
        edge_hide: true,
    }
}

#[cfg(not(target_os = "windows"))]
fn platform_capabilities() -> PlatformCapabilities {
    unix::capabilities()
}

pub fn apply_capability_limits(config: &mut AppConfig) -> bool {
    apply_capability_limits_for(config, &platform_capabilities())
}

fn apply_capability_limits_for(
    config: &mut AppConfig,
    capabilities: &PlatformCapabilities,
) -> bool {
    let before_edge_hide = config.edge_hide.enabled;
    let before_topmost_pin = config.topmost_pin.enabled;
    if !capabilities.edge_hide {
        config.edge_hide.enabled = false;
    }
    if !capabilities.window_topmost {
        config.topmost_pin.enabled = false;
    }
    before_edge_hide != config.edge_hide.enabled || before_topmost_pin != config.topmost_pin.enabled
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub fn inflate(&self, amount: i32) -> Self {
        Self {
            left: self.left - amount,
            top: self.top - amount,
            right: self.right + amount,
            bottom: self.bottom + amount,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Monitor {
    pub bounds: Rect,
    pub work_area: Rect,
    pub primary: bool,
    pub device_id: [u16; 128],
}

impl Monitor {
    pub fn id(&self) -> String {
        let length = self
            .device_id
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(self.device_id.len());
        if length == 0 {
            return self.legacy_id();
        }
        format!(
            "monitor:{}",
            String::from_utf16_lossy(&self.device_id[..length]).to_ascii_lowercase()
        )
    }

    pub fn legacy_id(&self) -> String {
        format!(
            "display:{}:{}:{}:{}",
            self.bounds.left, self.bounds.top, self.bounds.right, self.bounds.bottom
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WindowHandle(pub isize);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowInfo {
    pub handle: WindowHandle,
    pub rect: Rect,
    pub title: String,
    pub class_name: String,
    pub process_name: String,
    pub maximized: bool,
    pub transient: bool,
    pub arranged: bool,
    pub topmost: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InputState {
    pub left_down: bool,
    pub right_down: bool,
    pub middle_down: bool,
    pub escape_down: bool,
    pub enter_down: bool,
    pub modifiers: u8,
    pub left_click_count: u64,
    pub right_click_count: u64,
    pub wheel_delta: i64,
    pub left_click_modifier_sequences: [u64; 16],
    pub right_click_modifier_sequences: [u64; 16],
    pub wheel_modifier_sequences: [u64; 16],
}

#[derive(Clone, Debug)]
pub struct GestureCapture {
    pub trigger: crate::config::GestureTriggerButton,
    pub modifiers: u8,
    pub points: Vec<Point>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowDragMode {
    Move,
    Resize,
}

#[derive(Clone, Copy, Debug)]
pub struct WindowDragCapture {
    pub sequence: u64,
    pub mode: WindowDragMode,
    pub start: Point,
    pub current: Point,
    pub finished: bool,
}

#[cfg(test)]
mod monitor_tests {
    use super::*;

    #[test]
    fn stable_monitor_id_does_not_change_with_geometry() {
        let mut device_id = [0; 128];
        for (target, value) in device_id.iter_mut().zip("DISPLAY#ACME123".encode_utf16()) {
            *target = value;
        }
        let first = Monitor {
            bounds: Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            work_area: Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            },
            primary: true,
            device_id,
        };
        let resized = Monitor {
            bounds: Rect {
                left: -2560,
                top: 0,
                right: 0,
                bottom: 1440,
            },
            work_area: Rect {
                left: -2560,
                top: 0,
                right: 0,
                bottom: 1400,
            },
            ..first
        };

        assert_eq!(first.id(), resized.id());
        assert_ne!(first.legacy_id(), resized.legacy_id());
    }

    #[test]
    fn monitor_without_a_device_path_falls_back_to_geometry() {
        let monitor = Monitor {
            bounds: Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            work_area: Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            },
            primary: true,
            device_id: [0; 128],
        };
        assert_eq!(monitor.id(), "display:0:0:1920:1080");
    }
}

impl InputState {
    pub fn left_clicked_since(&self, previous: Self) -> bool {
        self.left_click_count != previous.left_click_count
            || (self.left_down && !previous.left_down)
    }

    pub fn right_clicked_since(&self, previous: Self) -> bool {
        self.right_click_count != previous.right_click_count
            || (self.right_down && !previous.right_down)
    }

    pub fn context_menu_dismissed_since(&self, previous: Self) -> bool {
        self.left_clicked_since(previous)
            || (self.escape_down && !previous.escape_down)
            || (self.enter_down && !previous.enter_down)
    }

    pub fn wheel_up_since(&self, previous: Self) -> bool {
        self.wheel_delta > previous.wheel_delta
    }

    pub fn wheel_down_since(&self, previous: Self) -> bool {
        self.wheel_delta < previous.wheel_delta
    }

    pub fn left_click_modifier_mask_since(&self, previous: Self) -> Option<u8> {
        latest_modifier_mask(
            &self.left_click_modifier_sequences,
            &previous.left_click_modifier_sequences,
        )
        .or_else(|| (self.left_down && !previous.left_down).then_some(self.modifiers))
    }

    pub fn right_click_modifier_mask_since(&self, previous: Self) -> Option<u8> {
        latest_modifier_mask(
            &self.right_click_modifier_sequences,
            &previous.right_click_modifier_sequences,
        )
        .or_else(|| (self.right_down && !previous.right_down).then_some(self.modifiers))
    }

    pub fn wheel_modifier_mask_since(&self, previous: Self) -> Option<u8> {
        latest_modifier_mask(
            &self.wheel_modifier_sequences,
            &previous.wheel_modifier_sequences,
        )
    }
}

fn latest_modifier_mask(current: &[u64; 16], previous: &[u64; 16]) -> Option<u8> {
    current
        .iter()
        .zip(previous)
        .enumerate()
        .filter(|(_, (current, previous))| current != previous)
        .max_by_key(|(_, (current, _))| *current)
        .map(|(mask, _)| mask as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_mouse_clicks_are_detected_between_poll_samples() {
        let previous = InputState {
            left_click_count: 10,
            right_click_count: 20,
            ..Default::default()
        };
        let current = InputState {
            left_click_count: 11,
            right_click_count: 21,
            ..Default::default()
        };

        assert!(current.left_clicked_since(previous));
        assert!(current.right_clicked_since(previous));
        assert!(current.context_menu_dismissed_since(previous));
    }

    #[test]
    fn latest_modifier_event_wins_when_combinations_change_between_polls() {
        let mut previous = InputState::default();
        previous.left_click_modifier_sequences[1] = 8;
        let mut current = previous;
        current.left_click_count += 2;
        current.left_click_modifier_sequences[1] = 9;
        current.left_click_modifier_sequences[6] = 10;

        assert_eq!(current.left_click_modifier_mask_since(previous), Some(6));
    }

    #[test]
    fn platform_contract_reports_system_architecture_and_capabilities() {
        let info = platform_info();
        assert_eq!(info.system, std::env::consts::OS);
        assert_eq!(info.architecture, std::env::consts::ARCH);
        let value = serde_json::to_value(info).unwrap();
        assert!(value["capabilities"]["globalInput"].is_boolean());
        assert!(value["capabilities"]["windowControl"].is_boolean());
        assert!(value["capabilities"]["windowTopmost"].is_boolean());
        assert!(value["capabilities"]["screenCapture"].is_boolean());
        assert!(value["capabilities"]["ocr"].is_boolean());
        assert!(value["capabilities"]["audio"].is_boolean());
        assert!(value["capabilities"]["systemActions"].is_boolean());
        assert!(value["capabilities"]["edgeHide"].is_boolean());
    }

    #[test]
    fn unavailable_capabilities_disable_runtime_only_features() {
        let mut config = AppConfig::default();
        config.edge_hide.enabled = true;
        config.topmost_pin.enabled = true;
        let capabilities = PlatformCapabilities {
            global_input: true,
            window_control: true,
            window_topmost: false,
            screen_capture: true,
            ocr: false,
            audio: false,
            system_actions: false,
            edge_hide: false,
        };

        assert!(apply_capability_limits_for(&mut config, &capabilities));
        assert!(!config.edge_hide.enabled);
        assert!(!config.topmost_pin.enabled);
    }
}
