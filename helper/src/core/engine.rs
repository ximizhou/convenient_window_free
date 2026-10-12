use crate::config::GestureMode;
use crate::config::{ActionKind, AppConfig, HotzoneId, HotzoneSetting, TriggerKind};
use crate::core::actions::ActionDispatcher;
use crate::core::edge_hide::{
    EdgeHideCommand, EdgeHideController, EdgeHideInput, EdgeHideLiveState,
};
use crate::core::gesture::{path_length_pixels, recognize};
use crate::core::hotzone::{configured_hotzone_rect, detect_configured_hotzone};
use crate::core::trigger::HotzoneTriggerController;
use crate::core::window_drag::WindowDragController;
use crate::ipc::errors::{ErrorCode, RuntimeError};
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

#[derive(Clone, Debug, Default)]
struct WindowDragActivity {
    active: bool,
    cancelled: bool,
    released_window: Option<platform::WindowInfo>,
}

fn gesture_overlay_label(config: &AppConfig, points: &[platform::Point]) -> Option<Option<String>> {
    if !config.mouse_gestures.show_trail
        || points.len() < 2
        || path_length_pixels(points) < config.mouse_gestures.min_distance as f32
    {
        return None;
    }
    Some(
        recognize(points, &config.mouse_gestures)
            .and_then(|result| config.gesture_label(result.gesture).map(str::to_owned)),
    )
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

    pub async fn run(&mut self) -> Result<()> {
        logging::write_line("engine: starting");
        if self.shutdown_rx.try_recv().is_ok() {
            return Ok(());
        }
        platform::install_mouse_hook()?;
        logging::write_line("engine: mouse hook requested");

        let dispatcher = ActionDispatcher::new(self.event_tx.clone());
        let mut config_rx = self.config_rx.clone();
        let mut config = config_rx.borrow_and_update().clone();
        let taskbar = platform::TaskbarAppearanceWorker::new(self.event_tx.clone());
        taskbar.configure(
            config.enabled && config.taskbar_appearance.enabled,
            &config.taskbar_appearance,
        );

        let mut previous_input = InputState::default();
        let mut previous_cursor = None;
        let mut foreground_tracker = ForegroundTracker::default();
        let mut hotzone_triggers = HotzoneTriggerController::new(Instant::now());
        let mut edge_hide = EdgeHideController::new();
        let mut window_drag = WindowDragController::default();
        let mut native_drag = NativeDragPreviewTracker::default();
        let mut last_status_at = Instant::now() - Duration::from_secs(5);
        let mut monitor_refresh = RefreshSchedule::new(Duration::from_secs(1));
        let mut window_prune = RefreshSchedule::new(Duration::from_secs(1));
        let mut cached_monitors = Vec::new();
        let mut runtime_failure = None;
        #[cfg(debug_assertions)]
        let injected_failure_at = injected_engine_failure_deadline();

        loop {
            if self.shutdown_rx.try_recv().is_ok() {
                break;
            }
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
                &config.window_drag.paused_apps,
            );
            let input = platform::input_state();

            if config.enabled {
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
                        if let Some(label) = gesture_overlay_label(&config, &gesture_points) {
                            platform::update_gesture_overlay(label.as_deref());
                        } else {
                            platform::hide_gesture_overlay();
                            if !config.mouse_gestures.show_trail {
                                platform::discard_gesture_trail();
                            }
                        }

                        // Resolve the display/profile and geometry once for both hints and input.
                        let hotzone_frame =
                            hotzone_frame_for(&config, cursor, monitors, paused || dragging);
                        platform::update_hotzone_hints(if config.show_hotzone_hint {
                            hotzone_frame.rect
                        } else {
                            None
                        });
                        self.handle_hotzone(
                            now,
                            &config,
                            &dispatcher,
                            cursor,
                            hotzone_frame,
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

                        let native_move_active = foreground.as_ref().is_some_and(|window| {
                            platform::native_window_move_size_active(window.handle)
                        });
                        let preview_cancelled = input.escape_down || drag_activity.cancelled;
                        let native_preview_active = if preview_cancelled {
                            native_drag.cancel();
                            false
                        } else if dragging {
                            native_drag.reset();
                            false
                        } else {
                            native_drag.observe(input.left_down, foreground.as_ref())
                        };
                        let moving_window = window_drag.moving_window();
                        let preview_window = moving_window.as_ref().or_else(|| {
                            native_preview_active
                                .then_some(foreground.as_ref())
                                .flatten()
                        });
                        let edge_hide_preview = if preview_cancelled {
                            edge_hide.cancel_drag_preview();
                            None
                        } else {
                            // Validate the final pointer sample before freezing release intent.
                            // A release may contain a new position that the periodic loop missed.
                            if let Some(window) = drag_activity.released_window.as_ref() {
                                edge_hide.update_drag_preview(
                                    now,
                                    &config.edge_hide,
                                    monitors,
                                    Some(window),
                                    true,
                                );
                            }
                            edge_hide.update_drag_preview(
                                now,
                                &config.edge_hide,
                                monitors,
                                preview_window,
                                moving_window.is_some() || native_preview_active,
                            )
                        };
                        platform::update_edge_hide_preview(edge_hide_preview);
                        let mut edge_hide_live_states = HashMap::new();
                        if !dragging {
                            self.handle_edge_hide(
                                &config,
                                &mut edge_hide,
                                now,
                                cursor,
                                monitors,
                                foreground.as_ref(),
                                EdgeHideInput {
                                    left_button_down: input.left_down || native_move_active,
                                    right_button_pressed: input.right_clicked_since(previous_input),
                                    context_menu_dismissed: input
                                        .context_menu_dismissed_since(previous_input),
                                    suppress_foreground_restore,
                                },
                                &mut edge_hide_live_states,
                            );
                        }
                        let strip_hints = if config.edge_hide.show_restore_hint {
                            edge_hide.collapsed_strips_with_live_state(monitors, |handle, _| {
                                cached_edge_hide_live_state(&mut edge_hide_live_states, handle)
                            })
                        } else {
                            Vec::new()
                        };
                        platform::update_strip_hints(&strip_hints);
                        previous_cursor = Some(cursor);
                    }
                    (cursor, monitors, foreground) => {
                        platform::configure_gesture_capture(
                            false,
                            config.mouse_gestures.trigger_button,
                        );
                        native_drag.reset();
                        edge_hide.cancel_drag_preview();
                        platform::update_edge_hide_preview(None);
                        platform::update_strip_hints(&[]);
                        platform::cancel_window_drag_capture();
                        if let Some((handle, rect)) = window_drag.cancel() {
                            let _ = platform::set_window_rect(handle, rect);
                        }
                        platform::hide_gesture_overlay();
                        self.handle_mouse_gestures(&config, &dispatcher, false);
                        hotzone_triggers.suspend(now);
                        previous_cursor = None;
                        platform::hide_hotzone_hints();
                        self.report_runtime_error_data(
                            RuntimeError::new(
                                ErrorCode::InputMonitorFailed,
                                json!({
                                    "cursor": cursor.err().map(|error| error.to_string()),
                                    "monitors": monitors.err().map(|error| error.to_string()),
                                    "foreground": foreground.err().map(|error| error.to_string()),
                                })
                                .to_string(),
                            )
                            .data(),
                        );
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
                    &config.window_drag.paused_apps,
                );
                native_drag.reset();
                edge_hide.cancel_drag_preview();
                platform::update_edge_hide_preview(None);
                platform::update_strip_hints(&[]);
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
                if edge_hide.has_restore_work() {
                    match platform::monitors() {
                        Ok(monitors) => cached_monitors = monitors,
                        Err(error) => {
                            self.report_runtime_error(ErrorCode::WindowOperationFailed, error)
                        }
                    }
                    self.restore_edge_hide_if_needed(
                        &mut edge_hide,
                        &cached_monitors,
                        config.edge_hide.strip_size,
                    );
                }
            }
            self.handle_ocr_completions();
            if let Some(feedback) = platform::take_adjustment_feedback() {
                // 调整失败必须同时走运行错误通道：桌面提示条之外（uTools 宿主、状态栏）
                // 只能看到 runtime.error，否则音量/亮度失败会变成静默事件。
                if let Some(error) = feedback.error.clone() {
                    self.report_runtime_error_data(error.data());
                }
                // 序列化失败只影响这一条事件，不能终止引擎循环。
                if let Ok(data) = serde_json::to_value(feedback) {
                    let _ = self
                        .event_tx
                        .send(HelperMessage::new("adjustment.updated", data));
                }
            }
            previous_input = input;
            let interval = if window_drag.is_active() {
                Duration::from_nanos(1_000_000_000 / config.window_drag.update_hz.clamp(30, 240))
            } else {
                engine_poll_interval(config.poll_interval_ms, false)
            };
            let wait = poll_wait(interval, now.elapsed());

            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = platform::window_drag_started_notification().notified() => {}
                changed = config_rx.changed() => {
                    if changed.is_ok() {
                        config = config_rx.borrow_and_update().clone();
                        taskbar.configure(config.enabled && config.taskbar_appearance.enabled, &config.taskbar_appearance);
                    }
                }
                _ = self.shutdown_rx.recv() => break,
            }
        }
        taskbar.configure(false, &config.taskbar_appearance);
        drop(taskbar);
        platform::configure_topmost_pins(false);
        platform::clear_topmost_pins();
        platform::configure_window_drag_capture(
            false,
            config.window_drag.move_button,
            0,
            config.window_drag.resize_button,
            0,
            &config.window_drag.paused_apps,
        );
        platform::update_edge_hide_preview(None);
        platform::update_strip_hints(&[]);
        if let Some((handle, rect)) = window_drag.cancel() {
            let _ = platform::set_window_rect(handle, rect);
        }
        platform::configure_gesture_capture(false, config.mouse_gestures.trigger_button);
        platform::hide_gesture_overlay();
        platform::hide_hotzone_hints();
        if edge_hide.has_restore_work() {
            match platform::monitors() {
                Ok(monitors) => cached_monitors = monitors,
                Err(error) => self.report_runtime_error(ErrorCode::WindowOperationFailed, error),
            }
            self.restore_edge_hide_if_needed(
                &mut edge_hide,
                &cached_monitors,
                config.edge_hide.strip_size,
            );
        }
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
                    self.report_runtime_error(ErrorCode::OcrFailed, anyhow::anyhow!(message));
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
            let cancelled = controller.is_active();
            platform::cancel_window_drag_capture();
            if let Some((handle, rect)) = controller.cancel() {
                if let Err(error) = platform::set_window_rect(handle, rect) {
                    self.report_runtime_error(ErrorCode::WindowOperationFailed, error);
                }
            }
            return WindowDragActivity {
                active: false,
                cancelled,
                ..Default::default()
            };
        }
        let Some(capture) = capture else {
            return WindowDragActivity::default();
        };
        if controller.sequence() != Some(capture.sequence) {
            // 应用名单在 draggable_window_at 内部、且在摘除最大化状态之前就已判定，
            // 所以这里拿到的 None 既包含"不可拖拽的窗口"，也包含"命中名单的应用"。
            // 名单命中时窗口没有任何副作用（判定前移正是为了避免先摘掉最大化状态），
            // 因此不需要额外记住"本次按下已被拒绝"：同序号的高频移动会重复一次
            // draggable_window_at，但那只是读取窗口信息，名单非空时才有额外比较。
            match platform::draggable_window_at(capture.start, &config.window_drag.paused_apps) {
                Ok(Some(window)) => {
                    controller.start(capture, &window);
                }
                Ok(None) => {
                    platform::discard_window_drag_capture();
                    return WindowDragActivity::default();
                }
                Err(error) => {
                    platform::cancel_window_drag_capture();
                    self.report_runtime_error(ErrorCode::WindowOperationFailed, error);
                    return WindowDragActivity::default();
                }
            }
        }
        if cancel {
            platform::cancel_window_drag_capture();
        }
        let Some(update) = controller.update(capture, cancel) else {
            return WindowDragActivity {
                active: controller.is_active(),
                cancelled: false,
                ..Default::default()
            };
        };
        if let Err(error) = platform::set_window_rect(update.handle, update.rect) {
            platform::cancel_window_drag_capture();
            let _ = controller.cancel();
            self.report_runtime_error(ErrorCode::WindowOperationFailed, error);
            return WindowDragActivity {
                active: false,
                cancelled: true,
                ..Default::default()
            };
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
            cancelled: update.cancelled,
            released_window: if update.finished
                && !update.cancelled
                && capture.mode == platform::WindowDragMode::Move
            {
                platform::window_info_for_handle(update.handle)
                    .ok()
                    .flatten()
            } else {
                None
            },
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
                    self.report_runtime_error(ErrorCode::ActionFailed, error);
                }
                continue;
            }
            if !allow_execution {
                continue;
            }
            let Some(result) = recognize(&capture.points, &config.mouse_gestures) else {
                let _ = self.event_tx.send(HelperMessage::new(
                    "runtime.status",
                    json!({ "code": "gesture_not_recognized" }),
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
                    let capture_path = platform::take_capture_result();
                    let _ = self.event_tx.send(HelperMessage::new(
                        "gesture.recognized",
                        json!({
                            "id": result.gesture.id,
                            "name": if result.gesture.name.is_empty() { None } else { Some(&result.gesture.name) },
                            "score": result.score,
                            "region": result.region.map(|rect| json!({
                                "left": rect.left, "top": rect.top, "right": rect.right, "bottom": rect.bottom
                            })),
                            "capturePath": capture_path
                        }),
                    ));
                }
                Err(error) => self.report_runtime_error(
                    if result.gesture.mode == GestureMode::RegionScreenshot {
                        ErrorCode::ScreenCaptureFailed
                    } else {
                        ErrorCode::ActionFailed
                    },
                    error,
                ),
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
        frame: HotzoneFrame<'_>,
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

        hotzone_triggers.observe(now, frame.detected);

        let Some(hotzone_id) = frame.detected else {
            return;
        };

        let Some(setting) = frame.setting else {
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
            if is_continuous_adjustment_trigger(action, trigger_action.trigger) {
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
                if let Err(error) = dispatcher.dispatch_scaled_at_with_modifiers(
                    action,
                    &format!("{:?}/{:?}", hotzone_id, trigger_action.trigger),
                    scale,
                    Some(cursor),
                    modifiers,
                ) {
                    self.report_runtime_error(ErrorCode::ActionFailed, error);
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
            if let Err(error) = dispatcher.dispatch_at_with_modifiers(
                action,
                &format!("{:?}/{:?}", hotzone_id, trigger_action.trigger),
                Some(cursor),
                modifiers,
            ) {
                self.report_runtime_error(ErrorCode::ActionFailed, error);
            }
        }
    }

    fn report_runtime_error(&self, code: ErrorCode, error: anyhow::Error) {
        self.report_runtime_error_data(RuntimeError::new(code, format!("{error:#}")).data());
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
        logging::write_line(format!("runtime.error: {data}"));
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
        live_states: &mut HashMap<platform::WindowHandle, EdgeHideLiveState>,
    ) {
        let drop_window = edge_hide
            .released_top_handle()
            .and_then(|handle| platform::window_info_for_handle(handle).ok().flatten());
        let top_drop = edge_hide.take_top_drop(
            now,
            &config.edge_hide,
            monitors,
            drop_window.as_ref(),
            drop_window
                .as_ref()
                .is_some_and(|window| platform::native_window_move_size_active(window.handle)),
        );
        if let Some(command) = top_drop {
            if let Err(error) = platform::prepare_top_edge_drop(edge_hide_command_handle(command)) {
                edge_hide.command_failed(command, now);
                self.report_runtime_error(ErrorCode::WindowOperationFailed, error);
                return;
            }
        }
        if let Some(command) = top_drop.or_else(|| {
            edge_hide.tick_with_live_state(
                now,
                &config.edge_hide,
                cursor,
                monitors,
                foreground,
                input,
                |handle, _| cached_edge_hide_live_state(live_states, handle),
            )
        }) {
            match execute_edge_hide_command(
                command,
                config.edge_hide.animation_enabled,
                Duration::from_millis(config.edge_hide.animation_duration_ms),
            ) {
                Ok((kind, rect)) => {
                    let handle = edge_hide_command_handle(command);
                    let live_state = edge_hide_live_state(handle);
                    live_states.insert(handle, live_state);
                    if let Some(cleanup) = edge_hide.command_succeeded(command, live_state, now) {
                        match execute_edge_hide_command(
                            cleanup,
                            false,
                            EDGE_HIDE_ANIMATION_DURATION,
                        ) {
                            Ok(_) => {
                                let cleanup_live_state = edge_hide_live_state(handle);
                                edge_hide.command_succeeded(cleanup, cleanup_live_state, now);
                                live_states.insert(handle, cleanup_live_state);
                            }
                            Err(error) => {
                                edge_hide.command_failed(cleanup, now);
                                self.report_runtime_error(ErrorCode::WindowOperationFailed, error);
                                live_states.insert(handle, edge_hide_live_state(handle));
                            }
                        }
                    }
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
                    edge_hide.command_failed(command, now);
                    self.report_runtime_error(ErrorCode::WindowOperationFailed, error);
                }
            }
        }
    }

    fn restore_edge_hide_if_needed(
        &self,
        edge_hide: &mut EdgeHideController,
        monitors: &[platform::Monitor],
        strip_size: i32,
    ) {
        edge_hide.prepare_restore_all(monitors, strip_size);
        for _ in 0..3 {
            let Some(
                command @ EdgeHideCommand::Restore {
                    handle,
                    rect,
                    topmost,
                },
            ) = edge_hide.restore_if_needed()
            else {
                break;
            };
            let now = Instant::now();
            match platform::set_window_rect_topmost(handle, rect, topmost) {
                Ok(()) => {
                    edge_hide.command_succeeded(command, edge_hide_live_state(handle), now);
                }
                Err(error) => {
                    edge_hide.command_failed(command, now);
                    self.report_runtime_error(ErrorCode::WindowOperationFailed, error);
                }
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

const EDGE_HIDE_ANIMATION_DURATION: Duration = Duration::from_millis(160);

fn execute_edge_hide_command(
    command: EdgeHideCommand,
    animated: bool,
    duration: Duration,
) -> Result<(&'static str, platform::Rect)> {
    let (handle, rect, topmost, kind) = match command {
        EdgeHideCommand::Collapse { handle, rect } => (handle, rect, true, "edge-hide.collapse"),
        EdgeHideCommand::Restore {
            handle,
            rect,
            topmost,
        } => (handle, rect, topmost, "edge-hide.restore"),
    };
    let from = if animated {
        platform::window_info_for_handle(handle)
            .ok()
            .flatten()
            .map(|window| window.rect)
            .unwrap_or(rect)
    } else {
        rect
    };
    animate_edge_hide_window(handle, from, rect, topmost, animated, duration)?;
    Ok((kind, rect))
}

fn animate_edge_hide_window(
    handle: platform::WindowHandle,
    from: platform::Rect,
    to: platform::Rect,
    topmost: bool,
    animated: bool,
    duration: Duration,
) -> Result<()> {
    run_edge_hide_transition(
        from,
        to,
        animated,
        duration,
        |rect, final_frame| {
            if final_frame {
                platform::set_window_rect_topmost(handle, rect, topmost)
            } else {
                platform::set_window_rect(handle, rect)
            }
        },
        std::thread::sleep,
    )
}

fn run_edge_hide_transition(
    from: platform::Rect,
    to: platform::Rect,
    animated: bool,
    duration: Duration,
    mut move_window: impl FnMut(platform::Rect, bool) -> Result<()>,
    mut sleep: impl FnMut(Duration),
) -> Result<()> {
    if !animated || from == to {
        return move_window(to, true);
    }
    let frames = (duration.as_millis() / 16).clamp(4, 50) as i32;
    let frame_duration = duration / frames as u32;
    for frame in 1..=frames {
        let progress = frame as f64 / frames as f64;
        let eased = 1.0 - (1.0 - progress).powi(3);
        let final_frame = frame == frames;
        let rect = interpolate_edge_hide_rect(from, to, eased, final_frame);
        move_window(rect, final_frame)?;
        if !final_frame {
            sleep(frame_duration);
        }
    }
    Ok(())
}

fn interpolate_edge_hide_rect(
    from: platform::Rect,
    to: platform::Rect,
    progress: f64,
    final_frame: bool,
) -> platform::Rect {
    if final_frame {
        return to;
    }
    let mix =
        |start: i32, end: i32| (start as f64 + (end - start) as f64 * progress).round() as i32;
    platform::Rect {
        left: mix(from.left, to.left),
        top: mix(from.top, to.top),
        right: mix(from.right, to.right),
        bottom: mix(from.bottom, to.bottom),
    }
}

fn edge_hide_command_handle(command: EdgeHideCommand) -> platform::WindowHandle {
    match command {
        EdgeHideCommand::Collapse { handle, .. } | EdgeHideCommand::Restore { handle, .. } => {
            handle
        }
    }
}

fn cached_edge_hide_live_state(
    states: &mut HashMap<platform::WindowHandle, EdgeHideLiveState>,
    handle: platform::WindowHandle,
) -> EdgeHideLiveState {
    *states
        .entry(handle)
        .or_insert_with(|| edge_hide_live_state(handle))
}

fn edge_hide_live_state(handle: platform::WindowHandle) -> EdgeHideLiveState {
    let Ok(Some(window)) = platform::window_info_for_handle(handle) else {
        return EdgeHideLiveState::Unavailable;
    };
    classify_edge_hide_live_state(Some(window.rect), platform::window_is_minimized(handle))
}

fn classify_edge_hide_live_state(
    rect: Option<platform::Rect>,
    minimized: bool,
) -> EdgeHideLiveState {
    match rect {
        None => EdgeHideLiveState::Unavailable,
        Some(_) if minimized => EdgeHideLiveState::Minimized,
        Some(rect) => EdgeHideLiveState::Visible(rect),
    }
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

/// 拖拽与缩放的应用名单判定的测试入口。
///
/// 生产路径的判定已经前移到 `platform::draggable_window_at` —— 它必须在摘除最大化状态
/// **之前**执行，否则命中名单的窗口已经被摘成浮动小窗（这是 0.6.0 用户反馈的缺陷）。
/// 这里保留一个转发，让引擎侧的名单语义仍有测试覆盖，并与平台层实现保持单一真相。
#[cfg(test)]
fn blocks_window_drag(paused_apps: &[String], window: &platform::WindowInfo) -> bool {
    platform::is_paused_window(paused_apps, window)
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

#[derive(Clone, Copy, Default)]
struct HotzoneFrame<'a> {
    detected: Option<HotzoneId>,
    setting: Option<&'a HotzoneSetting>,
    rect: Option<platform::Rect>,
}

fn hotzone_frame_for<'a>(
    config: &'a AppConfig,
    cursor: platform::Point,
    monitors: &[platform::Monitor],
    paused: bool,
) -> HotzoneFrame<'a> {
    if paused || !config.hotzones_enabled {
        return HotzoneFrame::default();
    }
    let zones = hotzones_for_cursor(config, cursor, monitors);
    let detected = detect_configured_hotzone(cursor, monitors, config.edge_size, zones);
    let setting =
        detected.and_then(|id| zones.iter().find(|zone| zone.id == id && zone.has_action()));
    let rect = setting.and_then(|zone| {
        monitors
            .iter()
            .find(|monitor| monitor.bounds.contains(cursor))
            .map(|monitor| {
                configured_hotzone_rect(
                    zone.id,
                    monitor.bounds,
                    config.edge_size,
                    zone.geometry.as_ref(),
                )
            })
    });
    HotzoneFrame {
        detected,
        setting,
        rect,
    }
}

#[cfg(test)]
fn hotzone_hint_for(
    config: &AppConfig,
    cursor: platform::Point,
    monitors: &[platform::Monitor],
    paused: bool,
) -> Option<HotzoneId> {
    if !config.show_hotzone_hint {
        return None;
    }
    let frame = hotzone_frame_for(config, cursor, monitors, paused);
    frame.setting.map(|zone| zone.id)
}

fn poll_wait(interval: Duration, processing: Duration) -> Duration {
    interval.saturating_sub(processing)
}

#[derive(Default)]
struct NativeDragPreviewTracker {
    previous: Option<(platform::WindowHandle, platform::Rect, bool)>,
    active: bool,
    resizing: bool,
    cancelled: bool,
}

impl NativeDragPreviewTracker {
    fn reset(&mut self) {
        self.previous = None;
        self.active = false;
        self.resizing = false;
        self.cancelled = false;
    }
    fn cancel(&mut self) {
        self.reset();
        self.cancelled = true;
    }
    fn observe(&mut self, held: bool, window: Option<&platform::WindowInfo>) -> bool {
        let Some(window) = window.filter(|_| held) else {
            self.reset();
            return false;
        };
        if self.cancelled {
            return false;
        }
        if let Some((handle, rect, was_arranged)) = self.previous {
            if handle != window.handle {
                self.active = false;
                self.resizing = false;
            } else if rect.width() != window.rect.width() || rect.height() != window.rect.height() {
                // Native dragging first restores a maximized/snapped window.
                // Treat that layout transition as a fresh movement baseline.
                if was_arranged && !window.maximized && !window.arranged {
                    self.active = false;
                } else {
                    self.resizing = true;
                }
            } else if rect != window.rect {
                self.active = true;
            }
        }
        self.active &= !self.resizing;
        self.previous = Some((
            window.handle,
            window.rect,
            window.maximized || window.arranged,
        ));
        self.active
    }
}

fn hotzones_for_cursor<'a>(
    config: &'a AppConfig,
    cursor: platform::Point,
    monitors: &[platform::Monitor],
) -> &'a [HotzoneSetting] {
    if config.monitor_profiles.is_empty() {
        return &config.hotzones;
    }
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
    if !matches!(
        action.kind,
        ActionKind::VolumeAdjust | ActionKind::BrightnessAdjust
    ) {
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

fn is_continuous_adjustment_trigger(
    action: &crate::config::HotzoneAction,
    trigger: TriggerKind,
) -> bool {
    matches!(
        action.kind,
        ActionKind::VolumeAdjust | ActionKind::BrightnessAdjust
    ) && matches!(
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

    #[test]
    fn gesture_overlay_stays_hidden_until_the_configured_distance() {
        use platform::Point;
        let mut config = AppConfig::default();
        config.mouse_gestures.min_distance = 60;
        for points in [
            vec![],
            vec![Point { x: 0, y: 0 }],
            vec![Point { x: 0, y: 0 }, Point { x: 0, y: 59 }],
        ] {
            assert_eq!(gesture_overlay_label(&config, &points), None);
        }
        let points = [Point { x: 0, y: 0 }, Point { x: 0, y: 60 }];
        assert_eq!(gesture_overlay_label(&config, &points), Some(None));
        config.mouse_gestures.show_trail = false;
        assert_eq!(gesture_overlay_label(&config, &points), None);
    }

    #[test]
    fn gesture_overlay_shows_only_the_matched_name_and_keeps_unknown_strokes_silent() {
        use platform::Point;
        let mut config = AppConfig::default();
        config
            .gesture_labels
            .insert("gesture-down".into(), "粘贴".into());
        let straight = [Point { x: 0, y: 0 }, Point { x: 0, y: 100 }];
        assert_eq!(
            gesture_overlay_label(&config, &straight),
            Some(Some("粘贴".into()))
        );
        let unknown = [
            Point { x: 0, y: 0 },
            Point { x: 100, y: 10 },
            Point { x: 0, y: 20 },
            Point { x: 100, y: 30 },
            Point { x: 0, y: 40 },
        ];
        assert_eq!(gesture_overlay_label(&config, &unknown), Some(None));
        config
            .mouse_gestures
            .gestures
            .iter_mut()
            .for_each(|gesture| gesture.enabled = false);
        assert_eq!(gesture_overlay_label(&config, &straight), Some(None));
    }

    #[tokio::test]
    async fn shutdown_before_engine_start_is_preserved() {
        let (_config_tx, config_rx) = watch::channel(AppConfig::default());
        let (event_tx, _) = broadcast::channel(8);
        let (shutdown_tx, shutdown_rx) = broadcast::channel(4);
        let mut engine = Engine::new(config_rx, event_tx, shutdown_rx);
        shutdown_tx.send(()).unwrap();

        tokio::time::timeout(Duration::from_millis(100), engine.run())
            .await
            .expect("a queued shutdown must not start the input hook or wait for another signal")
            .unwrap();
    }
    use crate::config::{
        ActionKind, HotzoneAction, HotzoneSetting, MonitorProfile, TriggerAction, TriggerKind,
    };
    use crate::core::hotzone::detect_hotzone;
    use crate::platform::{Monitor, Point, Rect};

    #[cfg(target_os = "windows")]
    #[test]
    fn isolated_edge_hide_recollapses_after_speed_change_and_desktop_hover() {
        use windows::core::w;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, WS_EX_NOACTIVATE, WS_POPUP, WS_VISIBLE,
        };
        struct TestWindow(HWND);
        impl Drop for TestWindow {
            fn drop(&mut self) {
                let _ = unsafe { DestroyWindow(self.0) };
            }
        }
        let fixture = TestWindow(unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("WindowRecollapseTest"),
                WS_POPUP | WS_VISIBLE,
                0,
                120,
                600,
                400,
                HWND::default(),
                None,
                None,
                None,
            )
            .unwrap()
        });
        let handle = platform::WindowHandle(fixture.0 .0 as isize);
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
        let mut desktop = named_window("explorer.exe", "Program Manager", "Progman");
        desktop.handle = platform::WindowHandle(99);
        desktop.rect = monitor.bounds;
        let mut config = AppConfig::default();
        config.edge_hide.enabled = true;
        config.edge_hide.collapse_delay_ms = 0;
        config.edge_hide.restore_delay_ms = 100;
        let (_, config_rx) = watch::channel(config.clone());
        let (event_tx, mut events) = broadcast::channel(32);
        let (_, shutdown_rx) = broadcast::channel(4);
        let engine = Engine::new(config_rx, event_tx, shutdown_rx);
        let mut controller = EdgeHideController::new();
        let mut now = Instant::now();
        let outside = Point { x: 1200, y: 900 };
        let mut tick =
            |config: &AppConfig, now, cursor, foreground: Option<&platform::WindowInfo>| {
                engine.handle_edge_hide(
                    config,
                    &mut controller,
                    now,
                    cursor,
                    std::slice::from_ref(&monitor),
                    foreground,
                    EdgeHideInput::default(),
                    &mut HashMap::new(),
                );
            };
        let target = platform::window_info_for_handle(handle).unwrap().unwrap();
        tick(&config, now, outside, Some(&target));
        now += Duration::from_millis(1);
        tick(&config, now, outside, Some(&target));
        assert_eq!(
            events.try_recv().unwrap().data["source"],
            "edge-hide.collapse"
        );
        for duration in [80, 160, 260, 400, 800] {
            config.edge_hide.animation_duration_ms = duration;
            let hidden = platform::window_info_for_handle(handle)
                .unwrap()
                .unwrap()
                .rect;
            assert_eq!(hidden.right, config.edge_hide.strip_size);
            now += Duration::from_secs(1);
            tick(&config, now, outside, Some(&desktop));
            tick(
                &config,
                now + Duration::from_millis(1),
                Point { x: 2, y: 200 },
                Some(&desktop),
            );
            assert_eq!(
                events.try_recv().unwrap().data["source"],
                "edge-hide.restore"
            );
            assert_eq!(
                platform::window_info_for_handle(handle)
                    .unwrap()
                    .unwrap()
                    .rect,
                target.rect
            );
            let finished = now + Duration::from_millis(duration + 1);
            tick(&config, finished, outside, Some(&desktop));
            tick(
                &config,
                finished + Duration::from_millis(100),
                outside,
                Some(&desktop),
            );
            assert_eq!(
                events.try_recv().unwrap().data["source"],
                "edge-hide.collapse"
            );
            assert_eq!(
                platform::window_info_for_handle(handle)
                    .unwrap()
                    .unwrap()
                    .rect,
                hidden
            );
            now = finished + Duration::from_millis(101);
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn isolated_drag_pipeline_moves_resizes_and_cancels_a_real_window() {
        use windows::core::w;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP,
        };
        struct TestWindow(HWND);
        impl Drop for TestWindow {
            fn drop(&mut self) {
                let _ = unsafe { DestroyWindow(self.0) };
            }
        }
        let fixture = TestWindow(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("WindowMotionTest"),
                WS_POPUP,
                100,
                100,
                600,
                400,
                HWND::default(),
                None,
                None,
                None,
            )
            .unwrap()
        });
        let read_rect = || {
            let mut rect = windows::Win32::Foundation::RECT::default();
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetWindowRect(fixture.0, &mut rect)
                    .unwrap();
            }
            Rect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            }
        };
        let mut window = named_window("test.exe", "WindowMotionTest", "STATIC");
        window.handle = platform::WindowHandle(fixture.0 .0 as isize);
        window.rect = read_rect();
        let original = window.rect;
        let mut config = AppConfig::default();
        config.window_drag.enabled = true;
        let (_, config_rx) = watch::channel(config.clone());
        let (event_tx, mut events) = broadcast::channel(8);
        let (_, shutdown_rx) = broadcast::channel(4);
        let engine = Engine::new(config_rx, event_tx, shutdown_rx);
        let mut controller = WindowDragController::default();
        let mut capture = platform::WindowDragCapture {
            sequence: 1,
            mode: platform::WindowDragMode::Move,
            start: Point { x: 200, y: 200 },
            current: Point { x: 200, y: 200 },
            finished: false,
        };
        controller.start(capture, &window);
        for step in 1..=20 {
            capture.current = Point {
                x: 200 + step * 4,
                y: 200 + step * 2,
            };
            assert!(
                engine
                    .handle_window_drag(&config, &mut controller, Some(capture), false)
                    .active
            );
            let actual = read_rect();
            assert_eq!(actual.left, original.left + step * 4);
            assert_eq!(actual.top, original.top + step * 2);
        }
        assert!(
            engine
                .handle_window_drag(&config, &mut controller, Some(capture), false)
                .active,
            "a stationary drag must still suppress gestures and premature collapse"
        );
        let result = engine.handle_window_drag(&config, &mut controller, Some(capture), true);
        assert!(result.cancelled);
        assert!(!controller.is_active());
        assert_eq!(read_rect(), original);
        assert_eq!(
            events.try_recv().unwrap().data["source"],
            "window-drag.cancel"
        );

        capture.sequence = 2;
        capture.mode = platform::WindowDragMode::Resize;
        capture.start = Point {
            x: original.right - 10,
            y: original.bottom - 10,
        };
        capture.current = Point {
            x: capture.start.x + 60,
            y: capture.start.y + 40,
        };
        controller.start(capture, &window);
        assert!(
            controller.moving_window().is_none(),
            "resize must not offer a collapse preview"
        );
        capture.finished = true;
        let result = engine.handle_window_drag(&config, &mut controller, Some(capture), false);
        assert!(!result.cancelled);
        let resized = read_rect();
        assert_eq!(resized.width(), original.width() + 60);
        assert_eq!(resized.height(), original.height() + 40);
        assert_eq!(
            events.try_recv().unwrap().data["source"],
            "window-drag.commit"
        );
    }

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
    fn edge_hide_animation_switch_skips_all_intermediate_moves_and_waits() {
        let from = platform::Rect {
            left: -400,
            top: 50,
            right: 0,
            bottom: 350,
        };
        let to = platform::Rect {
            left: -20,
            top: 50,
            right: 380,
            bottom: 350,
        };
        for animated in [false, true] {
            let mut moves = Vec::new();
            let mut sleeps = Vec::new();
            run_edge_hide_transition(
                from,
                to,
                animated,
                EDGE_HIDE_ANIMATION_DURATION,
                |rect, final_frame| {
                    moves.push((rect, final_frame));
                    Ok(())
                },
                |duration| sleeps.push(duration),
            )
            .unwrap();
            assert_eq!(moves.last(), Some(&(to, true)));
            assert_eq!(
                moves.iter().filter(|(_, final_frame)| *final_frame).count(),
                1
            );
            if animated {
                assert_eq!(moves.len(), 10);
                assert_eq!(sleeps.len(), 9);
            } else {
                assert_eq!(moves, vec![(to, true)]);
                assert!(sleeps.is_empty());
            }
        }
    }

    #[test]
    fn configured_animation_duration_controls_frame_wait_budget() {
        let from = Rect {
            left: 100,
            top: 100,
            right: 700,
            bottom: 500,
        };
        let to = Rect {
            left: -584,
            top: 100,
            right: 16,
            bottom: 500,
        };
        for ms in [80, 160, 260, 400, 800] {
            let mut waits = Vec::new();
            let mut final_rect = None;
            run_edge_hide_transition(
                from,
                to,
                true,
                Duration::from_millis(ms),
                |rect, final_frame| {
                    if final_frame {
                        final_rect = Some(rect);
                    }
                    Ok(())
                },
                |wait| waits.push(wait),
            )
            .unwrap();
            assert_eq!(final_rect, Some(to));
            let total: Duration = waits.into_iter().sum();
            assert!(total <= Duration::from_millis(ms));
            assert!(total + Duration::from_millis(20) >= Duration::from_millis(ms));
        }
    }

    #[test]
    fn edge_hide_animation_preserves_window_size_until_final_frame() {
        let from = Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        };
        let to = Rect {
            left: -584,
            top: 120,
            right: 16,
            bottom: 700,
        };
        let middle = interpolate_edge_hide_rect(from, to, 0.5, false);
        assert_eq!(middle.width(), from.width());
        assert_eq!(middle.height(), from.height());
        assert!(middle.left < from.left);
        assert!(middle.left > to.left);
        assert_eq!(interpolate_edge_hide_rect(from, to, 0.1, true), to);
    }

    #[test]
    fn active_drag_uses_a_sixteen_millisecond_ceiling_without_changing_idle_polling() {
        assert_eq!(engine_poll_interval(33, false), Duration::from_millis(33));
        assert_eq!(engine_poll_interval(33, true), Duration::from_millis(16));
        assert_eq!(engine_poll_interval(10, true), Duration::from_millis(10));
        assert_eq!(engine_poll_interval(250, false), Duration::from_millis(250));
    }

    #[test]
    fn native_drag_preview_stays_visible_while_a_drag_pauses() {
        let mut tracker = NativeDragPreviewTracker::default();
        let mut window = named_window("editor.exe", "Editor", "Editor");
        assert!(!tracker.observe(true, Some(&window)));
        window.rect.left += 10;
        window.rect.right += 10;
        for _ in 0..100 {
            assert!(tracker.observe(true, Some(&window)));
        }
        assert!(!tracker.observe(false, Some(&window)));
    }

    #[test]
    fn native_drag_preview_requires_window_motion() {
        let mut tracker = NativeDragPreviewTracker::default();
        let mut window = named_window("editor.exe", "Editor", "Editor");
        for _ in 0..100 {
            assert!(!tracker.observe(true, Some(&window)));
        }
        window.rect.left += 10;
        window.rect.right += 10;
        assert!(tracker.observe(true, Some(&window)));
        assert!(!tracker.observe(true, None));
    }

    #[test]
    fn native_resize_never_earns_a_collapse_preview() {
        let mut tracker = NativeDragPreviewTracker::default();
        let mut window = named_window("editor.exe", "Editor", "Editor");
        tracker.observe(true, Some(&window));
        window.rect.right += 20;
        assert!(!tracker.observe(true, Some(&window)));
        for _ in 0..100 {
            assert!(!tracker.observe(true, Some(&window)));
        }
        tracker.observe(false, Some(&window));
        assert!(!tracker.observe(true, Some(&window)));
        window.rect.left += 10;
        window.rect.right += 10;
        assert!(tracker.observe(true, Some(&window)));
    }

    #[test]
    fn dragging_a_restored_maximized_window_is_not_mistaken_for_resize() {
        let mut tracker = NativeDragPreviewTracker::default();
        let mut window = named_window("editor.exe", "Editor", "Editor");
        window.maximized = true;
        assert!(!tracker.observe(true, Some(&window)));
        window.maximized = false;
        window.rect.right -= 100;
        window.rect.bottom -= 100;
        assert!(!tracker.observe(true, Some(&window)));
        window.rect.left += 10;
        window.rect.right += 10;
        assert!(tracker.observe(true, Some(&window)));
    }

    #[test]
    fn cancelled_native_drag_cannot_rearm_until_the_button_is_released() {
        let mut tracker = NativeDragPreviewTracker::default();
        let mut window = named_window("editor.exe", "Editor", "Editor");
        assert!(!tracker.observe(true, Some(&window)));
        window.rect.left += 10;
        window.rect.right += 10;
        assert!(tracker.observe(true, Some(&window)));
        tracker.cancel();
        for _ in 0..100 {
            assert!(!tracker.observe(true, Some(&window)));
        }
        assert!(!tracker.observe(false, Some(&window)));
        assert!(!tracker.observe(true, Some(&window)));
        window.rect.left += 10;
        window.rect.right += 10;
        assert!(tracker.observe(true, Some(&window)));
    }

    #[test]
    fn drag_poll_budget_subtracts_processing_instead_of_adding_latency() {
        assert_eq!(
            poll_wait(Duration::from_millis(8), Duration::from_millis(3)),
            Duration::from_millis(5)
        );
        assert_eq!(
            poll_wait(Duration::from_millis(8), Duration::from_millis(20)),
            Duration::ZERO
        );
    }

    #[test]
    fn hidden_windows_stay_unavailable_even_if_windows_reports_them_minimized() {
        assert_eq!(
            classify_edge_hide_live_state(None, true),
            EdgeHideLiveState::Unavailable
        );
        assert_eq!(
            classify_edge_hide_live_state(
                Some(Rect {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100,
                }),
                true,
            ),
            EdgeHideLiveState::Minimized
        );
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
            geometry: None,
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
    fn hotzone_hint_can_be_hidden_and_restored_without_changing_actions() {
        let display = monitor();
        let cursor = Point { x: 99, y: 1 };
        let mut config = AppConfig::default();
        config.monitor_profiles = vec![MonitorProfile {
            monitor_id: monitor_id(&display),
            hotzones: vec![hotzone(HotzoneId::TopRight, true, ActionKind::ShowDesktop)],
        }];
        assert_eq!(
            hotzone_hint_for(&config, cursor, &[display], false),
            Some(HotzoneId::TopRight)
        );
        config.show_hotzone_hint = false;
        assert_eq!(hotzone_hint_for(&config, cursor, &[display], false), None);
        assert!(hotzones_for_cursor(&config, cursor, &[display])[0].has_action());
        config.show_hotzone_hint = true;
        assert_eq!(
            hotzone_hint_for(&config, cursor, &[display], false),
            Some(HotzoneId::TopRight)
        );
        assert_eq!(hotzone_hint_for(&config, cursor, &[display], true), None);
    }

    #[test]
    fn hidden_hotzone_hint_preserves_volume_wheel_detection_and_trigger() {
        let display = monitor();
        let cursor = Point { x: 50, y: 99 };
        let mut config = AppConfig::default();
        config.show_hotzone_hint = false;
        let mut setting = hotzone(HotzoneId::Bottom, true, ActionKind::VolumeAdjust);
        setting.actions[0].trigger = TriggerKind::WheelUp;
        setting.actions[0].action.value = Some("5".into());
        config.hotzones = vec![setting];
        assert_eq!(hotzone_hint_for(&config, cursor, &[display], false), None);
        let detected = detect_hotzone(cursor, &[display], config.edge_size);
        assert_eq!(detected, Some(HotzoneId::Bottom));
        let action = &hotzones_for_cursor(&config, cursor, &[display])[0].actions[0];
        assert_eq!(action.action.kind, ActionKind::VolumeAdjust);
        assert_eq!(action.trigger, TriggerKind::WheelUp);
        let now = Instant::now();
        let mut triggers = HotzoneTriggerController::new(now);
        triggers.observe(now, detected);
        assert_eq!(
            triggers.accumulate_continuous_motion(
                now,
                HotzoneId::Bottom,
                action.trigger,
                1.0,
                Duration::from_millis(10)
            ),
            Some(1.0)
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
    fn shared_hotzone_frame_preserves_actions_when_the_hint_is_hidden() {
        use crate::config::HotzoneGeometry;
        let mut config = AppConfig::default();
        let mut zone = hotzone(HotzoneId::Bottom, true, ActionKind::VolumeAdjust);
        zone.geometry = Some(HotzoneGeometry::Edge {
            thickness: 16,
            length_percent: 100,
        });
        config.hotzones = vec![zone];
        config.show_hotzone_hint = false;
        let frame = hotzone_frame_for(&config, Point { x: 10, y: 90 }, &[monitor()], false);
        assert_eq!(frame.detected, Some(HotzoneId::Bottom));
        assert_eq!(
            frame.setting.unwrap().actions[0].action.kind,
            ActionKind::VolumeAdjust
        );
        assert_eq!(
            frame.rect,
            Some(Rect {
                left: 0,
                top: 84,
                right: 100,
                bottom: 100
            })
        );
        let paused = hotzone_frame_for(&config, Point { x: 10, y: 90 }, &[monitor()], true);
        assert!(paused.detected.is_none() && paused.setting.is_none() && paused.rect.is_none());
        config.hotzones[0].actions[0].action.kind = ActionKind::None;
        let empty = hotzone_frame_for(&config, Point { x: 10, y: 90 }, &[monitor()], false);
        assert_eq!(empty.detected, Some(HotzoneId::Bottom));
        assert!(empty.setting.is_none() && empty.rect.is_none());
    }

    #[test]
    fn per_monitor_geometry_drives_hint_and_input_from_the_same_region() {
        use crate::config::HotzoneGeometry;
        let first = monitor();
        let second = Monitor {
            bounds: Rect {
                left: 100,
                top: 0,
                right: 200,
                bottom: 100,
            },
            work_area: Rect {
                left: 100,
                top: 0,
                right: 200,
                bottom: 100,
            },
            device_id: [1; 128],
            ..first
        };
        let mut first_zone = hotzone(HotzoneId::Bottom, true, ActionKind::VolumeAdjust);
        first_zone.geometry = Some(HotzoneGeometry::Edge {
            thickness: 16,
            length_percent: 100,
        });
        let mut config = AppConfig::default();
        config.hotzones = vec![hotzone(HotzoneId::Bottom, true, ActionKind::VolumeAdjust)];
        config.monitor_profiles = vec![MonitorProfile {
            monitor_id: monitor_id(&first),
            hotzones: vec![first_zone],
        }];
        let monitors = [first, second];
        for (cursor, expected) in [
            (Point { x: 10, y: 90 }, Some(HotzoneId::Bottom)),
            (Point { x: 110, y: 90 }, None),
            (Point { x: 150, y: 99 }, Some(HotzoneId::Bottom)),
        ] {
            assert_eq!(
                hotzone_hint_for(&config, cursor, &monitors, false),
                expected
            );
            assert_eq!(
                detect_configured_hotzone(
                    cursor,
                    &monitors,
                    config.edge_size,
                    hotzones_for_cursor(&config, cursor, &monitors)
                ),
                expected
            );
        }
        config.show_hotzone_hint = false;
        assert_eq!(
            hotzone_hint_for(&config, Point { x: 10, y: 90 }, &monitors, false),
            None
        );
        assert_eq!(
            detect_configured_hotzone(
                Point { x: 10, y: 90 },
                &monitors,
                config.edge_size,
                hotzones_for_cursor(&config, Point { x: 10, y: 90 }, &monitors)
            ),
            Some(HotzoneId::Bottom)
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

        assert!(is_continuous_adjustment_trigger(
            &volume,
            TriggerKind::WheelUp
        ));
        assert!(is_continuous_adjustment_trigger(
            &volume,
            TriggerKind::SlideBackward
        ));
        assert!(!is_continuous_adjustment_trigger(
            &volume,
            TriggerKind::Hover
        ));
        assert!(!is_continuous_adjustment_trigger(
            &volume,
            TriggerKind::LeftClick
        ));
    }

    #[test]
    fn brightness_uses_continuous_wheel_and_slide_motion() {
        let action = HotzoneAction {
            kind: ActionKind::BrightnessAdjust,
            value: Some("0.05".into()),
        };
        for trigger in [
            TriggerKind::WheelUp,
            TriggerKind::WheelDown,
            TriggerKind::SlideForward,
            TriggerKind::SlideBackward,
        ] {
            assert!(is_continuous_adjustment_trigger(&action, trigger));
        }
        assert!(!is_continuous_adjustment_trigger(
            &action,
            TriggerKind::Hover
        ));
        assert!(!is_continuous_adjustment_trigger(
            &action,
            TriggerKind::LeftClick
        ));
        assert_eq!(
            continuous_action_scale(
                &action,
                TriggerKind::WheelDown,
                InputState {
                    wheel_delta: -240,
                    ..Default::default()
                },
                InputState::default(),
                0,
            ),
            Some(2.0)
        );
        assert_eq!(
            continuous_action_scale(
                &action,
                TriggerKind::SlideForward,
                InputState::default(),
                InputState::default(),
                8,
            ),
            Some(0.5)
        );
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

    fn named_window(process: &str, title: &str, class_name: &str) -> platform::WindowInfo {
        platform::WindowInfo {
            handle: platform::WindowHandle(1),
            rect: Rect {
                left: 0,
                top: 0,
                right: 800,
                bottom: 600,
            },
            title: title.into(),
            class_name: class_name.into(),
            process_name: process.into(),
            maximized: false,
            transient: false,
            arranged: false,
            topmost: false,
        }
    }

    #[test]
    fn window_drag_paused_apps_match_process_title_or_class() {
        // 名单语义与鼠标手势、贴边隐藏一致：进程名、窗口标题、窗口类名任一命中即屏蔽。
        // 大小写不敏感，忽略空白，空项不命中任何窗口。
        let by_process = vec!["photoshop.exe".to_string()];
        assert!(blocks_window_drag(
            &by_process,
            &named_window("Photoshop.exe", "未命名-1", "Photoshop")
        ));
        assert!(!blocks_window_drag(
            &by_process,
            &named_window("notepad.exe", "无标题", "Notepad")
        ));

        let by_title = vec!["illustrator".to_string()];
        assert!(blocks_window_drag(
            &by_title,
            &named_window("unknown.exe", "Adobe Illustrator 2024", "SomeClass")
        ));

        let by_class = vec!["blender".to_string()];
        assert!(blocks_window_drag(
            &by_class,
            &named_window("unknown.exe", "无标题", "BlenderWindow")
        ));

        // 前后空白要被忽略，空字符串不得命中。
        let padded = vec!["  sketchup.exe  ".to_string()];
        assert!(blocks_window_drag(
            &padded,
            &named_window("SketchUp.exe", "模型", "SketchUp")
        ));
        let blank = vec!["   ".to_string(), String::new()];
        assert!(!blocks_window_drag(
            &blank,
            &named_window("SketchUp.exe", "模型", "SketchUp")
        ));

        // 空名单表示不屏蔽任何应用，保持默认零影响。
        assert!(!blocks_window_drag(
            &[],
            &named_window("photoshop.exe", "文档", "Photoshop")
        ));
    }
}
