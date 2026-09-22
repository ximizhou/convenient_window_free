use crate::{supervisor, DesktopState};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

const PENDING_DELAY: Duration = Duration::from_millis(250);
const PENDING_LIFETIME: Duration = Duration::from_secs(15);
const ERROR_LIFETIME: Duration = Duration::from_secs(3);
const RESULT_LIFETIME: Duration = Duration::from_millis(1_200);
const HUD_WIDTH: f64 = 280.0;
const HUD_HEIGHT: f64 = 72.0;

#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Volume,
    Brightness,
}

#[derive(Clone, PartialEq, Deserialize, Serialize)]
pub struct Screen {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    value: f32,
    muted: bool,
    device_name: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feedback {
    interaction: u64,
    sequence: u64,
    kind: Kind,
    screen: Screen,
    level: Option<Level>,
    error: Option<String>,
    pending: bool,
}

impl Feedback {
    fn same_content(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.screen == other.screen
            && self.level == other.level
            && self.error == other.error
            && (self.level.is_some() || self.error.is_some() || self.pending == other.pending)
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum HelperEvent {
    #[serde(rename = "adjustment.updated")]
    Adjustment { data: Feedback },
    #[serde(other)]
    Other,
}

#[derive(Clone, Default, Serialize)]
pub struct Snapshot {
    revision: u64,
    feedback: Option<Feedback>,
}

#[derive(Default)]
pub struct HudState {
    presentation: Mutex<Presentation>,
    scale_changed: AtomicBool,
}

#[derive(Default)]
struct Presentation {
    latest: u64,
    snapshot: Snapshot,
    deadline: Option<Instant>,
    reveal_at: Option<Instant>,
    rendered_revision: u64,
    visible: bool,
    positioned_screen: Option<Screen>,
    layout: Option<(tauri::PhysicalPosition<i32>, tauri::PhysicalSize<u32>)>,
}

impl Presentation {
    fn update(&mut self, feedback: Feedback, now: Instant) -> bool {
        if feedback.sequence < self.latest {
            return false;
        }
        let continues = self.deadline.is_some_and(|deadline| now < deadline)
            && self
                .snapshot
                .feedback
                .as_ref()
                .is_some_and(|previous| previous.interaction == feedback.interaction);
        if !continues {
            self.positioned_screen = None;
        }
        if feedback.pending && feedback.level.is_none() && feedback.error.is_none() {
            if !continues {
                self.reveal_at = Some(now + PENDING_DELAY);
            }
        } else {
            self.reveal_at = None;
        }
        self.latest = feedback.sequence;
        let lifetime = if feedback.pending {
            PENDING_LIFETIME
        } else if feedback.error.is_some() {
            ERROR_LIFETIME
        } else {
            RESULT_LIFETIME
        };
        self.deadline = Some(now + lifetime);
        let changed = !continues
            || self.positioned_screen.as_ref() != Some(&feedback.screen)
            || !self
                .snapshot
                .feedback
                .as_ref()
                .is_some_and(|previous| previous.same_content(&feedback));
        if changed {
            self.snapshot.revision += 1;
        }
        self.snapshot.feedback = Some(feedback);
        changed
    }

    fn expire(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.clear();
            return true;
        }
        false
    }

    fn clear(&mut self) {
        self.deadline = None;
        self.reveal_at = None;
        self.rendered_revision = 0;
        self.visible = false;
        self.positioned_screen = None;
        self.snapshot.revision += 1;
        self.snapshot.feedback = None;
    }

    fn reveal(&mut self, now: Instant) -> bool {
        if self.visible
            || self.rendered_revision != self.snapshot.revision
            || !self.deadline.is_some_and(|deadline| now < deadline)
            || self.reveal_at.is_some_and(|deadline| now < deadline)
        {
            return false;
        }
        self.reveal_at = None;
        true
    }

    fn timer_due(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|deadline| now >= deadline)
            || (self.rendered_revision == self.snapshot.revision
                && self.reveal_at.is_some_and(|deadline| now >= deadline))
    }
}

#[tauri::command]
pub fn adjustment_hud_ready(state: State<'_, HudState>) -> Snapshot {
    state
        .presentation
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .snapshot
        .clone()
}

#[tauri::command]
pub fn adjustment_hud_present(revision: u64, app: AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let state = handle.state::<HudState>();
        let mut state = state.presentation.lock().unwrap_or_else(|e| e.into_inner());
        if state.snapshot.revision != revision {
            return;
        }
        state.rendered_revision = revision;
        let current = state.reveal(Instant::now());
        if current {
            if let Some(window) = handle.get_webview_window("adjustment-hud") {
                state.visible = window.show().is_ok();
            }
        }
    });
}

pub fn start(app: &AppHandle) -> tauri::Result<()> {
    app.manage(HudState::default());
    let mut builder =
        WebviewWindowBuilder::new(app, "adjustment-hud", WebviewUrl::App("hud.html".into()))
            .title("音量与亮度")
            .inner_size(HUD_WIDTH, HUD_HEIGHT)
            .resizable(false)
            .decorations(false)
            .shadow(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .visible(false)
            .background_color(tauri::webview::Color(24, 27, 33, 255));
    if let Some(data_dir) = crate::explicit_data_dir().map_err(std::io::Error::other)? {
        builder = builder.data_directory(data_dir.join("webview-data"));
    }
    let window = builder.build()?;
    window.set_ignore_cursor_events(true)?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::ScaleFactorChanged { .. }) {
            let state = handle.state::<HudState>();
            state.scale_changed.store(true, Ordering::Release);
        }
    });
    let app = app.clone();
    std::thread::spawn(move || listen(app));
    Ok(())
}

fn deliver(app: &AppHandle, feedback: Feedback) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let state = handle.state::<HudState>();
        let scale_changed = state.scale_changed.swap(false, Ordering::AcqRel);
        let mut state = state.presentation.lock().unwrap_or_else(|e| e.into_inner());
        if scale_changed {
            state.positioned_screen = None;
            state.layout = None;
        }
        if !state.update(feedback.clone(), Instant::now()) {
            return;
        }
        if let Some(window) = handle.get_webview_window("adjustment-hud") {
            if state.visible
                && state
                    .reveal_at
                    .is_some_and(|deadline| Instant::now() < deadline)
            {
                if window.hide().is_ok() {
                    state.visible = false;
                }
            }
            let screen = &feedback.screen;
            if state.positioned_screen.as_ref() != Some(screen) {
                // CoreGraphics reports points; the Windows and X11 helpers report pixels.
                let logical = cfg!(target_os = "macos");
                let monitors = window.available_monitors().unwrap_or_default();
                let monitor = monitors.iter().find(|monitor| {
                    let scale = if logical { monitor.scale_factor() } else { 1.0 };
                    (monitor.position().x as f64 / scale - screen.left as f64).abs() < 2.0
                        && (monitor.position().y as f64 / scale - screen.top as f64).abs() < 2.0
                });
                if let Some(monitor) = monitor {
                    let scale = monitor.scale_factor();
                    let width = (HUD_WIDTH * scale).round() as i32;
                    let height = (HUD_HEIGHT * scale).round() as i32;
                    let x = monitor.position().x + (monitor.size().width as i32 - width) / 2;
                    let y = monitor.position().y + monitor.size().height as i32
                        - height
                        - (64.0 * scale) as i32;
                    let position = tauri::PhysicalPosition::new(x, y);
                    let size = tauri::PhysicalSize::new(width as u32, height as u32);
                    if state.layout.as_ref().map(|layout| layout.1) != Some(size)
                        && window.set_size(size).is_err()
                    {
                        return;
                    }
                    if state.layout.as_ref().map(|layout| layout.0) != Some(position)
                        && window.set_position(position).is_err()
                    {
                        return;
                    }
                    state.layout = Some((position, size));
                    state.positioned_screen = Some(screen.clone());
                } else {
                    return;
                }
            }
            let _ = window.emit("adjustment-feedback", state.snapshot.clone());
        }
    });
}

fn tick(app: &AppHandle, reset: bool) {
    if !reset {
        let state = app.state::<HudState>();
        if !state
            .presentation
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .timer_due(Instant::now())
        {
            return;
        }
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let state = handle.state::<HudState>();
        let mut state = state.presentation.lock().unwrap_or_else(|e| e.into_inner());
        let hide = if reset {
            state.latest = 0;
            state.clear();
            true
        } else {
            state.expire(Instant::now())
        };
        let show = state.reveal_at.is_some() && state.reveal(Instant::now());
        let snapshot = hide.then(|| state.snapshot.clone());
        if let Some(snapshot) = snapshot {
            if let Some(window) = handle.get_webview_window("adjustment-hud") {
                let _ = window.emit("adjustment-feedback", snapshot);
                let _ = window.hide();
            }
        } else if show {
            if let Some(window) = handle.get_webview_window("adjustment-hud") {
                state.visible = window.show().is_ok();
            }
        }
    });
}

fn listen(app: AppHandle) {
    loop {
        let state = app.state::<DesktopState>();
        if state
            .shutdown_started
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return;
        }
        let running = state.helper.lock().is_ok_and(|mut helper| helper.running());
        let socket = if running {
            supervisor::read_valid_token(&state.paths.helper_data_dir.join("auth-token"))
                .and_then(|token| supervisor::connect_authenticated(&token))
        } else {
            Err("helper stopped".into())
        };
        if let Ok(mut socket) = socket {
            tick(&app, true);
            let _ = socket
                .get_mut()
                .set_read_timeout(Some(Duration::from_millis(50)));
            loop {
                if state
                    .shutdown_started
                    .load(std::sync::atomic::Ordering::Acquire)
                {
                    return;
                }
                match socket.read() {
                    Ok(tungstenite::Message::Text(text)) => {
                        if let Ok(HelperEvent::Adjustment { data: feedback }) =
                            serde_json::from_str(&text)
                        {
                            deliver(&app, feedback);
                        }
                    }
                    Ok(tungstenite::Message::Close(_)) => break,
                    Ok(_) => {
                        let _ = socket.flush();
                    }
                    Err(tungstenite::Error::Io(error))
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(_) => break,
                }
                tick(&app, false);
            }
            tick(&app, true);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_parser_accepts_feedback_and_ignores_other_helper_messages() {
        let event = serde_json::json!({
            "id": "test", "time": 0, "type": "adjustment.updated",
            "data": feedback(1, true),
        });
        assert!(matches!(
            serde_json::from_value::<HelperEvent>(event).unwrap(),
            HelperEvent::Adjustment { .. }
        ));
        for data in [
            serde_json::json!({"version": "0.6.1"}),
            serde_json::Value::Null,
        ] {
            let event = serde_json::json!({"type": "helper.ready", "data": data});
            assert!(matches!(
                serde_json::from_value::<HelperEvent>(event).unwrap(),
                HelperEvent::Other
            ));
        }
        assert!(serde_json::from_value::<HelperEvent>(serde_json::json!({
            "type": "adjustment.updated", "data": {"pending": true}
        }))
        .is_err());
    }

    fn feedback(sequence: u64, pending: bool) -> Feedback {
        Feedback {
            interaction: 1,
            sequence,
            pending,
            kind: Kind::Brightness,
            screen: Screen {
                left: -1920,
                top: 0,
                right: 0,
                bottom: 1080,
            },
            level: None,
            error: None,
        }
    }
    #[test]
    fn old_results_cannot_replace_or_extend_a_new_action() {
        let now = Instant::now();
        let mut state = Presentation::default();
        assert!(state.update(feedback(2, true), now));
        assert!(!state.update(feedback(1, false), now));
        assert!(state.update(feedback(2, false), now));
        assert!(!state.expire(now + Duration::from_secs(1)));
        assert!(state.expire(now + Duration::from_secs(2)));
        assert!(!state.expire(now + Duration::from_secs(3)));
        assert!(!state.update(feedback(1, false), now));
    }

    fn readback(sequence: u64, value: f32) -> Feedback {
        let mut result = feedback(sequence, false);
        result.level = Some(Level {
            value,
            muted: false,
            device_name: "Display".into(),
        });
        result
    }

    #[test]
    fn unchanged_readbacks_extend_lifetime_without_redrawing_or_showing_again() {
        let now = Instant::now();
        let mut state = Presentation::default();
        let initial = readback(1, 0.4);
        state.update(initial.clone(), now);
        state.positioned_screen = Some(initial.screen);
        state.rendered_revision = state.snapshot.revision;
        assert!(state.reveal(now));
        state.visible = true;
        let revision = state.snapshot.revision;
        for sequence in 2..=100 {
            let mut next = readback(sequence, 0.4);
            next.pending = sequence % 2 == 1;
            assert!(!state.update(next, now + Duration::from_millis(sequence * 30)));
            assert_eq!(state.snapshot.revision, revision);
            assert!(!state.reveal(now));
        }
        assert!(!state.expire(now + Duration::from_millis(4_199)));
        assert!(state.expire(now + Duration::from_millis(4_200)));
        assert!(state.update(readback(101, 0.4), now + Duration::from_secs(5)));
        state.rendered_revision = state.snapshot.revision;
        assert!(state.reveal(now + Duration::from_secs(5)));
    }

    #[test]
    fn device_mute_errors_and_screen_changes_still_redraw() {
        let now = Instant::now();
        let initial = readback(1, 0.4);
        let mut variants = vec![readback(2, 0.5); 5];
        variants[1].level.as_mut().unwrap().muted = true;
        variants[1].level.as_mut().unwrap().value = 0.4;
        variants[2].level.as_mut().unwrap().device_name = "Other display".into();
        variants[2].level.as_mut().unwrap().value = 0.4;
        variants[3].error = Some("Unavailable".into());
        variants[4].screen.left = 0;
        for next in variants {
            let mut state = Presentation::default();
            state.update(initial.clone(), now);
            state.positioned_screen = Some(initial.screen.clone());
            state.visible = true;
            assert!(state.update(next, now));
            assert_eq!(state.snapshot.revision, 2);
        }
        let mut state = Presentation::default();
        state.update(initial.clone(), now);
        state.positioned_screen = Some(initial.screen.clone());
        assert!(!state.update(readback(2, 0.4), now));
        state.positioned_screen = None;
        assert!(state.update(readback(3, 0.4), now));
    }

    #[test]
    fn idle_and_unrendered_pending_do_not_schedule_main_thread_ticks() {
        let now = Instant::now();
        let mut state = Presentation::default();
        assert!(!state.timer_due(now));
        state.update(feedback(1, true), now);
        assert!(!state.timer_due(now + PENDING_DELAY));
        state.rendered_revision = state.snapshot.revision;
        assert!(!state.timer_due(now + Duration::from_millis(249)));
        assert!(state.timer_due(now + PENDING_DELAY));
        assert!(state.reveal(now + PENDING_DELAY));
        state.visible = true;
        assert!(!state.timer_due(now + Duration::from_secs(1)));
        assert!(state.timer_due(now + PENDING_LIFETIME));
        state.expire(now + PENDING_LIFETIME);
        assert!(!state.timer_due(now + PENDING_LIFETIME));
    }

    #[test]
    fn continuous_input_keeps_readback_until_the_next_result() {
        let now = Instant::now();
        let mut state = Presentation::default();
        state.update(readback(1, 0.4), now);
        for sequence in 2..20 {
            let mut pending = readback(sequence, 0.4);
            pending.pending = true;
            state.update(pending, now);
            state.rendered_revision = state.snapshot.revision;
            assert!(state.reveal(now));
            let shown = state.snapshot.feedback.as_ref().unwrap();
            assert!(shown.pending);
            assert_eq!(shown.level.as_ref().unwrap().value, 0.4);
            assert_eq!(shown.level.as_ref().unwrap().device_name, "Display");
        }
        state.update(readback(19, 0.6), now);
        assert_eq!(
            state
                .snapshot
                .feedback
                .as_ref()
                .unwrap()
                .level
                .as_ref()
                .unwrap()
                .value,
            0.6
        );
        let mut failure = feedback(20, false);
        failure.error = Some("Disconnected".into());
        state.update(failure, now);
        assert!(state.snapshot.feedback.as_ref().unwrap().level.is_none());
    }

    #[test]
    fn slow_feedback_is_revealed_once_without_postponement_by_new_input() {
        let now = Instant::now();
        let mut state = Presentation::default();
        for sequence in 1..=5 {
            let time = now + Duration::from_millis(sequence * 40 - 40);
            state.update(feedback(sequence, true), time);
            state.rendered_revision = state.snapshot.revision;
            assert!(!state.reveal(time));
        }
        assert!(!state.reveal(now + Duration::from_millis(249)));
        assert!(state.reveal(now + Duration::from_millis(250)));
        state.update(feedback(6, true), now + Duration::from_millis(300));
        state.rendered_revision = state.snapshot.revision;
        assert!(state.reveal(now + Duration::from_millis(300)));
    }

    #[test]
    fn fast_readback_or_error_cancels_the_initial_wait() {
        let now = Instant::now();
        let mut failure = feedback(2, false);
        failure.error = Some("Disconnected".into());
        for result in [readback(2, 0.4), failure] {
            let mut state = Presentation::default();
            state.update(feedback(1, true), now);
            state.rendered_revision = state.snapshot.revision;
            assert!(!state.reveal(now));
            state.update(result, now + Duration::from_millis(20));
            assert!(!state.reveal(now + Duration::from_millis(20)));
            state.rendered_revision = state.snapshot.revision;
            assert!(state.reveal(now + Duration::from_millis(20)));
            assert!(state.reveal_at.is_none());
            assert!(state.expire(now + Duration::from_secs(4)));
            assert!(!state.reveal(now + Duration::from_secs(4)));
        }
    }

    #[test]
    fn new_interaction_or_expired_hud_starts_a_fresh_wait() {
        let now = Instant::now();
        let mut other_kind = feedback(2, true);
        other_kind.kind = Kind::Volume;
        other_kind.interaction = 2;
        let mut other_screen = feedback(2, true);
        other_screen.screen.left = 0;
        other_screen.interaction = 2;
        for (next, delay, expire) in [
            (other_kind, Duration::ZERO, false),
            (other_screen, Duration::ZERO, false),
            (feedback(2, true), Duration::from_secs(2), false),
            (feedback(2, true), Duration::from_secs(2), true),
        ] {
            let mut state = Presentation::default();
            state.update(readback(1, 0.4), now);
            if expire {
                state.expire(now + delay);
            }
            state.update(next, now + delay);
            assert!(state.snapshot.feedback.as_ref().unwrap().level.is_none());
            state.rendered_revision = state.snapshot.revision;
            assert!(!state.reveal(now + delay));
            assert!(state.reveal(now + delay + Duration::from_millis(250)));
        }
    }
}
