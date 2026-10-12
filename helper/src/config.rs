use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 9;

const MAX_GESTURE_TEMPLATES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default)]
    pub schema_version: u32,
    /// Connection-local translated labels; excluded from persisted settings.
    #[serde(skip)]
    pub gesture_labels: std::collections::HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub hotzones_enabled: bool,
    /// Only controls the hover hint, not hotzone detection or actions.
    #[serde(default = "default_true")]
    pub show_hotzone_hint: bool,
    #[serde(default = "default_edge_size")]
    pub edge_size: i32,
    #[serde(default = "default_hover_delay_ms")]
    pub hover_delay_ms: u64,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_action_cooldown_ms")]
    pub action_cooldown_ms: u64,
    #[serde(default)]
    pub paused_apps: Vec<String>,
    #[serde(default = "default_hotzones")]
    pub hotzones: Vec<HotzoneSetting>,
    #[serde(default)]
    pub monitor_profiles: Vec<MonitorProfile>,
    #[serde(default)]
    pub edge_hide: EdgeHideConfig,
    #[serde(default)]
    pub mouse_gestures: MouseGestureConfig,
    #[serde(default)]
    pub window_drag: WindowDragConfig,
    #[serde(default)]
    pub topmost_pin: TopmostPinConfig,
    #[serde(default)]
    pub ocr: OcrConfig,
    #[serde(default)]
    pub taskbar_appearance: TaskbarAppearanceConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            gesture_labels: Default::default(),
            enabled: true,
            hotzones_enabled: true,
            show_hotzone_hint: true,
            edge_size: 8,
            hover_delay_ms: 350,
            poll_interval_ms: 33,
            action_cooldown_ms: 700,
            paused_apps: Vec::new(),
            hotzones: default_hotzones(),
            monitor_profiles: Vec::new(),
            edge_hide: EdgeHideConfig::default(),
            mouse_gestures: MouseGestureConfig::default(),
            window_drag: WindowDragConfig::default(),
            topmost_pin: TopmostPinConfig::default(),
            ocr: OcrConfig::default(),
            taskbar_appearance: TaskbarAppearanceConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskbarAppearanceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_taskbar_mode")]
    pub mode: String,
    #[serde(default = "default_taskbar_opacity")]
    pub opacity: u32,
    #[serde(default = "default_taskbar_tint")]
    pub tint: String,
    #[serde(default)]
    pub show_border: bool,
}

fn default_taskbar_mode() -> String {
    "transparent".into()
}
fn default_taskbar_opacity() -> u32 {
    58
}
fn default_taskbar_tint() -> String {
    "#233A63".into()
}

impl Default for TaskbarAppearanceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: default_taskbar_mode(),
            opacity: default_taskbar_opacity(),
            tint: default_taskbar_tint(),
            show_border: false,
        }
    }
}

impl AppConfig {
    pub fn gesture_label<'a>(&'a self, gesture: &'a GestureTemplate) -> Option<&'a str> {
        if !gesture.name.is_empty() {
            Some(&gesture.name)
        } else {
            self.gesture_labels.get(&gesture.id).map(String::as_str)
        }
    }

    pub fn normalized(mut self) -> Self {
        let source_schema_version = self.schema_version;
        self.schema_version = default_schema_version();
        if source_schema_version < 6 {
            self.hotzones_enabled = self.hotzones.iter().any(|zone| zone.enabled)
                || self
                    .monitor_profiles
                    .iter()
                    .flat_map(|profile| &profile.hotzones)
                    .any(|zone| zone.enabled);
        }
        self.edge_size = self.edge_size.clamp(2, 48);
        self.hover_delay_ms = self.hover_delay_ms.clamp(0, 3000);
        self.poll_interval_ms = self.poll_interval_ms.clamp(10, 250);
        self.action_cooldown_ms = self.action_cooldown_ms.clamp(10, 5000);
        self.paused_apps = normalized_string_list(self.paused_apps);
        if !matches!(
            self.taskbar_appearance.mode.as_str(),
            "transparent" | "acrylic" | "solid"
        ) {
            self.taskbar_appearance.mode = default_taskbar_mode();
        }
        self.taskbar_appearance.opacity = self.taskbar_appearance.opacity.clamp(0, 100);
        let taskbar_tint = self.taskbar_appearance.tint.trim();
        if !is_hex_color(taskbar_tint) {
            self.taskbar_appearance.tint = default_taskbar_tint();
        } else {
            self.taskbar_appearance.tint = taskbar_tint.to_uppercase();
        }

        self.edge_hide.strip_size = self.edge_hide.strip_size.clamp(4, 64);
        self.edge_hide.animation_duration_ms = self.edge_hide.animation_duration_ms.clamp(80, 800);
        self.edge_hide.top_trigger_distance = self.edge_hide.top_trigger_distance.clamp(8, 128);
        self.edge_hide.top_hover_delay_ms = self.edge_hide.top_hover_delay_ms.clamp(200, 2000);
        self.edge_hide.trigger_distance = self.edge_hide.trigger_distance.clamp(4, 96);
        self.edge_hide.trigger_ratio = self.edge_hide.trigger_ratio.clamp(1, 100);
        self.edge_hide.collapse_delay_ms = self.edge_hide.collapse_delay_ms.clamp(0, 5000);
        self.edge_hide.restore_delay_ms = self.edge_hide.restore_delay_ms.clamp(0, 5000);
        self.edge_hide.excluded_apps = normalized_string_list(self.edge_hide.excluded_apps);
        self.mouse_gestures.min_distance = self.mouse_gestures.min_distance.clamp(12, 240);
        self.mouse_gestures.sensitivity = self.mouse_gestures.sensitivity.clamp(35, 95);
        self.mouse_gestures.paused_apps = normalized_string_list(self.mouse_gestures.paused_apps);
        self.mouse_gestures.gestures =
            normalize_gestures(self.mouse_gestures.gestures, source_schema_version);
        self.window_drag.move_modifiers =
            normalized_drag_modifiers(self.window_drag.move_modifiers);
        self.window_drag.resize_modifiers =
            normalized_drag_modifiers(self.window_drag.resize_modifiers);
        self.window_drag.paused_apps = normalized_string_list(self.window_drag.paused_apps);
        self.window_drag.update_hz = self.window_drag.update_hz.clamp(30, 240);
        if source_schema_version < 5 {
            migrate_circle_topmost_action(&mut self.mouse_gestures.gestures);
        }

        self.edge_hide.edges = normalized_edges(self.edge_hide.edges);
        self.edge_hide.monitor_profiles = self
            .edge_hide
            .monitor_profiles
            .into_iter()
            .filter_map(|mut profile| {
                profile.monitor_id = profile.monitor_id.trim().to_string();
                if profile.monitor_id.is_empty() {
                    return None;
                }
                profile.edges = normalized_edges(profile.edges);
                Some(profile)
            })
            .collect();

        let defaults = default_hotzones();
        self.hotzones = HotzoneId::all()
            .into_iter()
            .map(|id| {
                let mut setting = self
                    .hotzones
                    .iter()
                    .find(|setting| setting.id == id)
                    .cloned()
                    .unwrap_or_else(|| {
                        defaults
                            .iter()
                            .find(|setting| setting.id == id)
                            .expect("all hotzone defaults must exist")
                            .clone()
                    });
                setting.normalize_actions();
                setting.apply_continuous_action_defaults(self.action_cooldown_ms);
                setting
            })
            .collect();

        self.monitor_profiles = self
            .monitor_profiles
            .into_iter()
            .filter_map(|mut profile| {
                profile.monitor_id = profile.monitor_id.trim().to_string();
                if profile.monitor_id.is_empty() {
                    return None;
                }
                profile.hotzones = normalize_hotzones(profile.hotzones);
                for hotzone in &mut profile.hotzones {
                    hotzone.apply_continuous_action_defaults(self.action_cooldown_ms);
                }
                Some(profile)
            })
            .collect();

        self
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotzoneSetting {
    pub id: HotzoneId,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub actions: Vec<TriggerAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<HotzoneGeometry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<TriggerKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<HotzoneAction>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum HotzoneGeometry {
    Corner {
        width: i32,
        height: i32,
        #[serde(default = "default_true")]
        linked: bool,
    },
    Edge {
        thickness: i32,
        #[serde(rename = "lengthPercent")]
        length_percent: i32,
    },
}

impl HotzoneGeometry {
    pub fn normalized(self, id: HotzoneId) -> Option<Self> {
        let corner = matches!(
            id,
            HotzoneId::TopLeft
                | HotzoneId::TopRight
                | HotzoneId::BottomLeft
                | HotzoneId::BottomRight
        );
        match self {
            Self::Corner {
                width,
                height,
                linked,
            } if corner => Some(Self::Corner {
                width: width.clamp(2, 128),
                height: height.clamp(2, 128),
                linked,
            }),
            Self::Edge {
                thickness,
                length_percent,
            } if !corner => Some(Self::Edge {
                thickness: thickness.clamp(2, 48),
                length_percent: length_percent.clamp(10, 100),
            }),
            _ => None,
        }
    }
}

impl HotzoneSetting {
    fn normalize_actions(&mut self) {
        self.geometry = self
            .geometry
            .take()
            .and_then(|shape| shape.normalized(self.id));
        if self.actions.is_empty() {
            if let (Some(trigger), Some(action)) = (self.trigger, self.action.clone()) {
                self.actions.push(TriggerAction {
                    trigger,
                    action,
                    modifier_actions: Vec::new(),
                    cooldown_ms: None,
                    hover_delay_ms: None,
                });
            }
        }
        let mut normalized: Vec<TriggerAction> = Vec::new();
        for mut item in self.actions.drain(..) {
            item.action.value = normalized_optional_string(item.action.value);
            migrate_legacy_volume_shortcut(&mut item.action);
            item.modifier_actions = normalize_modifier_actions(item.modifier_actions);
            item.cooldown_ms = item.cooldown_ms.map(|value| value.clamp(10, 5000));
            item.hover_delay_ms = item.hover_delay_ms.map(|value| value.clamp(0, 3000));
            if let Some(index) = normalized
                .iter()
                .position(|existing| existing.trigger == item.trigger)
            {
                normalized[index] = item;
            } else {
                normalized.push(item);
            }
        }
        self.actions = normalized;
        self.trigger = None;
        self.action = None;
    }

    pub fn has_action(&self) -> bool {
        self.actions.iter().any(|item| {
            item.action.kind != ActionKind::None
                || item
                    .modifier_actions
                    .iter()
                    .any(|variant| variant.action.kind != ActionKind::None)
        })
    }

    fn apply_continuous_action_defaults(&mut self, fallback_cooldown_ms: u64) {
        for item in &mut self.actions {
            if matches!(
                item.action.kind,
                ActionKind::VolumeAdjust | ActionKind::BrightnessAdjust
            ) && item.cooldown_ms.is_none()
            {
                item.cooldown_ms = Some(fallback_cooldown_ms.min(32));
            }
        }
    }
}

fn migrate_legacy_volume_shortcut(action: &mut HotzoneAction) {
    if action.kind != ActionKind::Shortcut {
        return;
    }
    let Some(value) = action.value.as_deref() else {
        return;
    };
    if value.eq_ignore_ascii_case("VolumeUp") {
        action.kind = ActionKind::VolumeAdjust;
        action.value = Some("0.02".to_string());
    } else if value.eq_ignore_ascii_case("VolumeDown") {
        action.kind = ActionKind::VolumeAdjust;
        action.value = Some("-0.02".to_string());
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerAction {
    pub trigger: TriggerKind,
    #[serde(default)]
    pub action: HotzoneAction,
    #[serde(default)]
    pub modifier_actions: Vec<ModifierAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover_delay_ms: Option<u64>,
}

impl TriggerAction {
    pub fn action_for_modifier_mask(&self, mask: u8) -> &HotzoneAction {
        self.modifier_actions
            .iter()
            .find(|variant| modifier_mask(&variant.modifiers) == mask)
            .map(|variant| &variant.action)
            .unwrap_or(&self.action)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorProfile {
    pub monitor_id: String,
    #[serde(default = "default_hotzones")]
    pub hotzones: Vec<HotzoneSetting>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HotzoneId {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl HotzoneId {
    pub fn all() -> [Self; 8] {
        [
            Self::TopLeft,
            Self::Top,
            Self::TopRight,
            Self::Right,
            Self::BottomRight,
            Self::Bottom,
            Self::BottomLeft,
            Self::Left,
        ]
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerKind {
    Hover,
    LeftClick,
    RightClick,
    #[serde(alias = "wheel")]
    WheelUp,
    WheelDown,
    SlideForward,
    SlideBackward,
}

impl Default for TriggerKind {
    fn default() -> Self {
        Self::Hover
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HotzoneAction {
    #[serde(default)]
    pub kind: ActionKind,
    #[serde(default)]
    pub value: Option<String>,
}

impl Default for HotzoneAction {
    fn default() -> Self {
        Self {
            kind: ActionKind::None,
            value: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionKind {
    None,
    Shortcut,
    ShowDesktop,
    ToggleWindowTopmost,
    LockScreen,
    VolumeAdjust,
    BrightnessAdjust,
    OpenCommand,
    #[serde(rename = "host-action", alias = "utools-redirect")]
    HostAction,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModifierKey {
    Ctrl,
    Alt,
    Shift,
    Win,
}

impl ModifierKey {
    pub const fn all() -> [Self; 4] {
        [Self::Ctrl, Self::Alt, Self::Shift, Self::Win]
    }

    pub const fn mask(self) -> u8 {
        match self {
            Self::Ctrl => 1,
            Self::Alt => 2,
            Self::Shift => 4,
            Self::Win => 8,
        }
    }
}

fn modifier_mask(modifiers: &[ModifierKey]) -> u8 {
    modifiers.iter().fold(0, |mask, key| mask | key.mask())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifierAction {
    #[serde(default)]
    pub modifiers: Vec<ModifierKey>,
    #[serde(default)]
    pub action: HotzoneAction,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
    X1,
    X2,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MouseGestureConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub trigger_button: GestureTriggerButton,
    #[serde(default = "default_gesture_min_distance")]
    pub min_distance: i32,
    #[serde(default = "default_gesture_sensitivity")]
    pub sensitivity: i32,
    #[serde(default = "default_true")]
    pub show_trail: bool,
    #[serde(default = "default_true")]
    pub fullscreen_pause: bool,
    #[serde(default)]
    pub paused_apps: Vec<String>,
    #[serde(default = "default_gestures")]
    pub gestures: Vec<GestureTemplate>,
}

impl Default for MouseGestureConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger_button: GestureTriggerButton::Right,
            min_distance: default_gesture_min_distance(),
            sensitivity: default_gesture_sensitivity(),
            show_trail: true,
            fullscreen_pause: true,
            paused_apps: Vec::new(),
            gestures: default_gestures(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GestureTriggerButton {
    #[default]
    Right,
    Middle,
    X1,
    X2,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GestureMode {
    #[default]
    Action,
    RegionScreenshot,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GesturePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GestureTemplate {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub mode: GestureMode,
    #[serde(default)]
    pub action: HotzoneAction,
    #[serde(default)]
    pub modifier_actions: Vec<ModifierAction>,
    #[serde(default)]
    pub samples: Vec<Vec<GesturePoint>>,
}

impl GestureTemplate {
    pub fn action_for_modifier_mask(&self, mask: u8) -> &HotzoneAction {
        self.modifier_actions
            .iter()
            .find(|variant| modifier_mask(&variant.modifiers) == mask)
            .map(|variant| &variant.action)
            .unwrap_or(&self.action)
    }
}

impl Default for ActionKind {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowDragConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_drag_update_hz")]
    pub update_hz: u64,
    #[serde(default = "default_drag_modifiers")]
    pub move_modifiers: Vec<ModifierKey>,
    #[serde(default = "default_drag_modifiers")]
    pub resize_modifiers: Vec<ModifierKey>,
    #[serde(default)]
    pub move_button: MouseButton,
    #[serde(default = "default_resize_button")]
    pub resize_button: MouseButton,
    /// 命中这些程序时完全不接管拖拽/缩放，让 Alt + 鼠标的组合键留给目标软件。
    /// 与鼠标手势、贴边隐藏的同类名单语义一致：按进程名、窗口标题或窗口类名做包含匹配。
    #[serde(default)]
    pub paused_apps: Vec<String>,
}

impl Default for WindowDragConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            update_hz: default_drag_update_hz(),
            move_modifiers: default_drag_modifiers(),
            resize_modifiers: default_drag_modifiers(),
            move_button: MouseButton::Left,
            resize_button: default_resize_button(),
            paused_apps: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopmostPinConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for TopmostPinConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum OcrLanguage {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "zh-Hans")]
    ZhHans,
    #[serde(rename = "en")]
    English,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScreenshotResultMode {
    #[default]
    Pin,
    CopyText,
    PinAndCopy,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrConfig {
    #[serde(default)]
    pub language: OcrLanguage,
    #[serde(default)]
    pub screenshot_result: ScreenshotResultMode,
    #[serde(default = "default_true")]
    pub pin_offset: bool,
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            language: OcrLanguage::default(),
            screenshot_result: ScreenshotResultMode::default(),
            pin_offset: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeHideConfig {
    #[serde(default = "default_true")]
    pub animation_enabled: bool,
    #[serde(default = "default_animation_duration_ms")]
    pub animation_duration_ms: u64,
    #[serde(default = "default_top_trigger_distance")]
    pub top_trigger_distance: i32,
    #[serde(default = "default_top_hover_delay_ms")]
    pub top_hover_delay_ms: u64,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub show_preview: bool,
    #[serde(default = "default_true")]
    pub show_restore_hint: bool,
    #[serde(default)]
    pub keep_expanded_when_foreground: bool,
    #[serde(default = "default_edges")]
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub monitor_profiles: Vec<EdgeHideMonitorProfile>,
    #[serde(default = "default_strip_size")]
    pub strip_size: i32,
    #[serde(default = "default_trigger_distance")]
    pub trigger_distance: i32,
    #[serde(default = "default_true")]
    pub distance_trigger_enabled: bool,
    #[serde(default = "default_true")]
    pub ratio_trigger_enabled: bool,
    #[serde(default = "default_trigger_ratio")]
    pub trigger_ratio: i32,
    #[serde(default = "default_collapse_delay_ms")]
    pub collapse_delay_ms: u64,
    #[serde(default = "default_restore_delay_ms")]
    pub restore_delay_ms: u64,
    #[serde(default)]
    pub excluded_apps: Vec<String>,
}

impl Default for EdgeHideConfig {
    fn default() -> Self {
        Self {
            animation_enabled: true,
            animation_duration_ms: default_animation_duration_ms(),
            top_trigger_distance: default_top_trigger_distance(),
            top_hover_delay_ms: default_top_hover_delay_ms(),
            enabled: false,
            show_preview: true,
            show_restore_hint: true,
            keep_expanded_when_foreground: false,
            edges: default_edges(),
            monitor_profiles: Vec::new(),
            strip_size: default_strip_size(),
            trigger_distance: default_trigger_distance(),
            distance_trigger_enabled: true,
            ratio_trigger_enabled: true,
            trigger_ratio: default_trigger_ratio(),
            collapse_delay_ms: default_collapse_delay_ms(),
            restore_delay_ms: default_restore_delay_ms(),
            excluded_apps: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeHideMonitorProfile {
    pub monitor_id: String,
    #[serde(default = "default_edges")]
    pub edges: Vec<Edge>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Edge {
    Left,
    Top,
    Right,
    Bottom,
}

fn default_drag_modifiers() -> Vec<ModifierKey> {
    vec![ModifierKey::Alt]
}

fn default_resize_button() -> MouseButton {
    MouseButton::Right
}

fn default_true() -> bool {
    true
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

fn default_gesture_min_distance() -> i32 {
    36
}

fn default_gesture_sensitivity() -> i32 {
    72
}

fn default_edge_size() -> i32 {
    8
}

fn default_hover_delay_ms() -> u64 {
    350
}

fn default_animation_duration_ms() -> u64 {
    160
}

fn default_top_trigger_distance() -> i32 {
    48
}

fn default_top_hover_delay_ms() -> u64 {
    600
}

fn default_drag_update_hz() -> u64 {
    120
}

fn default_poll_interval_ms() -> u64 {
    33
}

fn default_action_cooldown_ms() -> u64 {
    700
}

fn default_strip_size() -> i32 {
    16
}

fn default_trigger_distance() -> i32 {
    16
}

fn default_trigger_ratio() -> i32 {
    33
}

fn default_collapse_delay_ms() -> u64 {
    300
}

fn default_restore_delay_ms() -> u64 {
    200
}

fn default_edges() -> Vec<Edge> {
    vec![Edge::Left, Edge::Top, Edge::Right, Edge::Bottom]
}

fn normalized_edges(values: Vec<Edge>) -> Vec<Edge> {
    let mut edges = Vec::new();
    for edge in values {
        if !edges.contains(&edge) {
            edges.push(edge);
        }
    }
    edges
}

fn default_hotzones() -> Vec<HotzoneSetting> {
    HotzoneId::all()
        .into_iter()
        .map(|id| HotzoneSetting {
            id,
            enabled: false,
            actions: Vec::new(),
            geometry: None,
            trigger: None,
            action: None,
        })
        .collect()
}

fn default_gestures() -> Vec<GestureTemplate> {
    vec![
        builtin_gesture(
            "gesture-up",
            HotzoneAction {
                kind: ActionKind::Shortcut,
                value: Some("Ctrl+C".into()),
            },
            &[(0.5, 0.9), (0.5, 0.1)],
            GestureMode::Action,
        ),
        builtin_gesture(
            "gesture-down",
            HotzoneAction {
                kind: ActionKind::Shortcut,
                value: Some("Ctrl+V".into()),
            },
            &[(0.5, 0.1), (0.5, 0.9)],
            GestureMode::Action,
        ),
        builtin_gesture(
            "gesture-l",
            HotzoneAction {
                kind: ActionKind::Shortcut,
                value: Some("Alt+F4".into()),
            },
            &[(0.2, 0.1), (0.2, 0.85), (0.85, 0.85)],
            GestureMode::Action,
        ),
        builtin_gesture(
            "gesture-circle",
            HotzoneAction {
                kind: ActionKind::ToggleWindowTopmost,
                value: None,
            },
            &circle_points(),
            GestureMode::Action,
        ),
        builtin_gesture(
            "gesture-rectangle",
            HotzoneAction::default(),
            &[
                (0.15, 0.15),
                (0.85, 0.15),
                (0.85, 0.85),
                (0.15, 0.85),
                (0.15, 0.15),
            ],
            GestureMode::RegionScreenshot,
        ),
    ]
}

fn migrate_circle_topmost_action(gestures: &mut [GestureTemplate]) {
    if let Some(circle) = gestures
        .iter_mut()
        .find(|gesture| gesture.id == "gesture-circle")
    {
        if circle.action.kind == ActionKind::ShowDesktop {
            circle.action = HotzoneAction {
                kind: ActionKind::ToggleWindowTopmost,
                value: None,
            };
        }
    }
}

fn circle_points() -> Vec<(f32, f32)> {
    (0..=24)
        .map(|index| {
            let angle = std::f32::consts::TAU * index as f32 / 24.0 - std::f32::consts::FRAC_PI_2;
            (0.5 + angle.cos() * 0.38, 0.5 + angle.sin() * 0.38)
        })
        .collect()
}

fn builtin_gesture(
    id: &str,
    action: HotzoneAction,
    points: &[(f32, f32)],
    mode: GestureMode,
) -> GestureTemplate {
    GestureTemplate {
        id: id.into(),
        name: String::new(),
        enabled: true,
        builtin: true,
        mode,
        action,
        modifier_actions: Vec::new(),
        samples: vec![points.iter().map(|&(x, y)| GesturePoint { x, y }).collect()],
    }
}

fn is_legacy_gesture_name(id: &str, name: &str) -> bool {
    match id {
        "gesture-up" => matches!(name, "向上 · 复制" | "Up · Copy"),
        "gesture-down" => matches!(name, "向下 · 粘贴" | "Down · Paste"),
        "gesture-l" => matches!(name, "L 型 · 关闭窗口" | "L · Close"),
        "gesture-circle" => matches!(
            name,
            "圆圈 · 切换窗口置顶" | "圆圈 · 显示桌面" | "Circle · Topmost"
        ),
        "gesture-rectangle" => matches!(name, "矩形截图" | "Region capture"),
        _ => false,
    }
}

fn normalize_gestures(
    values: Vec<GestureTemplate>,
    source_schema_version: u32,
) -> Vec<GestureTemplate> {
    let defaults = default_gestures();
    let mut normalized = Vec::new();
    for mut gesture in values.into_iter().take(MAX_GESTURE_TEMPLATES) {
        gesture.id = gesture.id.trim().chars().take(80).collect();
        gesture.name = gesture.name.trim().chars().take(40).collect();
        let builtin = defaults.iter().any(|item| item.id == gesture.id);
        if source_schema_version < 8 && is_legacy_gesture_name(&gesture.id, &gesture.name) {
            gesture.name.clear();
        }
        gesture.builtin = builtin;
        if gesture.id.is_empty()
            || (!builtin && gesture.name.is_empty())
            || normalized
                .iter()
                .any(|item: &GestureTemplate| item.id == gesture.id)
        {
            continue;
        }
        gesture.action.value = normalized_optional_string(gesture.action.value);
        migrate_legacy_volume_shortcut(&mut gesture.action);
        gesture.modifier_actions = normalize_modifier_actions(gesture.modifier_actions);
        gesture.samples = gesture
            .samples
            .into_iter()
            .filter_map(normalize_gesture_sample)
            .take(8)
            .collect();
        if !gesture.samples.is_empty() {
            normalized.push(gesture);
        }
    }
    let mut output = Vec::with_capacity(MAX_GESTURE_TEMPLATES);
    for fallback in &defaults {
        if let Some(position) = normalized
            .iter()
            .position(|gesture| gesture.id == fallback.id)
        {
            let mut gesture = normalized.remove(position);
            gesture.builtin = true;
            gesture.mode = fallback.mode;
            output.push(gesture);
        } else {
            output.push(fallback.clone());
        }
    }
    let remaining = MAX_GESTURE_TEMPLATES - output.len();
    output.extend(normalized.into_iter().take(remaining));
    output
}

fn normalize_gesture_sample(points: Vec<GesturePoint>) -> Option<Vec<GesturePoint>> {
    let points: Vec<_> = points
        .into_iter()
        .filter(|point| point.x.is_finite() && point.y.is_finite())
        .map(|point| GesturePoint {
            x: point.x.clamp(0.0, 1.0),
            y: point.y.clamp(0.0, 1.0),
        })
        .take(512)
        .collect();
    (points.len() >= 2).then_some(points)
}

fn normalize_hotzones(hotzones: Vec<HotzoneSetting>) -> Vec<HotzoneSetting> {
    let defaults = default_hotzones();
    HotzoneId::all()
        .into_iter()
        .map(|id| {
            let mut setting = hotzones
                .iter()
                .find(|setting| setting.id == id)
                .cloned()
                .unwrap_or_else(|| {
                    defaults
                        .iter()
                        .find(|setting| setting.id == id)
                        .expect("all hotzone defaults must exist")
                        .clone()
                });
            setting.normalize_actions();
            setting
        })
        .collect()
}

fn normalize_modifier_actions(values: Vec<ModifierAction>) -> Vec<ModifierAction> {
    let mut output = Vec::new();
    for mut variant in values {
        variant.modifiers = normalized_modifiers(variant.modifiers);
        if variant.modifiers.is_empty()
            || output
                .iter()
                .any(|existing: &ModifierAction| existing.modifiers == variant.modifiers)
        {
            continue;
        }
        variant.action.value = normalized_optional_string(variant.action.value);
        migrate_legacy_volume_shortcut(&mut variant.action);
        output.push(variant);
        if output.len() >= 15 {
            break;
        }
    }
    output
}

fn normalized_drag_modifiers(values: Vec<ModifierKey>) -> Vec<ModifierKey> {
    let normalized = normalized_modifiers(values);
    if normalized.is_empty() {
        default_drag_modifiers()
    } else {
        normalized
    }
}
fn normalized_modifiers(values: Vec<ModifierKey>) -> Vec<ModifierKey> {
    ModifierKey::all()
        .into_iter()
        .filter(|key| values.contains(key))
        .collect()
}

fn is_hex_color(value: &str) -> bool {
    let value = value.trim();
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn normalized_string_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim();
        if !value.is_empty()
            && !normalized
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(value))
        {
            normalized.push(value.to_string());
        }
    }
    normalized
}

fn normalized_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_options_keep_the_original_animation_default_and_clamp_custom_values() {
        let defaults = AppConfig::default();
        assert_eq!(defaults.edge_hide.animation_duration_ms, 160);
        assert_eq!(defaults.edge_hide.top_trigger_distance, 48);
        assert_eq!(defaults.edge_hide.top_hover_delay_ms, 600);
        assert_eq!(defaults.window_drag.update_hz, 120);
        let old: AppConfig =
            serde_json::from_str(r#"{"schemaVersion":9,"edgeHide":{"animationEnabled":false}}"#)
                .unwrap();
        assert!(!old.edge_hide.animation_enabled);
        assert_eq!(old.edge_hide.animation_duration_ms, 160);
        let mut config = defaults;
        config.edge_hide.animation_duration_ms = 9999;
        config.edge_hide.top_trigger_distance = 0;
        config.edge_hide.top_hover_delay_ms = 1;
        config.window_drag.update_hz = 9999;
        let normalized = config.normalized();
        assert_eq!(normalized.edge_hide.animation_duration_ms, 800);
        assert_eq!(normalized.edge_hide.top_trigger_distance, 8);
        assert_eq!(normalized.edge_hide.top_hover_delay_ms, 200);
        assert_eq!(normalized.window_drag.update_hz, 240);
        let roundtrip: AppConfig =
            serde_json::from_value(serde_json::to_value(normalized).unwrap()).unwrap();
        assert_eq!(roundtrip.window_drag.update_hz, 240);
    }

    #[test]
    fn edge_hide_animation_defaults_on_and_preserves_disabled_on_roundtrip() {
        let default: AppConfig = serde_json::from_str("{}").unwrap();
        assert!(default.edge_hide.animation_enabled);
        let disabled: AppConfig = serde_json::from_value(serde_json::json!({
            "schemaVersion": 7, "edgeHide": { "animationEnabled": false }
        }))
        .unwrap();
        assert!(!disabled.clone().normalized().edge_hide.animation_enabled);
        let roundtrip: AppConfig =
            serde_json::from_value(serde_json::to_value(disabled).unwrap()).unwrap();
        assert!(!roundtrip.edge_hide.animation_enabled);
    }

    #[test]
    fn shared_schema_nine_migration_preserves_user_data_and_is_idempotent() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/i18n-migration.json")).unwrap();
        let config: AppConfig = serde_json::from_value(fixture["input"].clone()).unwrap();
        let migrated = config.normalized();
        assert_eq!(migrated.schema_version, 9);
        for (id, name) in fixture["expectedNames"].as_object().unwrap() {
            let gesture = migrated
                .mouse_gestures
                .gestures
                .iter()
                .find(|gesture| &gesture.id == id)
                .unwrap();
            assert_eq!(gesture.name, name.as_str().unwrap_or(""));
            assert!(!gesture.enabled);
            assert_eq!(gesture.action.value.as_deref(), Some("Ctrl+K"));
            assert_eq!(
                gesture.modifier_actions[0].action.value.as_deref(),
                Some("Ctrl+Q")
            );
            assert_eq!(
                gesture.samples,
                serde_json::from_value::<Vec<Vec<GesturePoint>>>(
                    fixture["input"]["mouseGestures"]["gestures"][0]["samples"].clone()
                )
                .unwrap()
            );
        }
        assert!(
            !migrated
                .mouse_gestures
                .gestures
                .iter()
                .find(|gesture| gesture.id == "custom")
                .unwrap()
                .builtin
        );
        let serialized = serde_json::to_value(&migrated).unwrap();
        assert!(serialized["mouseGestures"]["gestures"][0]
            .get("name")
            .is_none());
        let roundtrip: AppConfig = serde_json::from_value(serialized.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(roundtrip.normalized()).unwrap(),
            serialized
        );
        let mut customized = AppConfig::default();
        customized.mouse_gestures.gestures[0].name = "向上 · 复制".into();
        assert_eq!(
            customized.normalized().mouse_gestures.gestures[0].name,
            "向上 · 复制"
        );
    }

    #[test]
    fn matches_shared_frontend_helper_configuration_contract_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/config-contract.json"))
                .unwrap();
        let config = serde_json::from_value::<AppConfig>(fixture["input"].clone())
            .unwrap()
            .normalized();
        let actual = serde_json::json!({
            "schemaVersion": config.schema_version,
            "hotzonesEnabled": config.hotzones_enabled,
            "showHotzoneHint": config.show_hotzone_hint,
            "cornerGeometry": config.hotzones.iter().find(|zone| zone.id == HotzoneId::TopLeft).unwrap().geometry,
            "edgeGeometry": config.hotzones.iter().find(|zone| zone.id == HotzoneId::Bottom).unwrap().geometry,
            "edgeSize": config.edge_size,
            "hoverDelayMs": config.hover_delay_ms,
            "pollIntervalMs": config.poll_interval_ms,
            "actionCooldownMs": config.action_cooldown_ms,
            "pausedApps": config.paused_apps,
            "stripSize": config.edge_hide.strip_size,
            "edgeHidePreviewEnabled": config.edge_hide.show_preview,
            "showRestoreHint": config.edge_hide.show_restore_hint,
            "keepExpandedWhenForeground": config.edge_hide.keep_expanded_when_foreground,
            "triggerDistance": config.edge_hide.trigger_distance,
            "triggerRatio": config.edge_hide.trigger_ratio,
            "collapseDelayMs": config.edge_hide.collapse_delay_ms,
            "restoreDelayMs": config.edge_hide.restore_delay_ms,
            "minDistance": config.mouse_gestures.min_distance,
            "sensitivity": config.mouse_gestures.sensitivity,
            "gesturePausedApps": config.mouse_gestures.paused_apps,
            "windowDragEnabled": config.window_drag.enabled,
            "windowDragPausedApps": config.window_drag.paused_apps,
            "moveModifiers": config.window_drag.move_modifiers,
            "resizeModifiers": config.window_drag.resize_modifiers,
            "topmostPinEnabled": config.topmost_pin.enabled,
            "ocrLanguage": config.ocr.language,
            "screenshotResult": config.ocr.screenshot_result,
            "pinOffset": config.ocr.pin_offset
        });
        assert_eq!(actual, fixture["expected"]);
    }

    #[test]
    fn pin_offset_defaults_on_and_preserves_explicit_false_after_reload() {
        assert!(OcrConfig::default().pin_offset);
        let legacy: OcrConfig = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(legacy.pin_offset);
        let config: OcrConfig =
            serde_json::from_value(serde_json::json!({"pinOffset": false})).unwrap();
        let saved = serde_json::to_value(config).unwrap();
        let reloaded: OcrConfig = serde_json::from_value(saved).unwrap();
        assert!(!reloaded.pin_offset);
    }

    #[test]
    fn empty_drag_modifiers_fall_back_to_alt() {
        let config: AppConfig = serde_json::from_str::<AppConfig>(
            r#"{
            "windowDrag": { "moveModifiers": [], "resizeModifiers": [] }
        }"#,
        )
        .unwrap()
        .normalized();
        assert_eq!(config.window_drag.move_modifiers, vec![ModifierKey::Alt]);
        assert_eq!(config.window_drag.resize_modifiers, vec![ModifierKey::Alt]);
    }
    #[test]
    fn hotzone_hint_choice_survives_normalization_and_serialization() {
        for enabled in [true, false] {
            let config: AppConfig = serde_json::from_value(serde_json::json!({
                "schemaVersion": 8,
                "showHotzoneHint": enabled
            }))
            .unwrap();
            let config = config.normalized();
            assert_eq!(config.show_hotzone_hint, enabled);
            assert_eq!(config.schema_version, 9);
            let saved = serde_json::to_value(&config).unwrap();
            assert_eq!(saved["showHotzoneHint"], enabled);
            let reloaded: AppConfig = serde_json::from_value(saved).unwrap();
            assert_eq!(reloaded.normalized().show_hotzone_hint, enabled);
        }
    }

    #[test]
    fn schema_eight_shapes_are_preserved_without_materializing_overrides() {
        let config: AppConfig = serde_json::from_value(serde_json::json!({ "schemaVersion": 8, "edgeSize": 24,
            "showHotzoneHint": false, "hotzonesEnabled": true,
            "hotzones": [{"id":"bottom", "actions":[{"trigger":"wheel-up","action":{"kind":"volume-adjust","value":"0.02"}}]}]
        })).unwrap();
        let config = config.normalized();
        assert_eq!(config.schema_version, 9);
        assert_eq!(config.edge_size, 24);
        assert!(!config.show_hotzone_hint);
        assert!(config.hotzones.iter().all(|zone| zone.geometry.is_none()));
        assert!(config
            .hotzones
            .iter()
            .find(|zone| zone.id == HotzoneId::Bottom)
            .unwrap()
            .has_action());
    }

    #[test]
    fn per_monitor_geometry_round_trips_without_enabling_actions() {
        let config: AppConfig = serde_json::from_value(serde_json::json!({"schemaVersion":9,
            "monitorProfiles":[
                {"monitorId":"first","hotzones":[{"id":"bottom","geometry":{"kind":"edge","thickness":16,"lengthPercent":100}}]},
                {"monitorId":"second","hotzones":[{"id":"bottom","geometry":{"kind":"edge","thickness":4,"lengthPercent":20}}]}
            ]
        })).unwrap();
        let config = config.normalized();
        let saved = serde_json::to_value(&config).unwrap();
        let roundtrip: AppConfig = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(serde_json::to_value(roundtrip.normalized()).unwrap(), saved);
        assert_eq!(
            config.monitor_profiles[0]
                .hotzones
                .iter()
                .find(|zone| zone.id == HotzoneId::Bottom)
                .unwrap()
                .geometry,
            Some(HotzoneGeometry::Edge {
                thickness: 16,
                length_percent: 100
            })
        );
        assert!(config
            .monitor_profiles
            .iter()
            .flat_map(|profile| &profile.hotzones)
            .all(|zone| !zone.has_action()));
    }

    #[test]
    fn config_deserializes_with_defaults() {
        let config: AppConfig = serde_json::from_str(r#"{"enabled":true}"#).unwrap();

        assert!(config.enabled);
        assert!(config.hotzones_enabled);
        assert!(config.show_hotzone_hint);
        assert_eq!(config.edge_size, 8);
        assert_eq!(config.hotzones.len(), 8);
        assert_eq!(config.edge_hide.strip_size, 16);
        assert!(config.edge_hide.show_preview);
        assert!(config.edge_hide.show_restore_hint);
        assert!(!config.edge_hide.keep_expanded_when_foreground);
        assert!(config.edge_hide.distance_trigger_enabled);
        assert!(config.edge_hide.ratio_trigger_enabled);
        assert_eq!(config.edge_hide.trigger_ratio, 33);
        assert_eq!(config.edge_hide.collapse_delay_ms, 300);
        assert_eq!(config.edge_hide.restore_delay_ms, 200);
    }

    #[test]
    fn edge_hide_foreground_hold_defaults_off_and_preserves_an_explicit_on() {
        let defaulted: AppConfig = serde_json::from_str(r#"{"edgeHide":{}}"#).unwrap();
        let disabled: AppConfig =
            serde_json::from_str(r#"{"edgeHide":{"keepExpandedWhenForeground":true}}"#).unwrap();

        assert!(!defaulted.edge_hide.keep_expanded_when_foreground);
        assert!(disabled.edge_hide.keep_expanded_when_foreground);
    }

    #[test]
    fn restore_hint_visibility_defaults_on_and_preserves_an_explicit_off() {
        let defaulted: AppConfig = serde_json::from_str(r#"{"edgeHide":{}}"#).unwrap();
        let disabled: AppConfig =
            serde_json::from_str(r#"{"edgeHide":{"showRestoreHint":false}}"#).unwrap();

        assert!(defaulted.edge_hide.show_restore_hint);
        assert!(!disabled.edge_hide.show_restore_hint);
    }

    #[test]
    fn legacy_zone_switches_migrate_to_the_hotzone_global_switch() {
        let enabled = serde_json::from_str::<AppConfig>(
            r#"{
                "schemaVersion": 5,
                "hotzones": [{
                    "id": "right",
                    "enabled": true,
                    "actions": [{ "trigger": "hover", "action": { "kind": "show-desktop" } }]
                }]
            }"#,
        )
        .unwrap()
        .normalized();
        let disabled = serde_json::from_str::<AppConfig>(r#"{ "schemaVersion": 5 }"#)
            .unwrap()
            .normalized();
        let explicit = serde_json::from_str::<AppConfig>(
            r#"{
                "schemaVersion": 6,
                "hotzonesEnabled": false,
                "hotzones": [{ "id": "right", "enabled": true }]
            }"#,
        )
        .unwrap()
        .normalized();

        assert!(enabled.hotzones_enabled);
        assert!(!disabled.hotzones_enabled);
        assert!(!explicit.hotzones_enabled);
    }

    #[test]
    fn config_normalization_clamps_values_and_deduplicates_entries() {
        let mut config = AppConfig::default();
        config.edge_size = -20;
        config.hover_delay_ms = 99_999;
        config.poll_interval_ms = 0;
        config.paused_apps = vec![
            "  explorer.exe  ".to_string(),
            "".to_string(),
            "explorer.exe".to_string(),
        ];
        config.edge_hide.strip_size = 999;
        config.edge_hide.trigger_distance = 0;
        config.edge_hide.trigger_ratio = 999;
        config.edge_hide.edges = vec![Edge::Left, Edge::Left];
        config.hotzones[0].actions = vec![TriggerAction {
            trigger: TriggerKind::Hover,
            action: HotzoneAction {
                kind: ActionKind::Shortcut,
                value: Some("  Win+D  ".to_string()),
            },
            modifier_actions: Vec::new(),
            cooldown_ms: Some(99_999),
            hover_delay_ms: Some(99_999),
        }];

        let config = config.normalized();

        assert_eq!(config.edge_size, 2);
        assert_eq!(config.hover_delay_ms, 3000);
        assert_eq!(config.poll_interval_ms, 10);
        assert_eq!(config.paused_apps, vec!["explorer.exe"]);
        assert_eq!(config.edge_hide.strip_size, 64);
        assert_eq!(config.edge_hide.trigger_distance, 4);
        assert_eq!(config.edge_hide.trigger_ratio, 100);
        assert_eq!(config.edge_hide.edges, vec![Edge::Left]);
        assert_eq!(
            config.hotzones[0].actions[0].action.value.as_deref(),
            Some("Win+D")
        );
        assert_eq!(config.hotzones[0].actions[0].cooldown_ms, Some(5000));
        assert_eq!(config.hotzones[0].actions[0].hover_delay_ms, Some(3000));
    }

    #[test]
    fn modifier_variants_are_sorted_deduplicated_and_selected_exactly() {
        let config = serde_json::from_str::<AppConfig>(
            r#"{
                "hotzones": [{
                    "id": "top-left",
                    "actions": [{
                        "trigger": "hover",
                        "action": { "kind": "show-desktop" },
                        "modifierActions": [
                            { "modifiers": ["shift", "ctrl", "shift"], "action": { "kind": "shortcut", "value": " Ctrl+C " } },
                            { "modifiers": ["ctrl", "shift"], "action": { "kind": "lock-screen" } },
                            { "modifiers": [], "action": { "kind": "lock-screen" } }
                        ]
                    }]
                }]
            }"#,
        )
        .unwrap()
        .normalized();
        let slot = &config.hotzones[0].actions[0];

        assert_eq!(slot.modifier_actions.len(), 1);
        assert_eq!(
            slot.modifier_actions[0].modifiers,
            vec![ModifierKey::Ctrl, ModifierKey::Shift]
        );
        assert_eq!(
            slot.action_for_modifier_mask(ModifierKey::Ctrl.mask() | ModifierKey::Shift.mask())
                .value
                .as_deref(),
            Some("Ctrl+C")
        );
        assert_eq!(
            slot.action_for_modifier_mask(ModifierKey::Alt.mask()).kind,
            ActionKind::ShowDesktop
        );
    }

    #[test]
    fn legacy_single_trigger_config_migrates_to_multi_action_slots() {
        let config = serde_json::from_str::<AppConfig>(
            r#"{
                "hotzones": [{
                    "id": "right",
                    "enabled": true,
                    "trigger": "wheel",
                    "action": { "kind": "shortcut", "value": " VolumeUp " }
                }]
            }"#,
        )
        .unwrap();
        let config = config.normalized();
        let right = config
            .hotzones
            .iter()
            .find(|zone| zone.id == HotzoneId::Right)
            .unwrap();

        assert!(right.enabled);
        assert_eq!(right.actions.len(), 1);
        assert_eq!(right.actions[0].trigger, TriggerKind::WheelUp);
        assert_eq!(right.actions[0].action.kind, ActionKind::VolumeAdjust);
        assert_eq!(right.actions[0].action.value.as_deref(), Some("0.02"));
    }

    #[test]
    fn edge_hide_monitor_profiles_keep_independent_directions() {
        let config: AppConfig = serde_json::from_str(
            r#"{
                "edgeHide": {
                    "enabled": true,
                    "monitorProfiles": [
                        { "monitorId": "display:-1920:0:0:1080", "edges": ["left"] },
                        { "monitorId": "display:0:0:1920:1080", "edges": ["right", "bottom"] }
                    ]
                }
            }"#,
        )
        .unwrap();
        let config = config.normalized();

        assert_eq!(config.edge_hide.monitor_profiles.len(), 2);
        assert_eq!(config.edge_hide.monitor_profiles[0].edges, vec![Edge::Left]);
        assert_eq!(
            config.edge_hide.monitor_profiles[1].edges,
            vec![Edge::Right, Edge::Bottom]
        );
    }

    #[test]
    fn brightness_actions_keep_their_values_and_timing_after_roundtrip() {
        let config: AppConfig = serde_json::from_value(serde_json::json!({
            "hotzones": [{
                "id": "right", "enabled": true,
                "actions": [
                    { "trigger": "wheel-up", "action": { "kind": "brightness-adjust", "value": "0.05" } },
                    { "trigger": "wheel-down", "action": { "kind": "brightness-adjust", "value": "-0.05" }, "cooldownMs": 80 }
                ]
            }]
        })).unwrap();
        let config = config.normalized();
        let roundtrip: AppConfig =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        let zone = roundtrip
            .hotzones
            .iter()
            .find(|zone| zone.id == HotzoneId::Right)
            .unwrap();
        assert_eq!(zone.actions[0].action.kind, ActionKind::BrightnessAdjust);
        assert_eq!(zone.actions[0].action.value.as_deref(), Some("0.05"));
        assert_eq!(zone.actions[0].cooldown_ms, Some(32));
        assert_eq!(zone.actions[1].action.value.as_deref(), Some("-0.05"));
        assert_eq!(zone.actions[1].cooldown_ms, Some(80));
    }

    #[test]
    fn trigger_slots_keep_independent_timing() {
        let config = serde_json::from_str::<AppConfig>(
            r#"{
                "hotzones": [{
                    "id": "right",
                    "enabled": true,
                    "actions": [
                        { "trigger": "hover", "action": { "kind": "show-desktop" }, "cooldownMs": 900, "hoverDelayMs": 480 },
                        { "trigger": "wheel-up", "action": { "kind": "volume-adjust", "value": "0.02" }, "cooldownMs": 24 }
                    ]
                }]
            }"#,
        )
        .unwrap()
        .normalized();
        let right = config
            .hotzones
            .iter()
            .find(|zone| zone.id == HotzoneId::Right)
            .unwrap();

        assert_eq!(right.actions[0].cooldown_ms, Some(900));
        assert_eq!(right.actions[0].hover_delay_ms, Some(480));
        assert_eq!(right.actions[1].cooldown_ms, Some(24));
        assert_eq!(right.actions[1].hover_delay_ms, None);
    }

    #[test]
    fn gesture_config_clamps_controls_and_keeps_builtin_templates() {
        let config = serde_json::from_str::<AppConfig>(
            r#"{
                "mouseGestures": {
                    "enabled": true,
                    "triggerButton": "x2",
                    "minDistance": 999,
                    "sensitivity": 4,
                    "pausedApps": [" game.exe ", "game.exe"],
                    "gestures": [{
                        "id": "custom-z", "name": " Z ", "enabled": true,
                        "builtin": false, "mode": "action",
                        "action": { "kind": "shortcut", "value": " Ctrl+Z " },
                        "samples": [[{"x":-2,"y":0},{"x":2,"y":1}]]
                    }]
                }
            }"#,
        )
        .unwrap()
        .normalized();

        assert!(config.mouse_gestures.enabled);
        assert_eq!(
            config.mouse_gestures.trigger_button,
            GestureTriggerButton::X2
        );
        assert_eq!(config.mouse_gestures.min_distance, 240);
        assert_eq!(config.mouse_gestures.sensitivity, 35);
        assert_eq!(config.mouse_gestures.paused_apps, vec!["game.exe"]);
        let custom = config
            .mouse_gestures
            .gestures
            .iter()
            .find(|gesture| gesture.id == "custom-z")
            .unwrap();
        assert_eq!(custom.name, "Z");
        assert_eq!(custom.samples[0][0], GesturePoint { x: 0.0, y: 0.0 });
        assert!(config
            .mouse_gestures
            .gestures
            .iter()
            .any(|gesture| gesture.id == "gesture-rectangle"));
    }

    #[test]
    fn gesture_capacity_preserves_builtins_and_restores_protected_fields() {
        let mut gestures: Vec<_> = (0..MAX_GESTURE_TEMPLATES)
            .map(|index| GestureTemplate {
                id: format!("custom-{index}"),
                name: format!("Custom {index}"),
                enabled: true,
                builtin: false,
                mode: GestureMode::Action,
                action: HotzoneAction::default(),
                modifier_actions: Vec::new(),
                samples: vec![vec![
                    GesturePoint { x: 0.0, y: 0.0 },
                    GesturePoint { x: 1.0, y: 1.0 },
                ]],
            })
            .collect();
        let mut rectangle = default_gestures()
            .into_iter()
            .find(|gesture| gesture.id == "gesture-rectangle")
            .unwrap();
        rectangle.builtin = false;
        rectangle.mode = GestureMode::Action;
        gestures[0] = rectangle;

        let normalized = normalize_gestures(gestures, SCHEMA_VERSION);
        let protected_rectangle = normalized
            .iter()
            .find(|gesture| gesture.id == "gesture-rectangle")
            .unwrap();

        assert_eq!(normalized.len(), MAX_GESTURE_TEMPLATES);
        assert_eq!(
            normalized.iter().filter(|gesture| gesture.builtin).count(),
            5
        );
        assert_eq!(
            normalized.iter().filter(|gesture| !gesture.builtin).count(),
            MAX_GESTURE_TEMPLATES - 5
        );
        assert!(protected_rectangle.builtin);
        assert_eq!(protected_rectangle.mode, GestureMode::RegionScreenshot);
    }

    #[test]
    fn legacy_circle_default_migrates_without_replacing_a_custom_action() {
        let mut legacy = AppConfig::default();
        legacy.schema_version = 4;
        let circle = legacy
            .mouse_gestures
            .gestures
            .iter_mut()
            .find(|gesture| gesture.id == "gesture-circle")
            .unwrap();
        circle.action = HotzoneAction {
            kind: ActionKind::ShowDesktop,
            value: None,
        };
        let migrated = legacy.normalized();
        assert_eq!(
            migrated
                .mouse_gestures
                .gestures
                .iter()
                .find(|gesture| gesture.id == "gesture-circle")
                .unwrap()
                .action
                .kind,
            ActionKind::ToggleWindowTopmost
        );

        let mut customized = AppConfig::default();
        customized.schema_version = 4;
        let circle = customized
            .mouse_gestures
            .gestures
            .iter_mut()
            .find(|gesture| gesture.id == "gesture-circle")
            .unwrap();
        circle.action = HotzoneAction {
            kind: ActionKind::Shortcut,
            value: Some("Win+D".to_string()),
        };
        let normalized = customized.normalized();
        let action = &normalized
            .mouse_gestures
            .gestures
            .iter()
            .find(|gesture| gesture.id == "gesture-circle")
            .unwrap()
            .action;
        assert_eq!(action.kind, ActionKind::Shortcut);
        assert_eq!(action.value.as_deref(), Some("Win+D"));
    }
}

#[cfg(test)]
mod taskbar_config_tests {
    use super::*;
    #[test]
    fn legacy_configuration_does_not_enable_injection() {
        let config: AppConfig = serde_json::from_str("{}").unwrap();
        assert!(!config.taskbar_appearance.enabled);
        assert!(!config.normalized().taskbar_appearance.enabled);
    }
    #[test]
    fn taskbar_opt_in_round_trips_without_touching_other_rules() {
        for enabled in [true, false] {
            let mut config = AppConfig::default();
            config.taskbar_appearance.enabled = enabled;
            config.taskbar_appearance.mode = "solid".into();
            config.taskbar_appearance.opacity = 83;
            config.taskbar_appearance.tint = "#ab12ef".into();
            config.taskbar_appearance.show_border = false;
            let json = serde_json::to_string(&config).unwrap();
            let restored = serde_json::from_str::<AppConfig>(&json)
                .unwrap()
                .normalized();
            assert_eq!(restored.taskbar_appearance.enabled, enabled);
            assert_eq!(restored.taskbar_appearance.mode, "solid");
            assert_eq!(restored.taskbar_appearance.opacity, 83);
            assert_eq!(restored.taskbar_appearance.tint, "#AB12EF");
            assert!(!restored.taskbar_appearance.show_border);
        }
    }
    #[test]
    fn old_boolean_only_prototype_keeps_its_clear_background() {
        let config =
            serde_json::from_str::<AppConfig>(r##"{"taskbarAppearance":{"enabled":true}}"##)
                .unwrap()
                .normalized();
        assert!(config.taskbar_appearance.enabled);
        assert_eq!(config.taskbar_appearance.mode, "transparent");
        assert!(!config.taskbar_appearance.show_border);
    }
    #[test]
    fn material_settings_are_normalized_and_keep_valid_color_whitespace() {
        let mut config = AppConfig::default();
        config.taskbar_appearance.mode = "unknown".into();
        config.taskbar_appearance.opacity = 400;
        config.taskbar_appearance.tint = "not-a-color".into();
        let normalized = config.normalized().taskbar_appearance;
        assert_eq!(normalized.mode, "transparent");
        assert_eq!(normalized.opacity, 100);
        assert_eq!(normalized.tint, "#233A63");
        let mut config = AppConfig::default();
        config.taskbar_appearance.tint = " #eaf1fc ".into();
        assert_eq!(config.normalized().taskbar_appearance.tint, "#EAF1FC");
    }
}
