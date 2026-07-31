use serde::{Deserialize, Serialize};

const MAX_GESTURE_TEMPLATES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub hotzones_enabled: bool,
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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            enabled: true,
            hotzones_enabled: true,
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
        }
    }
}

impl AppConfig {
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

        self.edge_hide.strip_size = self.edge_hide.strip_size.clamp(4, 64);
        self.edge_hide.trigger_distance = self.edge_hide.trigger_distance.clamp(4, 96);
        self.edge_hide.trigger_ratio = self.edge_hide.trigger_ratio.clamp(1, 100);
        self.edge_hide.collapse_delay_ms = self.edge_hide.collapse_delay_ms.clamp(0, 5000);
        self.edge_hide.restore_delay_ms = self.edge_hide.restore_delay_ms.clamp(0, 5000);
        self.edge_hide.excluded_apps = normalized_string_list(self.edge_hide.excluded_apps);
        self.mouse_gestures.min_distance = self.mouse_gestures.min_distance.clamp(12, 240);
        self.mouse_gestures.sensitivity = self.mouse_gestures.sensitivity.clamp(35, 95);
        self.mouse_gestures.paused_apps = normalized_string_list(self.mouse_gestures.paused_apps);
        self.mouse_gestures.gestures = normalize_gestures(self.mouse_gestures.gestures);
        self.window_drag.move_modifiers = normalized_modifiers(self.window_drag.move_modifiers);
        self.window_drag.resize_modifiers = normalized_modifiers(self.window_drag.resize_modifiers);
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
    pub trigger: Option<TriggerKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<HotzoneAction>,
}

impl HotzoneSetting {
    fn normalize_actions(&mut self) {
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
            if item.action.kind == ActionKind::VolumeAdjust && item.cooldown_ms.is_none() {
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
    #[serde(default = "default_drag_modifiers")]
    pub move_modifiers: Vec<ModifierKey>,
    #[serde(default = "default_drag_modifiers")]
    pub resize_modifiers: Vec<ModifierKey>,
    #[serde(default)]
    pub move_button: MouseButton,
    #[serde(default = "default_resize_button")]
    pub resize_button: MouseButton,
}

impl Default for WindowDragConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            move_modifiers: default_drag_modifiers(),
            resize_modifiers: default_drag_modifiers(),
            move_button: MouseButton::Left,
            resize_button: default_resize_button(),
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrConfig {
    #[serde(default)]
    pub language: OcrLanguage,
    #[serde(default)]
    pub screenshot_result: ScreenshotResultMode,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeHideConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub show_preview: bool,
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
            enabled: false,
            show_preview: true,
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
    7
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
            trigger: None,
            action: None,
        })
        .collect()
}

fn default_gestures() -> Vec<GestureTemplate> {
    vec![
        builtin_gesture(
            "gesture-up",
            "向上 · 复制",
            HotzoneAction {
                kind: ActionKind::Shortcut,
                value: Some("Ctrl+C".into()),
            },
            &[(0.5, 0.9), (0.5, 0.1)],
            GestureMode::Action,
        ),
        builtin_gesture(
            "gesture-down",
            "向下 · 粘贴",
            HotzoneAction {
                kind: ActionKind::Shortcut,
                value: Some("Ctrl+V".into()),
            },
            &[(0.5, 0.1), (0.5, 0.9)],
            GestureMode::Action,
        ),
        builtin_gesture(
            "gesture-l",
            "L 型 · 关闭窗口",
            HotzoneAction {
                kind: ActionKind::Shortcut,
                value: Some("Alt+F4".into()),
            },
            &[(0.2, 0.1), (0.2, 0.85), (0.85, 0.85)],
            GestureMode::Action,
        ),
        builtin_gesture(
            "gesture-circle",
            "圆圈 · 切换窗口置顶",
            HotzoneAction {
                kind: ActionKind::ToggleWindowTopmost,
                value: None,
            },
            &circle_points(),
            GestureMode::Action,
        ),
        builtin_gesture(
            "gesture-rectangle",
            "矩形截图",
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
    name: &str,
    action: HotzoneAction,
    points: &[(f32, f32)],
    mode: GestureMode,
) -> GestureTemplate {
    GestureTemplate {
        id: id.into(),
        name: name.into(),
        enabled: true,
        builtin: true,
        mode,
        action,
        modifier_actions: Vec::new(),
        samples: vec![points.iter().map(|&(x, y)| GesturePoint { x, y }).collect()],
    }
}

fn normalize_gestures(values: Vec<GestureTemplate>) -> Vec<GestureTemplate> {
    let defaults = default_gestures();
    let mut normalized = Vec::new();
    for mut gesture in values.into_iter().take(MAX_GESTURE_TEMPLATES) {
        gesture.id = gesture.id.trim().chars().take(80).collect();
        gesture.name = gesture.name.trim().chars().take(40).collect();
        if gesture.id.is_empty()
            || gesture.name.is_empty()
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

fn normalized_modifiers(values: Vec<ModifierKey>) -> Vec<ModifierKey> {
    ModifierKey::all()
        .into_iter()
        .filter(|key| values.contains(key))
        .collect()
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
            "edgeSize": config.edge_size,
            "hoverDelayMs": config.hover_delay_ms,
            "pollIntervalMs": config.poll_interval_ms,
            "actionCooldownMs": config.action_cooldown_ms,
            "pausedApps": config.paused_apps,
            "stripSize": config.edge_hide.strip_size,
            "edgeHidePreviewEnabled": config.edge_hide.show_preview,
            "triggerDistance": config.edge_hide.trigger_distance,
            "triggerRatio": config.edge_hide.trigger_ratio,
            "collapseDelayMs": config.edge_hide.collapse_delay_ms,
            "restoreDelayMs": config.edge_hide.restore_delay_ms,
            "minDistance": config.mouse_gestures.min_distance,
            "sensitivity": config.mouse_gestures.sensitivity,
            "gesturePausedApps": config.mouse_gestures.paused_apps,
            "windowDragEnabled": config.window_drag.enabled,
            "moveModifiers": config.window_drag.move_modifiers,
            "resizeModifiers": config.window_drag.resize_modifiers,
            "topmostPinEnabled": config.topmost_pin.enabled,
            "ocrLanguage": config.ocr.language,
            "screenshotResult": config.ocr.screenshot_result
        });
        assert_eq!(actual, fixture["expected"]);
    }

    #[test]
    fn config_deserializes_with_defaults() {
        let config: AppConfig = serde_json::from_str(r#"{"enabled":true}"#).unwrap();

        assert!(config.enabled);
        assert!(config.hotzones_enabled);
        assert_eq!(config.edge_size, 8);
        assert_eq!(config.hotzones.len(), 8);
        assert_eq!(config.edge_hide.strip_size, 16);
        assert!(config.edge_hide.show_preview);
        assert!(config.edge_hide.distance_trigger_enabled);
        assert!(config.edge_hide.ratio_trigger_enabled);
        assert_eq!(config.edge_hide.trigger_ratio, 33);
        assert_eq!(config.edge_hide.collapse_delay_ms, 300);
        assert_eq!(config.edge_hide.restore_delay_ms, 200);
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

        let normalized = normalize_gestures(gestures);
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
