use crate::config::{ActionKind, AppConfig, HotzoneId, HotzoneSetting, TriggerKind};
use crate::core::actions::ActionDispatcher;
use crate::core::edge_hide::{EdgeHideCommand, EdgeHideController, EdgeHideInput};
use crate::core::gesture::{path_length_pixels, recognize};
use crate::core::hotzone::{detect_hotzone, hotzone_rect};
use crate::core::trigger::HotzoneTriggerController;
use crate::core::window_drag::WindowDragController;
use crate::ipc::messages::HelperMessage;
use crate::logging;
use crate::platform;
use crate::platform::InputState;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, watch};

pub struct Engine {
    config_rx: watch::Receiver<AppConfig>,
    event_tx: broadcast::Sender<HelperMessage>,
    shutdown_rx: broadcast::Receiver<()>,
    runtime_error_times: Mutex<HashMap<String, Instant>>,
}

#[derive(Clone, Copy, Debug, Default)]
struct WindowDragActivity {
    active: bool,
    target: Option<(platform::WindowHandle, platform::Rect)>,
}

impl Engine {
    pub fn new(
        config_rx: watch::Receiver<AppConfig>,
        event_tx: broadcast::Sender<HelperMessage>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            config_rx,
            event_tx,
            shutdown_rx,
            runtime_error_times: Mutex::new(HashMap::new()),
        }
    }

    pub async fn run(&self) -> Result<()> {
        logging::write_line("engine: starting");
        platform::install_mouse_hook()?;
        logging::write_line("engine: mouse hook requested");

        let dispatcher = ActionDispatcher::new(self.event_tx.clone());
        let mut config_rx = self.config_rx.clone();
        let mut config = config_rx.borrow_and_update().clone();
        let mut shutdown_rx = self.shutdown_rx.resubscribe();
        let mut previous_input = InputState::default();
        let mut previous_cursor = None;
        let mut foreground_tracker = ForegroundTracker::default();
        let mut hotzone_triggers = HotzoneTriggerController::new(Instant::now());
        let mut edge_hide = EdgeHideController::new();
        let mut window_drag = WindowDragController::default();
        let mut last_status_at = Instant::now() - Duration::from_secs(5);
        let mut monitor_refresh = RefreshSchedule::new(Duration::from_secs(1));
        let mut window_prune = RefreshSchedule::new(Duration::from_secs(1));
        let mut cached_monitors = Vec::new();
        let mut runtime_failure = None;
        #[cfg(debug_assertions)]
        let injected_failure_at = injected_engine_failure_deadline();

        loop {
            #[cfg(debug_assertions)]
            if injected_failure_at.is_some_and(|deadline| Instant::now() >= deadline) {
                runtime_failure = Some("simulated engine failure".to_string());
                break;
            }
            if !platform::mouse_hook_is_healthy() {
                runtime_failure = Some("mouse hook thread stopped unexpectedly".to_string());
                break;
            }
            if let Some(error) = platform::ocr_worker_error() {
                runtime_failure = Some(error);
                break;
            }
            let now = Instant::now();
            platform::configure_topmost_pins(config.enabled && config.topmost_pin.enabled);
            platform::configure_window_drag_capture(
                config.enabled && config.window_drag.enabled,
                config.window_drag.move_button,
                modifier_config_mask(&config.window_drag.move_modifiers),
                config.window_drag.resize_button,
                modifier_config_mask(&config.window_drag.resize_modifiers),
            );
            let input = platform::input_state();

            if config.enabled {
                edge_hide.prune_invalid_windows(platform::window_exists);
                let cursor = platform::cursor_position();
                let monitors = refresh_monitors(now, &mut monitor_refresh, &mut cached_monitors);
                let foreground = platform::foreground_window();

                match (cursor, monitors, foreground) {
                    (Ok(cursor), Ok(monitors), Ok(foreground)) => {
                        let foreground_handle = foreground.as_ref().map(|window| window.handle);
                        let suppress_foreground_restore = foreground_tracker.observe(
                            now,
                            foreground_handle,
                            platform::window_is_minimized,
                        );
                        if window_prune.should_refresh(now) {
                            edge_hide.prune_invalid_windows(platform::window_exists);
                        }
                        if last_status_at.elapsed() >= Duration::from_secs(2) {
                            let _ = self.event_tx.send(HelperMessage::new(
                                "runtime.status",
                                json!({
                                    "monitors": monitors.len(),
                                    "foreground": foreground.as_ref().map(|window| &window.process_name),
                                    "windowDragEnabled": config.window_drag.enabled,
                                    "displays": monitors.iter().map(|monitor| json!({
                                        "id": monitor_id(monitor),
                                        "legacyId": monitor.legacy_id(),
                                        "primary": monitor.primary,
                                        "bounds": {
                                            "left": monitor.bounds.left,
                                            "top": monitor.bounds.top,
                                            "right": monitor.bounds.right,
                                            "bottom": monitor.bounds.bottom
                                        },
                                        "workArea": {
                                            "left": monitor.work_area.left,
                                            "top": monitor.work_area.top,
                                            "right": monitor.work_area.right,
                                            "bottom": monitor.work_area.bottom
                                        }
                                    })).collect::<Vec<_>>(),
                                }),
                            ));
                            last_status_at = now;
                        }

                        let drag_capture = platform::take_window_drag_capture();
                        let drag_activity = self.handle_window_drag(
                            &config,
                            &mut window_drag,
                            drag_capture,
                            input.escape_down && !previous_input.escape_down,
                        );
                        let dragging = drag_activity.active;
                        let paused = foreground
                            .as_ref()
                            .is_some_and(|window| is_paused_app(&config.paused_apps, window));
                        let gestures_paused = foreground.as_ref().is_some_and(|window| {
                            is_paused_app(&config.mouse_gestures.paused_apps, window)
                                || (config.mouse_gestures.fullscreen_pause
                                    && is_fullscreen_window(window, monitors))
                        });
                        platform::configure_gesture_capture(
                            config.mouse_gestures.enabled && !gestures_paused && !dragging,
                            config.mouse_gestures.trigger_button,
                        );
                        let gesture_points = platform::active_gesture_points();
                        if config.mouse_gestures.show_trail && !gesture_points.is_empty() {
                            let label = recognize(&gesture_points, &config.mouse_gestures)
                                .map(|result| result.gesture.name.as_str());
                            platform::update_gesture_overlay(&gesture_points, label);
                        } else {
                            platform::hide_gesture_overlay();
                        }

                        let hint_rect =
                            hotzone_hint_for(&config, cursor, monitors, paused || dragging)
                                .and_then(|id| {
                                    monitors
                                        .iter()
                                        .find(|monitor| monitor.bounds.contains(cursor))
                                        .map(|monitor| {
                                            hotzone_rect(id, monitor.bounds, config.edge_size)
                                        })
                                });
                        platform::update_hotzone_hints(hint_rect);

                        self.handle_hotzone(
                            now,
                            &config,
                            &dispatcher,
                            cursor,
                            monitors,
                            input,
                            previous_input,
                            previous_cursor,
                            paused || dragging,
                            &mut hotzone_triggers,
                        );
                        self.handle_mouse_gestures(
                            &config,
                            &dispatcher,
                            config.mouse_gestures.enabled && !gestures_paused && !dragging,
                        );

                        if !dragging {
                            self.handle_edge_hide(
                                &config,
                                &mut edge_hide,
                                now,
                                cursor,
                                monitors,
                                foreground.as_ref(),
                                EdgeHideInput {
                                    left_button_down: input.left_down,
                                    right_button_pressed: input.right_clicked_since(previous_input),
                                    context_menu_dismissed: input
                                        .context_menu_dismissed_since(previous_input),
                                    suppress_foreground_restore,
                                },
                            );
                        }
                        let edge_hide_preview = if let Some((handle, rect)) = drag_activity.target {
                            match platform::window_info_for_handle(handle) {
                                Ok(Some(window)) => edge_hide.collapse_preview(
                                    &config.edge_hide,
                                    monitors,
                                    &window,
                                    rect,
                                ),
                                Ok(None) => None,
                                Err(error) => {
                                    self.report_runtime_error(error);
                                    None
                                }
                            }
                        } else if !dragging
                            && native_drag_preview_started(input.left_down, cursor, previous_cursor)
                        {
                            foreground.as_ref().and_then(|window| {
                                edge_hide.collapse_preview(
                                    &config.edge_hide,
                                    monitors,
                                    window,
                                    window.rect,
                                )
                            })
                        } else {
                            None
                        };
                        platform::update_edge_hide_preview(edge_hide_preview);
                        platform::update_strip_hints(&edge_hide.collapsed_strips_with_live_state(
                            monitors,
                            |handle| {
                                if platform::window_is_minimized(handle) {
                                    return Some((None, true));
                                }
                                match platform::window_info_for_handle(handle) {
                                    Ok(Some(window)) => Some((Some(window.rect), false)),
                                    Ok(None) | Err(_) => None,
                                }
                            },
                        ));
                        previous_cursor = Some(cursor);
                    }
                    (cursor, monitors, foreground) => {
                        platform::configure_gesture_capture(
                            false,
                            config.mouse_gestures.trigger_button,
                        );
                        platform::update_edge_hide_preview(None);
                        platform::cancel_window_drag_capture();
                        if let Some((handle, rect)) = window_drag.cancel() {
                            let _ = platform::set_window_rect(handle, rect);
                        }
                        platform::hide_gesture_overlay();
                        self.handle_mouse_gestures(&config, &dispatcher, false);
                        hotzone_triggers.suspend(now);
                        previous_cursor = None;
                        platform::hide_hotzone_hints();
                        self.report_runtime_error_data(json!({
                            "cursor": cursor.err().map(|error| error.to_string()),
                            "monitors": monitors.err().map(|error| error.to_string()),
                            "foreground": foreground.err().map(|error| error.to_string()),
                        }));
                    }
                }
            } else {
                platform::configure_topmost_pins(false);
                platform::configure_window_drag_capture(
                    false,
                    config.window_drag.move_button,
                    0,
                    config.window_drag.resize_button,
                    0,
                );
                platform::update_edge_hide_preview(None);
                if let Some((handle, rect)) = window_drag.cancel() {
                    let _ = platform::set_window_rect(handle, rect);
                }
                platform::configure_gesture_capture(false, config.mouse_gestures.trigger_button);
                platform::hide_gesture_overlay();
                self.handle_mouse_gestures(&config, &dispatcher, false);
                hotzone_triggers.suspend(now);
                previous_cursor = None;
                foreground_tracker.reset();
                platform::hide_hotzone_hints();
                self.restore_edge_hide_if_needed(&mut edge_hide);
            }
            self.handle_ocr_completions();
            previous_input = input;
            let interval = engine_poll_interval(config.poll_interval_ms, window_drag.is_active());

            tokio::select! {
                _ = tokio::time::sleep(interval) => {}
                changed = config_rx.changed() => {
                    if changed.is_ok() {
                        config = config_rx.borrow_and_update().clone();
                    }
                }
                _ = shutdown_rx.recv() => break,
            }
        }
        platform::configure_topmost_pins(false);
        platform::clear_topmost_pins();
        platform::configure_window_drag_capture(
            false,
            config.window_drag.move_button,
            0,
            config.window_drag.resize_button,
            0,
        );
        platform::update_edge_hide_preview(None);
        if let Some((handle, rect)) = window_drag.cancel() {
            let _ = platform::set_window_rect(handle, rect);
        }
        platform::configure_gesture_capture(false, config.mouse_gestures.trigger_button);
        platform::hide_gesture_overlay();
        platform::hide_hotzone_hints();
        self.restore_edge_hide_if_needed(&mut edge_hide);
        platform::stop_mouse_hook();
        logging::write_line("engine: stopped");

        if let Some(error) = runtime_failure {
            bail!(error);
        }
        Ok(())
    }

    fn handle_ocr_completions(&self) {
        while let Some(completion) = platform::take_ocr_completion() {
            match completion {
                platform::OcrCompletion::Copied(characters) => {
                    let _ = self.event_tx.send(HelperMessage::new(
                        "ocr.completed",
                        json!({ "characters": characters }),
                    ));
                }
                platform::OcrCompletion::Failed(message) => {
                    self.report_runtime_error(anyhow::anyhow!("文字识别失败：{message}"));
                }
            }
        }
    }

    fn handle_window_drag(
        &self,
        config: &AppConfig,
        controller: &mut WindowDragController,
        capture: Option<platform::WindowDragCapture>,
        cancel: bool,
    ) -> WindowDragActivity {
        if !config.window_drag.enabled {
            platform::cancel_window_drag_capture();
            if let Some((handle, rect)) = controller.cancel() {
                if let Err(error) = platform::set_window_rect(handle, rect) {
                    self.report_runtime_error(error);
                }
            }
            return WindowDragActivity::default();
        }
        let Some(capture) = capture else {
            return WindowDragActivity::default();
        };
        if controller.sequence() != Some(capture.sequence) {
            match platform::draggable_window_at(capture.start) {
                Ok(Some(window)) => controller.start(capture, &window),
                Ok(None) => {
                    platform::cancel_window_drag_capture();
                    return WindowDragActivity::default();
                }
                Err(error) => {
                    platform::cancel_window_drag_capture();
                    self.report_runtime_error(error);
                    return WindowDragActivity::default();
                }
            }
        }
        if cancel {
            platform::cancel_window_drag_capture();
        }
        let Some(update) = controller.update(capture, cancel) else {
            return WindowDragActivity::default();
        };
        if let Err(error) = platform::set_window_rect(update.handle, update.rect) {
            platform::cancel_window_drag_capture();
            let _ = controller.cancel();
            self.report_runtime_error(error);
            return WindowDragActivity::default();
        }
        if update.finished {
            let _ = self.event_tx.send(HelperMessage::new(
                "action.triggered",
                json!({
                    "source": if update.cancelled { "window-drag.cancel" } else { "window-drag.commit" },
                    "rect": {
                        "left": update.rect.left,
                        "top": update.rect.top,
                        "right": update.rect.right,
                        "bottom": update.rect.bottom
                    }
                }),
            ));
        }
        WindowDragActivity {
            active: true,
            target: (!update.finished).then_some((update.handle, update.rect)),
        }
    }

    fn handle_mouse_gestures(
        &self,
        config: &AppConfig,
        dispatcher: &ActionDispatcher,
        allow_execution: bool,
    ) {
        while let Some(capture) = platform::take_gesture_capture() {
            let length = path_length_pixels(&capture.points);
            if length < config.mouse_gestures.min_distance as f32 {
                if let Err(error) = platform::send_trigger_click(capture.trigger) {
                    self.report_runtime_error(error);
                }
                continue;
            }
            if !allow_execution {
                continue;
            }
            let Some(result) = recognize(&capture.points, &config.mouse_gestures) else {
                let _ = self.event_tx.send(HelperMessage::new(
                    "runtime.status",
                    json!({ "message": "手势未识别，已取消" }),
                ));
                continue;
            };
            let source = format!("gesture/{}", result.gesture.id);
            let dispatch_result = if let Some(region) = result.region {
                platform::hide_gesture_overlay_before_capture();
                platform::capture_and_pin(region, capture.points.last().copied(), &config.ocr)
            } else {
                dispatcher.dispatch_at_with_modifiers(
                    result.gesture.action_for_modifier_mask(capture.modifiers),
                    &source,
                    capture.points.first().copied(),
                    capture.modifiers,
                )
            };
            match dispatch_result {
                Ok(()) => {
                    let _ = self.event_tx.send(HelperMessage::new(
                        "gesture.recognized",
                        json!({
                            "id": result.gesture.id,
                            "name": result.gesture.name,
                            "score": result.score,
                            "region": result.region.map(|rect| json!({
                                "left": rect.left, "top": rect.top, "right": rect.right, "bottom": rect.bottom
                            }))
                        }),
                    ));
                }
                Err(error) => self.report_runtime_error(error),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_hotzone(
        &self,
        now: Instant,
        config: &AppConfig,
        dispatcher: &ActionDispatcher,
        cursor: platform::Point,
        monitors: &[platform::Monitor],
        input: InputState,
        previous_input: InputState,
        previous_cursor: Option<platform::Point>,
        paused: bool,
        hotzone_triggers: &mut HotzoneTriggerController,
    ) {
        if paused || !config.hotzones_enabled {
            hotzone_triggers.suspend(now);
            return;
        }

        let detected = detect_hotzone(cursor, monitors, config.edge_size);
        hotzone_triggers.observe(now, detected);

        let Some(hotzone_id) = detected else {
            return;
        };

        let Some(setting) = hotzones_for_cursor(config, cursor, monitors)
            .iter()
            .find(|item| item.id == hotzone_id && item.has_action())
        else {
            hotzone_triggers.suspend(now);
            return;
        };

        let slide_motion = previous_cursor
            .map(|previous| slide_motion(hotzone_id, cursor, previous))
            .unwrap_or_default();
        hotzone_triggers.record_slide_motion(slide_motion);
        for trigger_action in &setting.actions {
            let modifiers = trigger_modifier_mask(trigger_action.trigger, input, previous_input);
            let action = trigger_action.action_for_modifier_mask(modifiers);
            if action.kind == ActionKind::None {
                continue;
            }
            let cooldown = Duration::from_millis(
                trigger_action
                    .cooldown_ms
                    .unwrap_or(config.action_cooldown_ms)
                    .clamp(10, 5000),
            );
            if is_continuous_volume_trigger(action, trigger_action.trigger) {
                let input_scale = continuous_action_scale(
                    action,
                    trigger_action.trigger,
                    input,
                    previous_input,
                    slide_motion,
                )
                .unwrap_or_default();
                let Some(scale) = hotzone_triggers.accumulate_continuous_motion(
                    now,
                    hotzone_id,
                    trigger_action.trigger,
                    input_scale,
                    cooldown,
                ) else {
                    continue;
                };
                if matches!(
                    trigger_action.trigger,
                    TriggerKind::SlideForward | TriggerKind::SlideBackward
                ) {
                    hotzone_triggers.clear_slide_motion();
                }
                if let Err(error) = dispatcher.dispatch_scaled_with_modifiers(
                    action,
                    &format!("{:?}/{:?}", hotzone_id, trigger_action.trigger),
                    scale,
                    modifiers,
                ) {
                    self.report_runtime_error(error);
                }
                continue;
            }

            if !hotzone_triggers.should_trigger(
                now,
                hotzone_id,
                trigger_action.trigger,
                input,
                previous_input,
                Duration::from_millis(
                    trigger_action
                        .hover_delay_ms
                        .unwrap_or(config.hover_delay_ms)
                        .clamp(0, 3000),
                ),
                cooldown,
            ) {
                continue;
            }
            if let Err(error) = dispatcher.dispatch_with_modifiers(
                action,
                &format!("{:?}/{:?}", hotzone_id, trigger_action.trigger),
                modifiers,
            ) {
                self.report_runtime_error(error);
            }
        }
    }

    fn report_runtime_error(&self, error: anyhow::Error) {
        self.report_runtime_error_data(json!({
            "message": error.to_string()
        }));
    }

    fn report_runtime_error_data(&self, data: Value) {
        const ERROR_THROTTLE: Duration = Duration::from_secs(2);
        let key = data.to_string();
        let now = Instant::now();
        if let Ok(mut reported) = self.runtime_error_times.lock() {
            if reported
                .get(&key)
                .is_some_and(|last| now.saturating_duration_since(*last) < ERROR_THROTTLE)
            {
                return;
            }
            reported
                .retain(|_, last| now.saturating_duration_since(*last) < Duration::from_secs(60));
            reported.insert(key, now);
        }
        let _ = self
            .event_tx
            .send(HelperMessage::new("runtime.error", data));
    }

    fn handle_edge_hide(
        &self,
        config: &AppConfig,
        edge_hide: &mut EdgeHideController,
        now: Instant,
        cursor: platform::Point,
        monitors: &[platform::Monitor],
        foreground: Option<&platform::WindowInfo>,
        input: EdgeHideInput,
    ) {
        if let Some(command) =
            edge_hide.tick_with_input(now, &config.edge_hide, cursor, monitors, foreground, input)
        {
            let result = match command {
                EdgeHideCommand::Collapse { handle, rect } => {
                    platform::set_window_rect_topmost(handle, rect, true)
                        .map(|_| ("edge-hide.collapse", rect))
                }
                EdgeHideCommand::Restore {
                    handle,
                    rect,
                    topmost,
                } => platform::set_window_rect_topmost(handle, rect, topmost)
                    .map(|_| ("edge-hide.restore", rect)),
            };

            match result {
                Ok((kind, rect)) => {
                    let _ = self.event_tx.send(HelperMessage::new(
                        "action.triggered",
                        json!({
                            "source": kind,
                            "rect": {
                                "left": rect.left,
                                "top": rect.top,
                                "right": rect.right,
                                "bottom": rect.bottom
                            }
                        }),
                    ));
                }
                Err(error) => {
                    edge_hide.command_failed(command);
                    self.report_runtime_error(error);
                }
            }
        }
    }

    fn restore_edge_hide_if_needed(&self, edge_hide: &mut EdgeHideController) {
        while let Some(EdgeHideCommand::Restore {
            handle,
            rect,
            topmost,
        }) = edge_hide.restore_if_needed()
        {
            if let Err(error) = platform::set_window_rect_topmost(handle, rect, topmost) {
                self.report_runtime_error(error);
            }
        }
    }
}

#[cfg(debug_assertions)]
fn injected_engine_failure_deadline() -> Option<Instant> {
    let delay_ms = std::env::var("MAGIC_CORNERS_TEST_ENGINE_FAILURE_MS")
        .ok()?
        .parse::<u64>()
        .ok()?
        .clamp(50, 60_000);
    Some(Instant::now() + Duration::from_millis(delay_ms))
}

fn engine_poll_interval(configured_ms: u64, dragging: bool) -> Duration {
    let configured_ms = configured_ms.max(10);
    Duration::from_millis(if dragging {
        configured_ms.min(16)
    } else {
        configured_ms
    })
}

const MINIMIZED_FOREGROUND_GUARD: Duration = Duration::from_millis(250);

#[derive(Default)]
struct ForegroundTracker {
    previous: Option<platform::WindowHandle>,
    passive_guard_until: Option<Instant>,
}

impl ForegroundTracker {
    fn observe(
        &mut self,
        now: Instant,
        current: Option<platform::WindowHandle>,
        mut is_minimized: impl FnMut(platform::WindowHandle) -> bool,
    ) -> bool {
        if self
            .passive_guard_until
            .is_some_and(|deadline| now >= deadline)
        {
            self.passive_guard_until = None;
        }

        if let (Some(previous), Some(current)) = (self.previous, current) {
            if previous != current && is_minimized(previous) {
                self.passive_guard_until = Some(now + MINIMIZED_FOREGROUND_GUARD);
            }
        }

        self.observe_current(current);
        self.passive_guard_until
            .is_some_and(|deadline| now < deadline)
    }

    fn observe_current(&mut self, current: Option<platform::WindowHandle>) {
        if current.is_some() {
            self.previous = current;
        }
    }

    fn reset(&mut self) {
        self.previous = None;
        self.passive_guard_until = None;
    }
}

struct RefreshSchedule {
    interval: Duration,
    last_refresh: Option<Instant>,
}

impl RefreshSchedule {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_refresh: None,
        }
    }

    fn should_refresh(&mut self, now: Instant) -> bool {
        let due = self
            .last_refresh
            .is_none_or(|last| now.saturating_duration_since(last) >= self.interval);
        if due {
            self.last_refresh = Some(now);
        }
        due
    }
}

fn refresh_monitors<'a>(
    now: Instant,
    schedule: &mut RefreshSchedule,
    cached: &'a mut Vec<platform::Monitor>,
) -> anyhow::Result<&'a [platform::Monitor]> {
    let refresh_due = schedule.should_refresh(now);
    if cached.is_empty() || refresh_due {
        *cached = platform::monitors()?;
    }
    Ok(cached)
}

fn is_paused_app(paused_apps: &[String], window: &platform::WindowInfo) -> bool {
    let process = window.process_name.to_lowercase();
    let title = window.title.to_lowercase();
    let class_name = window.class_name.to_lowercase();

    paused_apps.iter().any(|item| {
        let item = item.trim().to_lowercase();
        !item.is_empty()
            && (process.contains(&item) || title.contains(&item) || class_name.contains(&item))
    })
}

fn is_fullscreen_window(window: &platform::WindowInfo, monitors: &[platform::Monitor]) -> bool {
    let class_name = window.class_name.to_ascii_lowercase();
    if window.maximized || matches!(class_name.as_str(), "progman" | "workerw") {
        return false;
    }
    monitors.iter().any(|monitor| {
        let rect = window.rect;
        let bounds = monitor.bounds;
        (rect.left - bounds.left).abs() <= 2
            && (rect.top - bounds.top).abs() <= 2
            && (rect.right - bounds.right).abs() <= 2
            && (rect.bottom - bounds.bottom).abs() <= 2
    })
}

fn hotzone_hint_for(
    config: &AppConfig,
    cursor: platform::Point,
    monitors: &[platform::Monitor],
    paused: bool,
) -> Option<HotzoneId> {
    if paused || !config.hotzones_enabled {
        return None;
    }

    let detected = detect_hotzone(cursor, monitors, config.edge_size)?;
    hotzones_for_cursor(config, cursor, monitors)
        .iter()
        .find(|item| item.id == detected && item.has_action())
        .map(|item| item.id)
}

fn native_drag_preview_started(
    left_button_down: bool,
    cursor: platform::Point,
    previous_cursor: Option<platform::Point>,
) -> bool {
    left_button_down && previous_cursor.is_some_and(|previous| previous != cursor)
}

fn hotzones_for_cursor<'a>(
    config: &'a AppConfig,
    cursor: platform::Point,
    monitors: &[platform::Monitor],
) -> &'a [HotzoneSetting] {
    let Some(monitor) = monitors
        .iter()
        .find(|monitor| monitor.bounds.contains(cursor))
    else {
        return &config.hotzones;
    };
    let id = monitor_id(monitor);
    config
        .monitor_profiles
        .iter()
        .find(|profile| profile.monitor_id == id)
        .map(|profile| profile.hotzones.as_slice())
        .unwrap_or(&config.hotzones)
}

fn monitor_id(monitor: &platform::Monitor) -> String {
    monitor.id()
}

fn slide_motion(id: HotzoneId, cursor: platform::Point, previous: platform::Point) -> i32 {
    match id {
        HotzoneId::Top | HotzoneId::Bottom => cursor.x - previous.x,
        HotzoneId::Left | HotzoneId::Right => cursor.y - previous.y,
        _ => 0,
    }
}

fn modifier_config_mask(modifiers: &[crate::config::ModifierKey]) -> u8 {
    modifiers.iter().fold(0, |mask, key| mask | key.mask())
}

fn trigger_modifier_mask(
    trigger: TriggerKind,
    input: InputState,
    previous_input: InputState,
) -> u8 {
    match trigger {
        TriggerKind::LeftClick => input
            .left_click_modifier_mask_since(previous_input)
            .unwrap_or(input.modifiers),
        TriggerKind::RightClick => input
            .right_click_modifier_mask_since(previous_input)
            .unwrap_or(input.modifiers),
        TriggerKind::WheelUp | TriggerKind::WheelDown => input
            .wheel_modifier_mask_since(previous_input)
            .unwrap_or(input.modifiers),
        _ => input.modifiers,
    }
}

fn continuous_action_scale(
    action: &crate::config::HotzoneAction,
    trigger: crate::config::TriggerKind,
    input: InputState,
    previous_input: InputState,
    slide_motion: i32,
) -> Option<f32> {
    if action.kind != ActionKind::VolumeAdjust {
        return None;
    }

    let wheel_motion = input.wheel_delta - previous_input.wheel_delta;
    match trigger {
        crate::config::TriggerKind::WheelUp if wheel_motion > 0 => {
            Some(wheel_motion as f32 / 120.0)
        }
        crate::config::TriggerKind::WheelDown if wheel_motion < 0 => {
            Some(wheel_motion.unsigned_abs() as f32 / 120.0)
        }
        crate::config::TriggerKind::SlideForward if slide_motion > 0 => {
            Some(slide_motion as f32 / 16.0)
        }
        crate::config::TriggerKind::SlideBackward if slide_motion < 0 => {
            Some(slide_motion.unsigned_abs() as f32 / 16.0)
        }
        _ => None,
    }
}

fn is_continuous_volume_trigger(
    action: &crate::config::HotzoneAction,
    trigger: TriggerKind,
) -> bool {
    action.kind == ActionKind::VolumeAdjust
        && matches!(
            trigger,
            TriggerKind::WheelUp
                | TriggerKind::WheelDown
                | TriggerKind::SlideForward
                | TriggerKind::SlideBackward
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ActionKind, HotzoneAction, HotzoneSetting, MonitorProfile, TriggerAction, TriggerKind,
    };
    use crate::platform::{Monitor, Point, Rect};

    fn monitor() -> Monitor {
        Monitor {
            bounds: Rect {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
            work_area: Rect {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
            primary: true,
            device_id: [0; 128],
        }
    }

    #[test]
    fn active_drag_uses_a_sixteen_millisecond_ceiling_without_changing_idle_polling() {
        assert_eq!(engine_poll_interval(33, false), Duration::from_millis(33));
        assert_eq!(engine_poll_interval(33, true), Duration::from_millis(16));
        assert_eq!(engine_poll_interval(10, true), Duration::from_millis(10));
        assert_eq!(engine_poll_interval(250, false), Duration::from_millis(250));
    }

    #[test]
    fn native_drag_preview_requires_pointer_motion() {
        assert!(!native_drag_preview_started(
            true,
            Point { x: 10, y: 10 },
            Some(Point { x: 10, y: 10 }),
        ));
        assert!(!native_drag_preview_started(
            true,
            Point { x: 10, y: 10 },
            None,
        ));
        assert!(native_drag_preview_started(
            true,
            Point { x: 11, y: 10 },
            Some(Point { x: 10, y: 10 }),
        ));
        assert!(!native_drag_preview_started(
            false,
            Point { x: 11, y: 10 },
            Some(Point { x: 10, y: 10 }),
        ));
    }

    #[test]
    fn minimized_foreground_fallback_guard_survives_an_intermediate_window() {
        let start = Instant::now();
        let collapsed = platform::WindowHandle(1);
        let intermediate = platform::WindowHandle(2);
        let target = platform::WindowHandle(3);
        let mut tracker = ForegroundTracker::default();

        assert!(!tracker.observe(start, Some(collapsed), |_| false));
        assert!(tracker.observe(
            start + Duration::from_millis(8),
            Some(intermediate),
            |handle| handle == collapsed,
        ));
        assert!(tracker.observe(start + Duration::from_millis(16), Some(target), |_| false,));
        assert!(!tracker.observe(start + Duration::from_millis(260), Some(target), |_| false,));
    }

    #[test]
    fn minimized_foreground_fallback_guard_handles_a_transient_empty_foreground() {
        let start = Instant::now();
        let collapsed = platform::WindowHandle(1);
        let target = platform::WindowHandle(2);
        let mut tracker = ForegroundTracker::default();

        assert!(!tracker.observe(start, Some(collapsed), |_| false));
        assert!(!tracker.observe(start + Duration::from_millis(8), None, |_| true));
        assert!(tracker.observe(
            start + Duration::from_millis(16),
            Some(target),
            |handle| handle == collapsed,
        ));
    }

    #[test]
    fn foreground_tracker_does_not_guard_without_a_minimized_previous_window() {
        let start = Instant::now();
        let previous = platform::WindowHandle(1);
        let current = platform::WindowHandle(2);
        let mut tracker = ForegroundTracker::default();

        assert!(!tracker.observe(start, Some(previous), |_| false));
        assert!(!tracker.observe(start + Duration::from_millis(8), Some(current), |_| false,));
        tracker.reset();
        assert!(!tracker.observe(start + Duration::from_millis(16), Some(current), |_| true));
    }

    #[test]
    fn foreground_tracker_keeps_the_latest_non_empty_foreground_baseline() {
        let start = Instant::now();
        let previous = platform::WindowHandle(1);
        let current = platform::WindowHandle(2);
        let mut tracker = ForegroundTracker::default();

        assert!(!tracker.observe(start, Some(previous), |_| false));
        assert!(!tracker.observe(start + Duration::from_millis(8), None, |_| true));
        assert!(!tracker.observe(start + Duration::from_millis(16), Some(previous), |_| false));
        assert!(!tracker.observe(start + Duration::from_millis(24), Some(current), |_| false,));
    }

    #[test]
    fn monitor_refresh_is_rate_limited_but_topology_stays_fresh() {
        let start = Instant::now();
        let mut refresh = RefreshSchedule::new(Duration::from_secs(1));

        assert!(refresh.should_refresh(start));
        assert!(!refresh.should_refresh(start + Duration::from_millis(999)));
        assert!(refresh.should_refresh(start + Duration::from_secs(1)));
        assert!(!refresh.should_refresh(start + Duration::from_millis(1500)));
        assert!(refresh.should_refresh(start + Duration::from_secs(2)));
    }

    fn hotzone(id: HotzoneId, enabled: bool, kind: ActionKind) -> HotzoneSetting {
        HotzoneSetting {
            id,
            enabled,
            actions: vec![TriggerAction {
                trigger: TriggerKind::Hover,
                action: HotzoneAction { kind, value: None },
                modifier_actions: Vec::new(),
                cooldown_ms: None,
                hover_delay_ms: None,
            }],
            trigger: None,
            action: None,
        }
    }

    #[test]
    fn hint_only_appears_when_hotzones_are_enabled_and_have_an_action() {
        let mut config = AppConfig::default();
        config.hotzones_enabled = false;
        config.hotzones = vec![hotzone(HotzoneId::TopRight, true, ActionKind::ShowDesktop)];

        assert_eq!(
            hotzone_hint_for(&config, Point { x: 99, y: 1 }, &[monitor()], false),
            None
        );

        config.hotzones_enabled = true;
        config.hotzones = vec![hotzone(HotzoneId::TopRight, false, ActionKind::None)];
        assert_eq!(
            hotzone_hint_for(&config, Point { x: 99, y: 1 }, &[monitor()], false),
            None
        );

        config.hotzones = vec![hotzone(HotzoneId::TopRight, false, ActionKind::ShowDesktop)];
        assert_eq!(
            hotzone_hint_for(&config, Point { x: 99, y: 1 }, &[monitor()], false),
            Some(HotzoneId::TopRight)
        );
    }

    #[test]
    fn hint_only_appears_inside_the_action_hotzone() {
        let mut config = AppConfig::default();
        config.edge_size = 8;
        config.hotzones = vec![hotzone(HotzoneId::TopRight, true, ActionKind::ShowDesktop)];
        let outside = Point { x: 70, y: 20 };
        let inside = Point { x: 95, y: 5 };

        assert_eq!(
            detect_hotzone(outside, &[monitor()], config.edge_size),
            None
        );
        assert_eq!(
            hotzone_hint_for(&config, outside, &[monitor()], false),
            None
        );
        assert_eq!(
            hotzone_hint_for(&config, inside, &[monitor()], false),
            Some(HotzoneId::TopRight)
        );
    }

    #[test]
    fn monitor_profile_overrides_global_hotzones_for_that_display() {
        let display = monitor();
        let mut config = AppConfig::default();
        config.hotzones = vec![hotzone(HotzoneId::TopRight, true, ActionKind::None)];
        config.monitor_profiles = vec![MonitorProfile {
            monitor_id: monitor_id(&display),
            hotzones: vec![hotzone(HotzoneId::TopRight, true, ActionKind::ShowDesktop)],
        }];

        assert_eq!(
            hotzone_hint_for(&config, Point { x: 99, y: 1 }, &[display], false),
            Some(HotzoneId::TopRight)
        );
    }

    #[test]
    fn slide_motion_follows_the_axis_of_each_edge() {
        assert_eq!(
            slide_motion(HotzoneId::Top, Point { x: 15, y: 1 }, Point { x: 10, y: 1 }),
            5
        );
        assert_eq!(
            slide_motion(
                HotzoneId::Right,
                Point { x: 99, y: 7 },
                Point { x: 99, y: 12 }
            ),
            -5
        );
        assert_eq!(
            slide_motion(
                HotzoneId::TopRight,
                Point { x: 99, y: 1 },
                Point { x: 94, y: 1 }
            ),
            0
        );
    }

    #[test]
    fn continuous_volume_uses_raw_wheel_and_slide_distance() {
        let volume = HotzoneAction {
            kind: ActionKind::VolumeAdjust,
            value: Some("0.02".to_string()),
        };

        assert_eq!(
            continuous_action_scale(
                &volume,
                TriggerKind::WheelUp,
                InputState {
                    wheel_delta: 240,
                    ..Default::default()
                },
                InputState::default(),
                0,
            ),
            Some(2.0)
        );
        assert_eq!(
            continuous_action_scale(
                &volume,
                TriggerKind::WheelUp,
                InputState {
                    wheel_delta: 30,
                    ..Default::default()
                },
                InputState::default(),
                0,
            ),
            Some(0.25)
        );
        assert_eq!(
            continuous_action_scale(
                &volume,
                TriggerKind::SlideForward,
                InputState::default(),
                InputState::default(),
                8,
            ),
            Some(0.5)
        );
        assert_eq!(
            continuous_action_scale(
                &volume,
                TriggerKind::SlideBackward,
                InputState::default(),
                InputState::default(),
                -4,
            ),
            Some(0.25)
        );
    }

    #[test]
    fn volume_is_only_continuous_for_wheel_and_slide_triggers() {
        let volume = HotzoneAction {
            kind: ActionKind::VolumeAdjust,
            value: Some("0.02".to_string()),
        };

        assert!(is_continuous_volume_trigger(&volume, TriggerKind::WheelUp));
        assert!(is_continuous_volume_trigger(
            &volume,
            TriggerKind::SlideBackward
        ));
        assert!(!is_continuous_volume_trigger(&volume, TriggerKind::Hover));
        assert!(!is_continuous_volume_trigger(
            &volume,
            TriggerKind::LeftClick
        ));
    }

    #[test]
    fn fullscreen_pause_uses_monitor_bounds_not_work_area() {
        let mut monitor = monitor();
        monitor.work_area.bottom = monitor.bounds.bottom - 10;
        let mut window = crate::platform::WindowInfo {
            handle: crate::platform::WindowHandle(1),
            rect: monitor.bounds,
            title: String::new(),
            class_name: String::new(),
            process_name: "game.exe".into(),
            maximized: false,
            transient: false,
            arranged: false,
            topmost: false,
        };
        assert!(is_fullscreen_window(&window, &[monitor]));
        window.rect = monitor.work_area;
        assert!(!is_fullscreen_window(&window, &[monitor]));
    }

    #[test]
    fn fullscreen_pause_excludes_maximized_and_desktop_shell_windows() {
        let monitor = monitor();
        let mut window = crate::platform::WindowInfo {
            handle: crate::platform::WindowHandle(1),
            rect: monitor.bounds,
            title: String::new(),
            class_name: "Chrome_WidgetWin_1".into(),
            process_name: "browser.exe".into(),
            maximized: true,
            transient: false,
            arranged: false,
            topmost: false,
        };
        assert!(!is_fullscreen_window(&window, &[monitor]));

        window.maximized = false;
        window.class_name = "WorkerW".into();
        window.process_name = "explorer.exe".into();
        assert!(!is_fullscreen_window(&window, &[monitor]));
    }
}
