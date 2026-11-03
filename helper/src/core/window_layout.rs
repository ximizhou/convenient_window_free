//! Layout requests execute on the engine thread that owns edge-hide state.
use super::edge_hide::{CollapsedWindow, EdgeHideCommand, EdgeHideController, EdgeHideLiveState};
use crate::config::AppConfig;
#[cfg(test)]
use crate::platform::WindowInfo;
use crate::platform::{self, Monitor, WindowHandle, WindowLayout};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

const MAX_PENDING_REQUESTS: usize = 16;
const MAX_LAYOUT_WINDOWS: usize = 512;
// Hosts time out requests after eight seconds; reply with a typed error first.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowLayoutRestoreError {
    pub(crate) window: WindowLayout,
    pub(crate) details: String,
}

#[derive(Debug)]
pub(crate) struct WindowLayoutRestoreSummary {
    pub(crate) restored: usize,
    pub(crate) skipped: usize,
    pub(crate) errors: Vec<WindowLayoutRestoreError>,
}

enum Request {
    Capture(oneshot::Sender<Result<Vec<WindowLayout>>>),
    Restore(
        Vec<WindowLayout>,
        oneshot::Sender<Result<WindowLayoutRestoreSummary>>,
    ),
}

struct RequestQueue {
    active: bool,
    requests: VecDeque<(Instant, Request)>,
}

static QUEUE: Mutex<RequestQueue> = Mutex::new(RequestQueue {
    active: false,
    requests: VecDeque::new(),
});

fn reset_session(active: bool) {
    let mut queue = QUEUE.lock().unwrap_or_else(|error| error.into_inner());
    queue.active = active;
    for (_, request) in queue.requests.drain(..) {
        let error = anyhow::anyhow!("window layout engine session ended");
        match request {
            Request::Capture(reply) => {
                let _ = reply.send(Err(error));
            }
            Request::Restore(_, reply) => {
                let _ = reply.send(Err(error));
            }
        }
    }
}

pub(crate) fn start_session() {
    reset_session(true);
}
pub(crate) fn stop_session() {
    reset_session(false);
}

fn enqueue(request: Request) -> Result<()> {
    let mut queue = QUEUE.lock().unwrap_or_else(|error| error.into_inner());
    anyhow::ensure!(queue.active, "window layout engine is not running");
    anyhow::ensure!(
        queue.requests.len() < MAX_PENDING_REQUESTS,
        "window layout request queue is full"
    );
    queue
        .requests
        .push_back((Instant::now() + REQUEST_TIMEOUT, request));
    Ok(())
}

pub(crate) async fn capture() -> Result<Vec<WindowLayout>> {
    let (reply, response) = oneshot::channel();
    enqueue(Request::Capture(reply))?;
    tokio::time::timeout(REQUEST_TIMEOUT, response)
        .await
        .context("window layout capture timed out waiting for the engine")?
        .context("window layout engine stopped before capture completed")?
}

pub(crate) async fn restore(layouts: Vec<WindowLayout>) -> Result<WindowLayoutRestoreSummary> {
    anyhow::ensure!(
        layouts.len() <= MAX_LAYOUT_WINDOWS,
        "window layout contains too many entries"
    );
    let (reply, response) = oneshot::channel();
    enqueue(Request::Restore(layouts, reply))?;
    tokio::time::timeout(REQUEST_TIMEOUT, response)
        .await
        .context("window layout restore timed out waiting for the engine")?
        .context("window layout engine stopped before restore completed")?
}

/// Called before the engine reads the foreground window for this tick.
/// Returning true suppresses passive activation restoration after layout moves.
pub(crate) fn process_pending(
    controller: &mut EdgeHideController,
    config: &AppConfig,
    now: Instant,
    mut execute: impl FnMut(EdgeHideCommand) -> Result<EdgeHideLiveState>,
) -> bool {
    let requests: Vec<_> = {
        let mut queue = QUEUE.lock().unwrap_or_else(|error| error.into_inner());
        queue.requests.drain(..).collect()
    };
    // Acquire topology only after taking this batch, so a newly queued restore
    // cannot sneak past a separate pending check and use stale monitor data.
    let needs_topology = requests.iter().any(|(deadline, request)| {
        Instant::now() < *deadline
            && matches!(request, Request::Restore(_, reply) if !reply.is_closed())
    });
    let monitors = if needs_topology {
        platform::monitors().unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut restored = false;
    for (deadline, request) in requests {
        if Instant::now() >= deadline {
            let error = anyhow::anyhow!("window layout request expired before engine processing");
            match request {
                Request::Capture(reply) => {
                    let _ = reply.send(Err(error));
                }
                Request::Restore(_, reply) => {
                    let _ = reply.send(Err(error));
                }
            }
            continue;
        }
        match request {
            Request::Capture(reply) if !reply.is_closed() => {
                let _ = reply.send(capture_current(controller));
            }
            Request::Restore(layouts, reply) if !reply.is_closed() => {
                restored = true;
                let _ = reply.send(restore_current(
                    &layouts,
                    controller,
                    config,
                    &monitors,
                    now,
                    &mut execute,
                ));
            }
            _ => {}
        }
    }
    restored
}

fn collapsed_by_handle(controller: &EdgeHideController) -> HashMap<isize, CollapsedWindow> {
    controller
        .collapsed_windows()
        .into_iter()
        .map(|window| (window.handle.0, window))
        .collect()
}

fn capture_current(controller: &EdgeHideController) -> Result<Vec<WindowLayout>> {
    let mut windows = platform::list_window_layouts()?;
    for window in &mut windows {
        // All owned states, including pending cleanup, retain the user's intent.
        // An untracked window keeps its real native style, including genuine pins.
        if let Some(topmost) = controller.original_topmost(window.handle) {
            window.topmost = topmost;
        }
    }
    Ok(layouts_from_layout_targets(
        windows,
        &collapsed_by_handle(controller),
    ))
}

fn restore_current(
    layouts: &[WindowLayout],
    controller: &mut EdgeHideController,
    config: &AppConfig,
    monitors: &[Monitor],
    now: Instant,
    execute: &mut impl FnMut(EdgeHideCommand) -> Result<EdgeHideLiveState>,
) -> Result<WindowLayoutRestoreSummary> {
    anyhow::ensure!(
        !monitors.is_empty()
            || !config.enabled
            || !config.edge_hide.enabled
            || !layouts.iter().any(|layout| layout.edge_hidden.is_some()),
        "current display topology is unavailable; edge-hidden layout was not applied"
    );
    let mut windows = platform::list_window_layouts()?;
    let collapsed = collapsed_by_handle(controller);
    // Number duplicate identities using the same expanded geometry as capture.
    for window in &mut windows {
        if let Some(hidden) = collapsed.get(&window.handle.0) {
            window.rect = hidden.restore_rect;
            window.placement_rect = None;
        }
    }
    let indexed = windows_with_occurrences(windows);
    let mut used_handles = HashSet::new();
    let mut summary = WindowLayoutRestoreSummary {
        restored: 0,
        skipped: 0,
        errors: Vec::new(),
    };
    for layout in layouts {
        let Some((window, _)) = indexed.iter().find(|(window, occurrence)| {
            *occurrence == layout.occurrence
                && !used_handles.contains(&window.handle)
                && layout_matches(window, layout)
        }) else {
            summary.skipped += 1;
            continue;
        };
        used_handles.insert(window.handle);
        let result = restore_target(
            window.handle,
            layout,
            controller,
            config,
            monitors,
            now,
            execute,
        );
        match result {
            Ok(()) => summary.restored += 1,
            Err(error) => summary.errors.push(WindowLayoutRestoreError {
                window: layout.clone(),
                details: format!("{error:#}"),
            }),
        }
    }
    Ok(summary)
}

fn restore_target(
    handle: WindowHandle,
    layout: &WindowLayout,
    controller: &mut EdgeHideController,
    config: &AppConfig,
    monitors: &[Monitor],
    now: Instant,
    execute: &mut impl FnMut(EdgeHideCommand) -> Result<EdgeHideLiveState>,
) -> Result<()> {
    if controller.original_topmost(handle).is_some() {
        let window = platform::window_info_for_handle(handle)?
            .context("edge-owned layout target is no longer available")?;
        anyhow::ensure!(
            layout_identity(&window.process_name, &window.class_name, &window.title)
                == layout_identity(&layout.process_name, &layout.class_name, &layout.title),
            "layout target identity changed before cleanup"
        );
    }
    if let Some(cleanup) = controller.prepare_layout_restore(handle) {
        execute_owned_command(controller, cleanup, now, execute)
            .context("could not release edge-hide state before layout restore")?;
    }
    // The platform revalidates identity immediately before applying the snapshot.
    // Legacy true values cannot be distinguished from genuine pins; preserve them.
    platform::apply_window_layout(handle, layout)?;
    if let Some(hidden) = layout
        .edge_hidden
        .as_ref()
        .filter(|_| config.enabled && config.edge_hide.enabled)
    {
        let window = platform::window_info_for_handle(handle)?
            .context("restored edge-hide window is unavailable")?;
        anyhow::ensure!(
            layout_identity(&window.process_name, &window.class_name, &window.title)
                == layout_identity(&layout.process_name, &layout.class_name, &layout.title),
            "layout target identity changed before re-collapse"
        );
        let command = controller
            .collapse_now(
                now,
                &config.edge_hide,
                monitors,
                &window,
                hidden.edge,
                layout.rect,
                hidden.hidden_rect,
            )
            .context("saved edge-hide geometry is not available in the current configuration")?;
        execute_owned_command(controller, command, now, execute)
            .context("could not confirm restored edge-hide state")?;
    }
    Ok(())
}

fn execute_owned_command(
    controller: &mut EdgeHideController,
    command: EdgeHideCommand,
    now: Instant,
    execute: &mut impl FnMut(EdgeHideCommand) -> Result<EdgeHideLiveState>,
) -> Result<()> {
    let live = match execute(command) {
        Ok(live) => live,
        Err(error) => {
            controller.command_failed(command, now);
            return Err(error);
        }
    };
    if let Some(cleanup) = controller.command_succeeded(command, live, now) {
        // A rejected/misaligned collapse may need immediate cleanup; acknowledge
        // that cleanup too, while retaining the original operation's failure.
        execute_owned_command(controller, cleanup, now, execute)?;
        anyhow::bail!("edge-hide move did not reach the requested rectangle");
    }
    let confirmed = match command {
        EdgeHideCommand::Restore { handle, .. } => controller.original_topmost(handle).is_none(),
        EdgeHideCommand::Collapse { handle, rect } => controller
            .collapsed_windows()
            .iter()
            .any(|window| window.handle == handle && window.hidden_rect == rect),
    };
    if !confirmed {
        controller.command_failed(command, now);
        anyhow::bail!("edge-hide move could not be confirmed against the live window");
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn layouts_from_windows(windows: Vec<WindowInfo>) -> Vec<WindowLayout> {
    let targets = windows
        .into_iter()
        .enumerate()
        .map(|(z_index, window)| crate::platform::WindowLayoutTarget {
            handle: window.handle,
            z_index: z_index as u32,
            rect: window.rect,
            placement_rect: None,
            title: window.title,
            class_name: window.class_name,
            process_name: window.process_name,
            minimized: false,
            maximized: window.maximized,
            transient: window.transient,
            topmost: window.topmost,
        })
        .collect();
    layouts_from_layout_targets(targets, &HashMap::new())
}

pub(crate) fn layouts_from_layout_targets(
    mut windows: Vec<crate::platform::WindowLayoutTarget>,
    collapsed: &HashMap<isize, crate::core::edge_hide::CollapsedWindow>,
) -> Vec<WindowLayout> {
    // A controller tick may not have observed an external move or minimize yet.
    // Keep its topmost ownership, but only attach edge geometry that is still live.
    for window in &mut windows {
        if let Some(hidden) = collapsed.get(&window.handle.0) {
            window.topmost = hidden.original_topmost;
        }
    }
    let collapsed: HashMap<_, _> = collapsed
        .iter()
        .filter(|(_, hidden)| {
            windows.iter().any(|window| {
                window.handle == hidden.handle
                    && !window.minimized
                    && rects_near(window.rect, hidden.hidden_rect)
            })
        })
        .map(|(handle, hidden)| (*handle, *hidden))
        .collect();
    // Occurrence assignment sorts duplicate identities by geometry, so
    // edge-hidden windows must already be represented by their expanded
    // rectangle here; otherwise capture order and restore order diverge.
    for window in &mut windows {
        if let Some(hidden) = collapsed.get(&window.handle.0) {
            window.rect = hidden.restore_rect;
            window.placement_rect = None;
            window.topmost = hidden.original_topmost;
        }
    }
    windows_with_occurrences(windows)
        .into_iter()
        .map(|(window, occurrence)| {
            let edge_hidden =
                collapsed
                    .get(&window.handle.0)
                    .map(|hidden| crate::platform::EdgeHiddenLayout {
                        edge: hidden.edge,
                        restore_rect: hidden.restore_rect,
                        hidden_rect: hidden.hidden_rect,
                    });
            WindowLayout {
                process_name: window.process_name,
                class_name: window.class_name,
                title: window.title,
                occurrence,
                z_index: window.z_index,
                rect: window.rect,
                placement_rect: window.placement_rect,
                minimized: window.minimized,
                maximized: window.maximized,
                topmost: window.topmost,
                edge_hidden,
            }
        })
        .collect()
}

fn rects_near(left: platform::Rect, right: platform::Rect) -> bool {
    (i64::from(left.left) - i64::from(right.left)).abs() <= 2
        && (i64::from(left.top) - i64::from(right.top)).abs() <= 2
        && (i64::from(left.right) - i64::from(right.right)).abs() <= 2
        && (i64::from(left.bottom) - i64::from(right.bottom)).abs() <= 2
}

fn windows_with_occurrences(
    windows: Vec<crate::platform::WindowLayoutTarget>,
) -> Vec<(crate::platform::WindowLayoutTarget, u32)> {
    let mut windows: Vec<crate::platform::WindowLayoutTarget> = windows
        .into_iter()
        .filter(|window| !window.transient)
        .collect();
    // Enumeration order is usually z-order, which changes as soon as the
    // user focuses a different window. Sorting duplicate identities by their
    // captured geometry makes occurrence assignment stable across a restore
    // round trip instead of swapping two otherwise identical windows.
    windows.sort_by(|left, right| {
        layout_identity(&left.process_name, &left.class_name, &left.title)
            .cmp(&layout_identity(
                &right.process_name,
                &right.class_name,
                &right.title,
            ))
            .then_with(|| {
                layout_sort_rect(left)
                    .left
                    .cmp(&layout_sort_rect(right).left)
            })
            .then_with(|| layout_sort_rect(left).top.cmp(&layout_sort_rect(right).top))
            .then_with(|| {
                layout_sort_rect(left)
                    .right
                    .cmp(&layout_sort_rect(right).right)
            })
            .then_with(|| {
                layout_sort_rect(left)
                    .bottom
                    .cmp(&layout_sort_rect(right).bottom)
            })
    });
    let mut occurrences = HashMap::<(String, String, String), u32>::new();
    windows
        .into_iter()
        .map(|window| {
            let key = layout_identity(&window.process_name, &window.class_name, &window.title);
            let occurrence = occurrences.entry(key).or_insert(0);
            let value = *occurrence;
            *occurrence = occurrence.saturating_add(1);
            (window, value)
        })
        .collect()
}

fn layout_identity(process_name: &str, class_name: &str, title: &str) -> (String, String, String) {
    (
        process_name.to_lowercase(),
        class_name.to_lowercase(),
        title.to_string(),
    )
}

fn layout_matches(window: &crate::platform::WindowLayoutTarget, layout: &WindowLayout) -> bool {
    layout_identity(&window.process_name, &window.class_name, &window.title)
        == layout_identity(&layout.process_name, &layout.class_name, &layout.title)
}

fn layout_sort_rect(window: &crate::platform::WindowLayoutTarget) -> crate::platform::Rect {
    window.placement_rect.unwrap_or(window.rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Edge;
    use crate::platform::Rect;

    #[tokio::test]
    async fn queue_bounds_and_session_reset_complete_old_requests_with_errors() {
        stop_session();
        assert!(capture()
            .await
            .unwrap_err()
            .to_string()
            .contains("not running"));
        start_session();
        let mut responses = Vec::new();
        for _ in 0..MAX_PENDING_REQUESTS {
            let (reply, response) = oneshot::channel();
            enqueue(Request::Capture(reply)).unwrap();
            responses.push(response);
        }
        assert!(capture().await.unwrap_err().to_string().contains("full"));
        start_session();
        for response in responses {
            assert!(response
                .await
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("session ended"));
        }
        let (reply, response) = oneshot::channel();
        enqueue(Request::Restore(Vec::new(), reply)).unwrap();
        stop_session();
        assert!(response
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("session ended"));
        assert!(restore(Vec::new()).await.is_err());

        start_session();
        let (reply, response) = oneshot::channel();
        enqueue(Request::Restore(Vec::new(), reply)).unwrap();
        drop(response);
        let (expired_reply, expired_response) = oneshot::channel();
        QUEUE.lock().unwrap().requests.push_back((
            Instant::now() - Duration::from_secs(1),
            Request::Restore(Vec::new(), expired_reply),
        ));
        assert!(!process_pending(
            &mut EdgeHideController::new(),
            &config(),
            Instant::now(),
            |_| panic!("cancelled and expired requests must not execute")
        ));
        assert!(expired_response
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("expired"));
        stop_session();
    }

    fn monitor() -> Monitor {
        let bounds = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        Monitor {
            bounds,
            work_area: bounds,
            primary: true,
            device_id: [0; 128],
        }
    }

    fn config() -> AppConfig {
        let mut config = AppConfig::default();
        config.enabled = true;
        config.edge_hide.enabled = true;
        config.edge_hide.animation_enabled = false;
        config.edge_hide.edges = vec![Edge::Left, Edge::Right];
        config
    }

    #[cfg(target_os = "windows")]
    fn execute_native(command: EdgeHideCommand) -> Result<EdgeHideLiveState> {
        let (handle, rect, topmost) = match command {
            EdgeHideCommand::Collapse { handle, rect } => (handle, rect, true),
            EdgeHideCommand::Restore {
                handle,
                rect,
                topmost,
            } => (handle, rect, topmost),
        };
        platform::set_window_rect_topmost(handle, rect, topmost)?;
        let window = platform::window_info_for_handle(handle)?.context("test window missing")?;
        Ok(EdgeHideLiveState::Visible(window.rect))
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn isolated_layout_cycles_keep_temporary_topmost_out_of_snapshots_and_release_owned_state() {
        use crate::platform::windows::native_test_windows::{IsolatedDesktop, TestWindow};
        let _desktop = IsolatedDesktop::new();
        let first = TestWindow::new("Layout duplicate ordinary");
        let second = TestWindow::new("Layout duplicate ordinary");
        let pinned = TestWindow::new("Layout genuine pin");
        let pinned_second = TestWindow::new("Layout second genuine pin");
        let fixtures = [&first, &second, &pinned, &pinned_second];
        let mut controller = EdgeHideController::new();
        let mut config = config();
        let monitors = [monitor()];
        let now = Instant::now();
        for (index, window) in fixtures.into_iter().enumerate() {
            let top = 100 + 220 * index as i32;
            let rect = Rect {
                left: 0,
                top,
                right: 240,
                bottom: top + 160,
            };
            let hidden = Rect {
                left: -224,
                top,
                right: 16,
                bottom: top + 160,
            };
            platform::set_window_rect_topmost(window.handle(), rect, index >= 2).unwrap();
            let info = platform::window_info_for_handle(window.handle())
                .unwrap()
                .unwrap();
            let command = controller
                .collapse_now(
                    now,
                    &config.edge_hide,
                    &monitors,
                    &info,
                    Edge::Left,
                    rect,
                    hidden,
                )
                .unwrap();
            execute_owned_command(&mut controller, command, now, &mut execute_native).unwrap();
            assert!(window.topmost());
        }
        // Windows may create its own input-indicator surfaces even on this
        // private desktop; only the four explicitly owned fixtures are tested.
        let fixture_capture = |controller: &EdgeHideController| {
            capture_current(controller)
                .unwrap()
                .into_iter()
                .filter(|window| window.title.starts_with("Layout "))
                .collect::<Vec<_>>()
        };
        let snapshot = fixture_capture(&controller);
        assert_eq!(
            snapshot.len(),
            4,
            "isolated desktop must contain only fixture windows"
        );
        assert_eq!(snapshot.iter().filter(|window| window.topmost).count(), 2);
        let before_failure = fixtures.map(|window| {
            platform::window_info_for_handle(window.handle())
                .unwrap()
                .unwrap()
        });
        let topology_error =
            restore_current(&snapshot, &mut controller, &config, &[], now, &mut |_| {
                panic!("unavailable topology must not execute cleanup or collapse")
            })
            .unwrap_err();
        assert!(topology_error.to_string().contains("topology"));
        for (window, before) in fixtures.iter().zip(before_failure) {
            assert_eq!(
                platform::window_info_for_handle(window.handle())
                    .unwrap()
                    .unwrap(),
                before
            );
        }
        assert!(snapshot.iter().all(|window| window.edge_hidden.is_some()
            && window.rect.left == 0
            && window.placement_rect.is_none()));
        let ordinary: Vec<_> = snapshot
            .iter()
            .filter(|window| window.title == "Layout duplicate ordinary")
            .collect();
        assert_eq!(
            ordinary
                .iter()
                .map(|window| window.occurrence)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        let right_snapshot: Vec<_> = snapshot
            .iter()
            .cloned()
            .map(|mut window| {
                let top = window.rect.top;
                window.rect = Rect {
                    left: 1600,
                    top,
                    right: 1920,
                    bottom: top + 180,
                };
                window.edge_hidden = Some(platform::EdgeHiddenLayout {
                    edge: Edge::Right,
                    restore_rect: window.rect,
                    hidden_rect: Rect {
                        left: 1904,
                        top,
                        right: 2224,
                        bottom: top + 180,
                    },
                });
                window
            })
            .collect();
        let normalize_z = |mut windows: Vec<WindowLayout>| {
            for window in &mut windows {
                window.z_index = 0;
            }
            windows
        };
        for iteration in 0..8 {
            // Move already-collapsed windows to a different historical edge and
            // size, then restore the other snapshot while they are still owned.
            for desired in [&right_snapshot, &snapshot] {
                let restored = restore_current(
                    desired,
                    &mut controller,
                    &config,
                    &monitors,
                    now,
                    &mut execute_native,
                )
                .unwrap();
                assert_eq!(restored.restored, 4);
                assert_eq!(restored.skipped, 0);
                assert!(restored.errors.is_empty(), "{:?}", restored.errors);
                assert_eq!(
                    controller.collapsed_windows().len(),
                    4,
                    "reply follows confirmed recollapse"
                );
                assert!(fixtures.iter().all(|window| window.topmost()));
                assert_eq!(
                    normalize_z(fixture_capture(&controller)),
                    normalize_z(desired.clone())
                );
            }
            if iteration % 2 == 0 {
                config.enabled = false;
            } else {
                config.edge_hide.enabled = false;
            }
            let released = restore_current(
                &snapshot,
                &mut controller,
                &config,
                &monitors,
                now,
                &mut execute_native,
            )
            .unwrap();
            assert_eq!(released.restored, 4);
            assert!(released.errors.is_empty());
            assert!(!controller.has_restore_work());
            assert!(!first.topmost() && !second.topmost());
            assert!(
                pinned.topmost() && pinned_second.topmost(),
                "genuine pinned windows must remain pinned"
            );
            for window in fixtures {
                assert_eq!(
                    platform::window_info_for_handle(window.handle())
                        .unwrap()
                        .unwrap()
                        .rect
                        .left,
                    0
                );
            }
            config.enabled = true;
            config.edge_hide.enabled = true;
        }
        let recollapsed = restore_current(
            &snapshot,
            &mut controller,
            &config,
            &monitors,
            now,
            &mut execute_native,
        )
        .unwrap();
        assert_eq!(recollapsed.restored, 4);
        let failed_handle = first.handle();
        let mut failure = |command| {
            if matches!(command, EdgeHideCommand::Restore { handle, .. } if handle == failed_handle)
            {
                anyhow::bail!("injected cleanup failure");
            }
            execute_native(command)
        };
        let partial = restore_current(
            &snapshot,
            &mut controller,
            &config,
            &monitors,
            now,
            &mut failure,
        )
        .unwrap();
        assert_eq!(partial.restored, 3);
        assert_eq!(partial.errors.len(), 1);
        assert_eq!(
            controller.original_topmost(failed_handle),
            Some(false),
            "failed cleanup must retain recovery state"
        );
        let plain: Vec<_> = snapshot
            .iter()
            .cloned()
            .map(|mut window| {
                window.edge_hidden = None;
                window
            })
            .collect();
        let restored = restore_current(
            &plain,
            &mut controller,
            &config,
            &monitors,
            now,
            &mut execute_native,
        )
        .unwrap();
        assert_eq!(restored.restored, 4);
        assert!(restored.errors.is_empty());
        assert!(!controller.has_restore_work());
        assert!(!first.topmost() && !second.topmost());
        assert!(pinned.topmost() && pinned_second.topmost());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn isolated_capture_uses_live_geometry_after_external_move_before_controller_tick() {
        use crate::platform::windows::native_test_windows::{IsolatedDesktop, TestWindow};
        let _desktop = IsolatedDesktop::new();
        let fixture = TestWindow::new("Layout externally moved");
        let mut controller = EdgeHideController::new();
        let config = config();
        let monitors = [monitor()];
        let now = Instant::now();
        let restore = Rect {
            left: 0,
            top: 100,
            right: 240,
            bottom: 260,
        };
        let hidden = Rect {
            left: -224,
            top: 100,
            right: 16,
            bottom: 260,
        };
        platform::set_window_rect_topmost(fixture.handle(), restore, false).unwrap();
        let info = platform::window_info_for_handle(fixture.handle())
            .unwrap()
            .unwrap();
        let collapse = controller
            .collapse_now(
                now,
                &config.edge_hide,
                &monitors,
                &info,
                Edge::Left,
                restore,
                hidden,
            )
            .unwrap();
        execute_owned_command(&mut controller, collapse, now, &mut execute_native).unwrap();
        let moved = Rect {
            left: 500,
            top: 200,
            right: 740,
            bottom: 360,
        };
        platform::set_window_rect_topmost(fixture.handle(), moved, true).unwrap();
        let captured = capture_current(&controller)
            .unwrap()
            .into_iter()
            .find(|window| window.title == "Layout externally moved")
            .unwrap();
        assert_eq!(
            captured.rect, moved,
            "capture must observe the external move before the next controller tick"
        );
        assert!(captured.edge_hidden.is_none());
        assert!(
            !captured.topmost,
            "temporary topmost is still owned until cleanup"
        );
        assert_eq!(controller.original_topmost(fixture.handle()), Some(false));
    }

    #[test]
    fn capture_keeps_a_genuine_topmost_edge_window() {
        let restore = Rect {
            left: 0,
            top: 100,
            right: 240,
            bottom: 260,
        };
        let hidden = Rect {
            left: -224,
            top: 100,
            right: 16,
            bottom: 260,
        };
        let handle = WindowHandle(1);
        let targets = vec![platform::WindowLayoutTarget {
            handle,
            z_index: 0,
            rect: hidden,
            placement_rect: None,
            title: "Pinned".into(),
            class_name: "Window".into(),
            process_name: "test.exe".into(),
            minimized: false,
            maximized: false,
            transient: false,
            topmost: true,
        }];
        let collapsed = HashMap::from([(
            handle.0,
            CollapsedWindow {
                handle,
                edge: Edge::Left,
                restore_rect: restore,
                hidden_rect: hidden,
                original_topmost: true,
            },
        )]);
        let captured = layouts_from_layout_targets(targets, &collapsed);
        assert!(captured[0].topmost);
        assert_eq!(captured[0].rect, restore);
    }
}
