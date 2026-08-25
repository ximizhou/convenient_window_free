use crate::config::{Edge, EdgeHideConfig};
use crate::platform::{Monitor, Point, Rect, WindowHandle, WindowInfo};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const EDGE_HIDE_PREVIEW_THICKNESS: i32 = 4;
const RELOCATION_RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EdgeHideCommand {
    Collapse {
        handle: WindowHandle,
        rect: Rect,
    },
    Restore {
        handle: WindowHandle,
        rect: Rect,
        topmost: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EdgeHideLiveState {
    Visible(Rect),
    Minimized,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EdgeHideInput {
    pub left_button_down: bool,
    pub right_button_pressed: bool,
    pub context_menu_dismissed: bool,
    pub suppress_foreground_restore: bool,
}

#[derive(Clone, Debug)]
enum EdgeHideState {
    Pending {
        edge: Edge,
        observed_rect: Rect,
        restore_rect: Rect,
        original_topmost: bool,
        since: Instant,
    },
    Collapsing {
        edge: Edge,
        restore_rect: Rect,
        hidden_rect: Rect,
        original_topmost: bool,
        was_foreground: bool,
    },
    Collapsed {
        edge: Edge,
        restore_rect: Rect,
        hidden_rect: Rect,
        original_topmost: bool,
        was_foreground: bool,
    },
    Expanding {
        edge: Edge,
        restore_rect: Rect,
        hidden_rect: Rect,
        original_topmost: bool,
        pointer_entered: bool,
    },
    Relocating {
        edge: Edge,
        restore_rect: Rect,
        hidden_rect: Rect,
        previous_restore_rect: Rect,
        previous_hidden_rect: Rect,
        original_topmost: bool,
        was_foreground: bool,
    },
    Expanded {
        edge: Edge,
        restore_rect: Rect,
        hidden_rect: Rect,
        original_topmost: bool,
        pointer_entered: bool,
        leave_since: Option<Instant>,
    },
    CleaningUp {
        rect: Rect,
        original_topmost: bool,
    },
}

pub struct EdgeHideController {
    states: HashMap<WindowHandle, EdgeHideState>,
    restore_queue: VecDeque<EdgeHideCommand>,
    relocation_retry_after: HashMap<WindowHandle, Instant>,
    context_menu_guard: Option<WindowHandle>,
}

impl EdgeHideController {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            restore_queue: VecDeque::new(),
            relocation_retry_after: HashMap::new(),
            context_menu_guard: None,
        }
    }

    /// Returns strip hints only for collapsed windows that still occupy their hidden rect.
    ///
    /// A live window query is kept at the rendering boundary so an externally moved or hidden
    /// window cannot leave a stale white restore strip behind. Minimized and unavailable windows
    /// keep their recoverable state but do not expose a misleading strip or hotzone.
    pub fn collapsed_strips_with_live_state(
        &self,
        monitors: &[Monitor],
        mut window_state: impl FnMut(WindowHandle, Rect) -> EdgeHideLiveState,
    ) -> Vec<Rect> {
        self.collapsed_strips_filtered(monitors, |handle, hidden_rect| {
            live_window_matches_hidden_rect(hidden_rect, window_state(handle, hidden_rect))
        })
    }

    fn collapsed_strips_filtered(
        &self,
        monitors: &[Monitor],
        mut keep: impl FnMut(WindowHandle, Rect) -> bool,
    ) -> Vec<Rect> {
        self.states
            .iter()
            .filter_map(|(handle, state)| {
                let EdgeHideState::Collapsed {
                    edge,
                    restore_rect,
                    hidden_rect,
                    ..
                } = state
                else {
                    return None;
                };
                if self.relocation_retry_after.contains_key(handle) || !keep(*handle, *hidden_rect)
                {
                    return None;
                }
                let monitor = exposed_monitor_for_restore(*restore_rect, *edge, monitors)?;
                let strip = strip_rect(*hidden_rect, monitor.work_area);
                (strip.width() > 0 && strip.height() > 0).then_some(strip)
            })
            .collect()
    }

    /// Returns the edge segment that would receive the window if the pointer were released now.
    pub fn collapse_preview(
        &self,
        config: &EdgeHideConfig,
        monitors: &[Monitor],
        window: &WindowInfo,
        rect: Rect,
    ) -> Option<Rect> {
        if !config.enabled
            || !config.show_preview
            || self.context_menu_guard == Some(window.handle)
            || !is_candidate_window_at(window, rect, config)
        {
            return None;
        }

        let (edge, monitor) = detect_window_edge(rect, monitors, config)?;
        Some(edge_preview_rect(
            rect,
            monitor.work_area,
            edge,
            config.strip_size,
        ))
    }

    pub fn prune_invalid_windows(&mut self, mut window_exists: impl FnMut(WindowHandle) -> bool) {
        self.states.retain(|handle, _| window_exists(*handle));
        self.relocation_retry_after
            .retain(|handle, _| window_exists(*handle));
        self.restore_queue.retain(|command| {
            let handle = match command {
                EdgeHideCommand::Collapse { handle, .. }
                | EdgeHideCommand::Restore { handle, .. } => *handle,
            };
            window_exists(handle)
        });
        if self
            .context_menu_guard
            .is_some_and(|handle| !window_exists(handle))
        {
            self.context_menu_guard = None;
        }
    }

    pub fn restore_if_needed(&mut self) -> Option<EdgeHideCommand> {
        if self.restore_queue.is_empty() {
            self.queue_restore_all();
        }
        self.restore_queue.pop_front()
    }

    pub fn has_restore_work(&self) -> bool {
        !self.states.is_empty() || !self.restore_queue.is_empty()
    }

    pub fn prepare_restore_all(&mut self, monitors: &[Monitor], strip_size: i32) {
        if self.restore_queue.is_empty() {
            self.reconcile_for_restore(monitors, strip_size);
        }
    }

    pub fn command_succeeded(
        &mut self,
        command: EdgeHideCommand,
        live_state: EdgeHideLiveState,
        now: Instant,
    ) -> Option<EdgeHideCommand> {
        match command {
            EdgeHideCommand::Restore {
                handle,
                rect,
                topmost,
            } => {
                match self.states.get(&handle).cloned() {
                    Some(EdgeHideState::CleaningUp {
                        rect: cleanup_rect,
                        original_topmost,
                    }) if cleanup_rect == rect && original_topmost == topmost => {
                        if live_window_matches_rect(rect, live_state) {
                            self.states.remove(&handle);
                            self.relocation_retry_after.remove(&handle);
                        } else {
                            self.relocation_retry_after
                                .insert(handle, now + RELOCATION_RETRY_DELAY);
                        }
                    }
                    Some(EdgeHideState::Expanding {
                        edge,
                        restore_rect,
                        hidden_rect,
                        original_topmost,
                        pointer_entered,
                    }) if restore_rect == rect && original_topmost == topmost => match live_state {
                        EdgeHideLiveState::Visible(actual)
                            if rects_roughly_equal(actual, restore_rect) =>
                        {
                            self.states.insert(
                                handle,
                                EdgeHideState::Expanded {
                                    edge,
                                    restore_rect,
                                    hidden_rect,
                                    original_topmost,
                                    pointer_entered,
                                    leave_since: None,
                                },
                            );
                            self.relocation_retry_after.remove(&handle);
                        }
                        EdgeHideLiveState::Visible(actual)
                            if rects_roughly_equal(actual, hidden_rect) =>
                        {
                            self.states.insert(
                                handle,
                                EdgeHideState::Collapsed {
                                    edge,
                                    restore_rect,
                                    hidden_rect,
                                    original_topmost,
                                    was_foreground: false,
                                },
                            );
                            self.relocation_retry_after.remove(&handle);
                        }
                        EdgeHideLiveState::Visible(_) => {
                            self.states.remove(&handle);
                            self.relocation_retry_after.remove(&handle);
                        }
                        EdgeHideLiveState::Minimized | EdgeHideLiveState::Unavailable => {
                            self.relocation_retry_after
                                .insert(handle, now + RELOCATION_RETRY_DELAY);
                        }
                    },
                    Some(EdgeHideState::Expanded {
                        edge,
                        restore_rect,
                        hidden_rect,
                        original_topmost,
                        pointer_entered,
                        leave_since,
                    }) if restore_rect == rect && original_topmost == topmost => match live_state {
                        EdgeHideLiveState::Visible(actual)
                            if rects_roughly_equal(actual, hidden_rect) =>
                        {
                            self.states.insert(
                                handle,
                                EdgeHideState::Collapsed {
                                    edge,
                                    restore_rect,
                                    hidden_rect,
                                    original_topmost,
                                    was_foreground: false,
                                },
                            );
                            self.relocation_retry_after.remove(&handle);
                        }
                        EdgeHideLiveState::Visible(actual)
                            if rects_roughly_equal(actual, restore_rect) =>
                        {
                            self.states.insert(
                                handle,
                                EdgeHideState::Expanded {
                                    edge,
                                    restore_rect,
                                    hidden_rect,
                                    original_topmost,
                                    pointer_entered,
                                    leave_since,
                                },
                            );
                            self.relocation_retry_after.remove(&handle);
                        }
                        EdgeHideLiveState::Visible(_) => {
                            self.states.remove(&handle);
                            self.relocation_retry_after.remove(&handle);
                        }
                        EdgeHideLiveState::Minimized | EdgeHideLiveState::Unavailable => {
                            self.states.insert(
                                handle,
                                EdgeHideState::Expanding {
                                    edge,
                                    restore_rect,
                                    hidden_rect,
                                    original_topmost,
                                    pointer_entered,
                                },
                            );
                            self.relocation_retry_after
                                .insert(handle, now + RELOCATION_RETRY_DELAY);
                        }
                    },
                    None if !live_window_matches_rect(rect, live_state) => {
                        self.states.insert(
                            handle,
                            EdgeHideState::CleaningUp {
                                rect,
                                original_topmost: topmost,
                            },
                        );
                        self.relocation_retry_after
                            .insert(handle, now + RELOCATION_RETRY_DELAY);
                    }
                    _ => {}
                }
                None
            }
            EdgeHideCommand::Collapse { handle, rect } => match self.states.get(&handle).cloned() {
                Some(EdgeHideState::Collapsing {
                    edge,
                    restore_rect,
                    hidden_rect,
                    original_topmost,
                    was_foreground,
                }) if hidden_rect == rect => match live_state {
                    EdgeHideLiveState::Visible(actual)
                        if rects_roughly_equal(actual, hidden_rect) =>
                    {
                        self.states.insert(
                            handle,
                            EdgeHideState::Collapsed {
                                edge,
                                restore_rect,
                                hidden_rect,
                                original_topmost,
                                was_foreground,
                            },
                        );
                        None
                    }
                    EdgeHideLiveState::Visible(actual) => {
                        self.states.insert(
                            handle,
                            EdgeHideState::CleaningUp {
                                rect: actual,
                                original_topmost,
                            },
                        );
                        Some(EdgeHideCommand::Restore {
                            handle,
                            rect: actual,
                            topmost: original_topmost,
                        })
                    }
                    EdgeHideLiveState::Minimized | EdgeHideLiveState::Unavailable => None,
                },
                Some(EdgeHideState::Relocating {
                    edge,
                    restore_rect,
                    hidden_rect,
                    previous_restore_rect,
                    previous_hidden_rect,
                    original_topmost,
                    was_foreground,
                }) if hidden_rect == rect => match live_state {
                    EdgeHideLiveState::Visible(actual)
                        if rects_roughly_equal(actual, hidden_rect) =>
                    {
                        self.relocation_retry_after.remove(&handle);
                        self.states.insert(
                            handle,
                            EdgeHideState::Collapsed {
                                edge,
                                restore_rect,
                                hidden_rect,
                                original_topmost,
                                was_foreground,
                            },
                        );
                        None
                    }
                    EdgeHideLiveState::Visible(actual)
                        if !rects_roughly_equal(actual, previous_hidden_rect) =>
                    {
                        self.states.insert(
                            handle,
                            EdgeHideState::CleaningUp {
                                rect: actual,
                                original_topmost,
                            },
                        );
                        self.relocation_retry_after.remove(&handle);
                        Some(EdgeHideCommand::Restore {
                            handle,
                            rect: actual,
                            topmost: original_topmost,
                        })
                    }
                    _ => {
                        self.states.insert(
                            handle,
                            EdgeHideState::Collapsed {
                                edge,
                                restore_rect: previous_restore_rect,
                                hidden_rect: previous_hidden_rect,
                                original_topmost,
                                was_foreground,
                            },
                        );
                        self.relocation_retry_after
                            .insert(handle, now + RELOCATION_RETRY_DELAY);
                        None
                    }
                },
                _ => None,
            },
        }
    }

    pub fn command_failed(&mut self, command: EdgeHideCommand, now: Instant) {
        match command {
            EdgeHideCommand::Collapse { handle, rect } => {
                let previous = match self.states.get(&handle) {
                    Some(EdgeHideState::Relocating {
                        edge,
                        hidden_rect,
                        previous_restore_rect,
                        previous_hidden_rect,
                        original_topmost,
                        was_foreground,
                        ..
                    }) if *hidden_rect == rect => Some(EdgeHideState::Collapsed {
                        edge: *edge,
                        restore_rect: *previous_restore_rect,
                        hidden_rect: *previous_hidden_rect,
                        original_topmost: *original_topmost,
                        was_foreground: *was_foreground,
                    }),
                    _ => None,
                };
                if let Some(previous) = previous {
                    self.states.insert(handle, previous);
                    self.relocation_retry_after
                        .insert(handle, now + RELOCATION_RETRY_DELAY);
                    return;
                }
                let cleanup = match self.states.get(&handle) {
                    Some(EdgeHideState::Collapsing {
                        hidden_rect,
                        restore_rect,
                        original_topmost,
                        ..
                    }) if *hidden_rect == rect => Some((*restore_rect, *original_topmost)),
                    _ => None,
                };
                if let Some((rect, original_topmost)) = cleanup {
                    self.states.insert(
                        handle,
                        EdgeHideState::CleaningUp {
                            rect,
                            original_topmost,
                        },
                    );
                    self.relocation_retry_after
                        .insert(handle, now + RELOCATION_RETRY_DELAY);
                }
            }
            EdgeHideCommand::Restore {
                handle,
                rect,
                topmost,
            } => {
                if matches!(
                    self.states.get(&handle),
                    Some(EdgeHideState::CleaningUp {
                        rect: cleanup_rect,
                        original_topmost,
                    }) if *cleanup_rect == rect && *original_topmost == topmost
                ) {
                    self.relocation_retry_after
                        .insert(handle, now + RELOCATION_RETRY_DELAY);
                    return;
                }
                let next_state = match self.states.get(&handle) {
                    Some(EdgeHideState::Expanding {
                        edge,
                        restore_rect,
                        hidden_rect,
                        original_topmost,
                        pointer_entered,
                    }) if *restore_rect == rect && *original_topmost == topmost => {
                        Some(EdgeHideState::Expanding {
                            edge: *edge,
                            restore_rect: *restore_rect,
                            hidden_rect: *hidden_rect,
                            original_topmost: *original_topmost,
                            pointer_entered: *pointer_entered,
                        })
                    }
                    Some(EdgeHideState::Expanded {
                        edge,
                        restore_rect,
                        hidden_rect,
                        original_topmost,
                        pointer_entered,
                        leave_since,
                    }) if *restore_rect == rect && *original_topmost == topmost => {
                        Some(EdgeHideState::Expanded {
                            edge: *edge,
                            restore_rect: *restore_rect,
                            hidden_rect: *hidden_rect,
                            original_topmost: *original_topmost,
                            pointer_entered: *pointer_entered,
                            leave_since: *leave_since,
                        })
                    }
                    None => Some(EdgeHideState::CleaningUp {
                        rect,
                        original_topmost: topmost,
                    }),
                    _ => None,
                };
                if let Some(next_state) = next_state {
                    self.states.insert(handle, next_state);
                    self.relocation_retry_after
                        .insert(handle, now + RELOCATION_RETRY_DELAY);
                }
            }
        }
    }

    #[cfg(test)]
    pub fn tick(
        &mut self,
        now: Instant,
        config: &EdgeHideConfig,
        cursor: Point,
        monitors: &[Monitor],
        foreground: Option<&WindowInfo>,
    ) -> Option<EdgeHideCommand> {
        self.tick_with_input(
            now,
            config,
            cursor,
            monitors,
            foreground,
            EdgeHideInput::default(),
        )
    }

    #[cfg(test)]
    pub fn tick_with_left_button_state(
        &mut self,
        now: Instant,
        config: &EdgeHideConfig,
        cursor: Point,
        monitors: &[Monitor],
        foreground: Option<&WindowInfo>,
        left_button_down: bool,
    ) -> Option<EdgeHideCommand> {
        self.tick_with_input(
            now,
            config,
            cursor,
            monitors,
            foreground,
            EdgeHideInput {
                left_button_down,
                ..Default::default()
            },
        )
    }

    #[cfg(test)]
    pub fn tick_with_input(
        &mut self,
        now: Instant,
        config: &EdgeHideConfig,
        cursor: Point,
        monitors: &[Monitor],
        foreground: Option<&WindowInfo>,
        input: EdgeHideInput,
    ) -> Option<EdgeHideCommand> {
        self.tick_with_live_state(
            now,
            config,
            cursor,
            monitors,
            foreground,
            input,
            |_, hidden_rect| EdgeHideLiveState::Visible(hidden_rect),
        )
    }

    pub fn tick_with_live_state(
        &mut self,
        now: Instant,
        config: &EdgeHideConfig,
        cursor: Point,
        monitors: &[Monitor],
        foreground: Option<&WindowInfo>,
        input: EdgeHideInput,
        mut window_state: impl FnMut(WindowHandle, Rect) -> EdgeHideLiveState,
    ) -> Option<EdgeHideCommand> {
        self.relocation_retry_after
            .retain(|_, retry_after| now < *retry_after);
        if let Some(command) = self.restore_queue.pop_front() {
            return Some(command);
        }

        if !config.enabled {
            self.reconcile_for_restore(monitors, config.strip_size);
            return self.disable();
        }

        if let Some(command) =
            self.reconcile_monitors(monitors, config.strip_size, &mut window_state)
        {
            return Some(command);
        }
        let active_collapsed = self
            .states
            .iter()
            .filter_map(|(handle, state)| {
                let EdgeHideState::Collapsed {
                    edge,
                    restore_rect,
                    hidden_rect,
                    ..
                } = state
                else {
                    return None;
                };
                (!self.relocation_retry_after.contains_key(handle)
                    && exposed_monitor_for_restore(*restore_rect, *edge, monitors).is_some()
                    && live_window_matches_hidden_rect(
                        *hidden_rect,
                        window_state(*handle, *hidden_rect),
                    ))
                .then_some(*handle)
            })
            .collect::<Vec<_>>();

        if input.context_menu_dismissed || input.left_button_down {
            self.context_menu_guard = None;
        }

        if input.right_button_pressed {
            if let Some(window) = foreground.filter(|window| !window.transient) {
                self.context_menu_guard = Some(window.handle);
            }
            self.suspend_new_collapses();
            return None;
        }

        if let Some(command) = self.handle_collapsed(
            cursor,
            config,
            monitors,
            foreground,
            !input.suppress_foreground_restore,
            &active_collapsed,
        ) {
            return Some(command);
        }

        if input.left_button_down {
            self.suspend_new_collapses();
            return None;
        }

        if let Some(command) = self.handle_expanded(now, config, cursor, monitors, foreground) {
            return Some(command);
        }

        let Some(window) = foreground else {
            self.clear_pending();
            return None;
        };

        self.states.retain(|handle, state| {
            !matches!(state, EdgeHideState::Pending { .. }) || *handle == window.handle
        });

        if self.context_menu_guard == Some(window.handle) {
            self.clear_pending();
            return None;
        }

        if matches!(
            self.states.get(&window.handle),
            Some(
                EdgeHideState::Collapsing { .. }
                    | EdgeHideState::Expanding { .. }
                    | EdgeHideState::Collapsed { .. }
                    | EdgeHideState::Relocating { .. }
                    | EdgeHideState::Expanded { .. }
                    | EdgeHideState::CleaningUp { .. }
            )
        ) {
            return None;
        }

        if !is_candidate_window(window, config) {
            self.states.remove(&window.handle);
            return None;
        }

        let Some((edge, monitor)) = detect_window_edge(window.rect, monitors, config) else {
            if matches!(
                self.states.get(&window.handle),
                Some(EdgeHideState::Pending { .. })
            ) {
                self.states.remove(&window.handle);
            }
            return None;
        };

        match self.states.get(&window.handle) {
            Some(EdgeHideState::Pending {
                edge: pending_edge,
                observed_rect,
                restore_rect,
                original_topmost,
                since,
                ..
            }) if *pending_edge == edge && rects_roughly_equal(*observed_rect, window.rect) => {
                if now.duration_since(*since) >= Duration::from_millis(config.collapse_delay_ms) {
                    let hidden_rect =
                        hidden_rect_for(*restore_rect, monitor.work_area, edge, config.strip_size);
                    self.states.insert(
                        window.handle,
                        EdgeHideState::Collapsing {
                            edge,
                            restore_rect: *restore_rect,
                            hidden_rect,
                            original_topmost: *original_topmost,
                            was_foreground: true,
                        },
                    );
                    Some(EdgeHideCommand::Collapse {
                        handle: window.handle,
                        rect: hidden_rect,
                    })
                } else {
                    None
                }
            }
            _ => {
                self.states.insert(
                    window.handle,
                    EdgeHideState::Pending {
                        edge,
                        observed_rect: window.rect,
                        restore_rect: restore_rect_for(window.rect, monitor.work_area),
                        original_topmost: window.topmost,
                        since: now,
                    },
                );
                None
            }
        }
    }

    fn handle_collapsed(
        &mut self,
        cursor: Point,
        config: &EdgeHideConfig,
        monitors: &[Monitor],
        foreground: Option<&WindowInfo>,
        allow_foreground_restore: bool,
        active_collapsed: &[WindowHandle],
    ) -> Option<EdgeHideCommand> {
        let manually_moved = foreground.and_then(|window| {
            let EdgeHideState::Collapsed {
                hidden_rect,
                original_topmost,
                ..
            } = self.states.get(&window.handle)?
            else {
                return None;
            };

            (!rects_roughly_equal(window.rect, *hidden_rect) && window.rect.contains(cursor))
                .then_some((window.handle, window.rect, *original_topmost))
        });
        if let Some((handle, rect, topmost)) = manually_moved {
            self.states.remove(&handle);
            return Some(EdgeHideCommand::Restore {
                handle,
                rect,
                topmost,
            });
        }

        let foreground_handle = foreground.map(|window| window.handle);
        let mut activated = None;
        for (handle, state) in &mut self.states {
            if let EdgeHideState::Collapsed {
                edge,
                restore_rect,
                hidden_rect,
                original_topmost,
                was_foreground,
            } = state
            {
                let is_foreground = foreground_handle == Some(*handle);
                if !active_collapsed.contains(handle) {
                    *was_foreground = is_foreground;
                    continue;
                }
                if allow_foreground_restore && is_foreground && !*was_foreground {
                    activated = Some((
                        *handle,
                        *edge,
                        *restore_rect,
                        *hidden_rect,
                        *original_topmost,
                    ));
                }
                *was_foreground = is_foreground;
            }
        }

        let target = self.states.iter().find_map(|(handle, state)| {
            if let EdgeHideState::Collapsed {
                edge,
                restore_rect,
                hidden_rect,
                original_topmost,
                ..
            } = state
            {
                if !active_collapsed.contains(handle) {
                    return None;
                }
                let work_area =
                    exposed_monitor_for_restore(*restore_rect, *edge, monitors)?.work_area;
                let visible = strip_rect(*hidden_rect, work_area);
                if visible
                    .inflate(config.trigger_distance.max(1))
                    .contains(cursor)
                {
                    return Some((
                        *handle,
                        *edge,
                        *restore_rect,
                        *hidden_rect,
                        *original_topmost,
                    ));
                }
            }
            None
        });

        let (handle, edge, restore_rect, hidden_rect, original_topmost, pointer_entered) =
            if let Some((handle, edge, restore_rect, hidden_rect, original_topmost)) = activated {
                (
                    handle,
                    edge,
                    restore_rect,
                    hidden_rect,
                    original_topmost,
                    false,
                )
            } else {
                let (handle, edge, restore_rect, hidden_rect, original_topmost) = target?;
                (
                    handle,
                    edge,
                    restore_rect,
                    hidden_rect,
                    original_topmost,
                    restore_rect
                        .inflate(config.trigger_distance.max(1))
                        .contains(cursor),
                )
            };
        self.states.insert(
            handle,
            EdgeHideState::Expanding {
                edge,
                restore_rect,
                hidden_rect,
                original_topmost,
                pointer_entered,
            },
        );
        Some(EdgeHideCommand::Restore {
            handle,
            rect: restore_rect,
            topmost: original_topmost,
        })
    }

    fn handle_expanded(
        &mut self,
        now: Instant,
        config: &EdgeHideConfig,
        cursor: Point,
        _monitors: &[Monitor],
        foreground: Option<&WindowInfo>,
    ) -> Option<EdgeHideCommand> {
        let mut remove_handle = None;
        if let Some(window) = foreground {
            if let Some(EdgeHideState::Expanded {
                restore_rect,
                hidden_rect,
                original_topmost,
                ..
            }) = self.states.get_mut(&window.handle)
            {
                *original_topmost = window.topmost;
                if !rects_roughly_equal(window.rect, *restore_rect)
                    && !rects_roughly_equal(window.rect, *hidden_rect)
                {
                    remove_handle = Some(window.handle);
                }
            }
        }
        if let Some(handle) = remove_handle {
            self.states.remove(&handle);
        }

        let target = self.states.iter_mut().find_map(|(handle, state)| {
            if let EdgeHideState::Expanded {
                edge,
                restore_rect,
                hidden_rect,
                original_topmost,
                pointer_entered,
                leave_since,
            } = state
            {
                if restore_rect
                    .inflate(config.trigger_distance.max(1))
                    .contains(cursor)
                {
                    *pointer_entered = true;
                    *leave_since = None;
                    return None;
                }

                if config.keep_expanded_when_foreground
                    && foreground.is_some_and(|window| window.handle == *handle)
                {
                    *leave_since = None;
                    return None;
                }

                if !*pointer_entered {
                    *leave_since = None;
                    return None;
                }

                if let Some(since) = leave_since {
                    if now.duration_since(*since) >= Duration::from_millis(config.restore_delay_ms)
                    {
                        return Some((
                            *handle,
                            *edge,
                            *restore_rect,
                            *hidden_rect,
                            *original_topmost,
                        ));
                    }
                } else {
                    *leave_since = Some(now);
                }
            }
            None
        });

        let (handle, edge, restore_rect, hidden_rect, original_topmost) = target?;
        self.states.insert(
            handle,
            EdgeHideState::Collapsing {
                edge,
                restore_rect,
                hidden_rect,
                original_topmost,
                was_foreground: foreground.is_some_and(|window| window.handle == handle),
            },
        );
        Some(EdgeHideCommand::Collapse {
            handle,
            rect: hidden_rect,
        })
    }

    fn queue_restore_all(&mut self) {
        self.relocation_retry_after.clear();
        for (handle, state) in self.states.drain() {
            match state {
                EdgeHideState::Collapsing {
                    restore_rect,
                    original_topmost,
                    ..
                } => {
                    self.restore_queue.push_back(EdgeHideCommand::Restore {
                        handle,
                        rect: restore_rect,
                        topmost: original_topmost,
                    });
                }
                EdgeHideState::Collapsed {
                    restore_rect,
                    original_topmost,
                    ..
                } => {
                    self.restore_queue.push_back(EdgeHideCommand::Restore {
                        handle,
                        rect: restore_rect,
                        topmost: original_topmost,
                    });
                }
                EdgeHideState::Expanded {
                    restore_rect,
                    original_topmost,
                    ..
                }
                | EdgeHideState::Expanding {
                    restore_rect,
                    original_topmost,
                    ..
                }
                | EdgeHideState::Relocating {
                    restore_rect,
                    original_topmost,
                    ..
                } => {
                    self.restore_queue.push_back(EdgeHideCommand::Restore {
                        handle,
                        rect: restore_rect,
                        topmost: original_topmost,
                    });
                }
                EdgeHideState::CleaningUp {
                    rect,
                    original_topmost,
                } => {
                    self.restore_queue.push_back(EdgeHideCommand::Restore {
                        handle,
                        rect,
                        topmost: original_topmost,
                    });
                }
                EdgeHideState::Pending { .. } => {}
            }
        }
    }

    fn suspend_new_collapses(&mut self) {
        self.states.retain(|_, state| {
            if let EdgeHideState::Expanded { leave_since, .. } = state {
                *leave_since = None;
            }
            !matches!(state, EdgeHideState::Pending { .. })
        });
    }

    fn clear_pending(&mut self) {
        self.states
            .retain(|_, state| !matches!(state, EdgeHideState::Pending { .. }));
    }

    fn reconcile_monitors(
        &mut self,
        monitors: &[Monitor],
        strip_size: i32,
        window_state: &mut impl FnMut(WindowHandle, Rect) -> EdgeHideLiveState,
    ) -> Option<EdgeHideCommand> {
        for state in self.states.values_mut() {
            let (EdgeHideState::Expanded {
                edge,
                restore_rect,
                hidden_rect,
                ..
            }
            | EdgeHideState::Expanding {
                edge,
                restore_rect,
                hidden_rect,
                ..
            }) = state
            else {
                continue;
            };
            let Some(monitor) = monitor_for_edge_restore(*restore_rect, *edge, monitors) else {
                continue;
            };
            let adjusted_restore = restore_rect_for(*restore_rect, monitor.work_area);
            *restore_rect = adjusted_restore;
            *hidden_rect = hidden_rect_for(adjusted_restore, monitor.work_area, *edge, strip_size);
        }

        let handles = self.states.keys().copied().collect::<Vec<_>>();
        for handle in handles {
            if self.relocation_retry_after.contains_key(&handle) {
                continue;
            }
            match self.states.get(&handle).cloned() {
                Some(EdgeHideState::Expanding {
                    edge,
                    restore_rect,
                    hidden_rect,
                    original_topmost,
                    pointer_entered,
                }) => match window_state(handle, restore_rect) {
                    EdgeHideLiveState::Visible(actual)
                        if rects_roughly_equal(actual, restore_rect) =>
                    {
                        self.states.insert(
                            handle,
                            EdgeHideState::Expanded {
                                edge,
                                restore_rect,
                                hidden_rect,
                                original_topmost,
                                pointer_entered,
                                leave_since: None,
                            },
                        );
                    }
                    EdgeHideLiveState::Visible(actual)
                        if rects_roughly_equal(actual, hidden_rect) =>
                    {
                        self.states.insert(
                            handle,
                            EdgeHideState::Collapsed {
                                edge,
                                restore_rect,
                                hidden_rect,
                                original_topmost,
                                was_foreground: false,
                            },
                        );
                    }
                    EdgeHideLiveState::Visible(_) => {
                        self.states.remove(&handle);
                    }
                    EdgeHideLiveState::Minimized | EdgeHideLiveState::Unavailable => {}
                },
                Some(EdgeHideState::Collapsing {
                    edge,
                    restore_rect,
                    hidden_rect,
                    original_topmost,
                    was_foreground,
                }) => match window_state(handle, hidden_rect) {
                    EdgeHideLiveState::Visible(actual)
                        if rects_roughly_equal(actual, hidden_rect) =>
                    {
                        self.states.insert(
                            handle,
                            EdgeHideState::Collapsed {
                                edge,
                                restore_rect,
                                hidden_rect,
                                original_topmost,
                                was_foreground,
                            },
                        );
                    }
                    EdgeHideLiveState::Visible(actual) => {
                        self.states.insert(
                            handle,
                            EdgeHideState::CleaningUp {
                                rect: actual,
                                original_topmost,
                            },
                        );
                        return Some(EdgeHideCommand::Restore {
                            handle,
                            rect: actual,
                            topmost: original_topmost,
                        });
                    }
                    EdgeHideLiveState::Minimized | EdgeHideLiveState::Unavailable => {}
                },
                Some(EdgeHideState::CleaningUp {
                    rect,
                    original_topmost,
                }) => {
                    return Some(EdgeHideCommand::Restore {
                        handle,
                        rect,
                        topmost: original_topmost,
                    });
                }
                _ => {}
            }
            let Some(EdgeHideState::Collapsed {
                edge,
                restore_rect,
                hidden_rect,
                original_topmost,
                was_foreground,
            }) = self.states.get(&handle).cloned()
            else {
                continue;
            };
            let Some(monitor) = monitor_for_edge_restore(restore_rect, edge, monitors) else {
                continue;
            };
            let adjusted_restore = restore_rect_for(restore_rect, monitor.work_area);
            let adjusted_hidden =
                hidden_rect_for(adjusted_restore, monitor.work_area, edge, strip_size);
            if adjusted_restore == restore_rect && adjusted_hidden == hidden_rect {
                continue;
            }

            let live_state = window_state(handle, hidden_rect);
            if live_window_matches_hidden_rect(adjusted_hidden, live_state) {
                self.states.insert(
                    handle,
                    EdgeHideState::Collapsed {
                        edge,
                        restore_rect: adjusted_restore,
                        hidden_rect: adjusted_hidden,
                        original_topmost,
                        was_foreground,
                    },
                );
                continue;
            }
            if !live_window_matches_hidden_rect(hidden_rect, live_state) {
                continue;
            }

            self.states.insert(
                handle,
                EdgeHideState::Relocating {
                    edge,
                    restore_rect: adjusted_restore,
                    hidden_rect: adjusted_hidden,
                    previous_restore_rect: restore_rect,
                    previous_hidden_rect: hidden_rect,
                    original_topmost,
                    was_foreground,
                },
            );
            return Some(EdgeHideCommand::Collapse {
                handle,
                rect: adjusted_hidden,
            });
        }
        None
    }

    fn reconcile_for_restore(&mut self, monitors: &[Monitor], strip_size: i32) {
        for state in self.states.values_mut() {
            let (edge, restore_rect, hidden_rect) = match state {
                EdgeHideState::Collapsed {
                    edge,
                    restore_rect,
                    hidden_rect,
                    ..
                }
                | EdgeHideState::Expanded {
                    edge,
                    restore_rect,
                    hidden_rect,
                    ..
                }
                | EdgeHideState::Expanding {
                    edge,
                    restore_rect,
                    hidden_rect,
                    ..
                }
                | EdgeHideState::Relocating {
                    edge,
                    restore_rect,
                    hidden_rect,
                    ..
                } => (edge, restore_rect, hidden_rect),
                EdgeHideState::Collapsing {
                    edge,
                    restore_rect,
                    hidden_rect,
                    ..
                } => (edge, restore_rect, hidden_rect),
                EdgeHideState::Pending { .. } | EdgeHideState::CleaningUp { .. } => continue,
            };
            let Some(monitor) = monitor_for_rect(*restore_rect, monitors) else {
                continue;
            };
            let adjusted_restore = restore_rect_for(*restore_rect, monitor.work_area);
            *restore_rect = adjusted_restore;
            *hidden_rect = hidden_rect_for(adjusted_restore, monitor.work_area, *edge, strip_size);
        }
    }

    fn disable(&mut self) -> Option<EdgeHideCommand> {
        self.context_menu_guard = None;
        if self.restore_queue.is_empty() {
            self.queue_restore_all();
        }
        self.restore_queue.pop_front()
    }
}

fn is_candidate_window(window: &WindowInfo, config: &EdgeHideConfig) -> bool {
    is_candidate_window_at(window, window.rect, config)
}

fn is_candidate_window_at(window: &WindowInfo, rect: Rect, config: &EdgeHideConfig) -> bool {
    if window.transient
        || window.arranged
        || window.maximized
        || rect.width() < 120
        || rect.height() < 80
    {
        return false;
    }

    let process = window.process_name.to_lowercase();
    let title = window.title.to_lowercase();
    let class_name = window.class_name.to_lowercase();

    if is_system_shell_surface(&process, &title, &class_name) {
        return false;
    }

    !config.excluded_apps.iter().any(|item| {
        let item = item.to_lowercase();
        !item.is_empty()
            && (process.contains(&item) || title.contains(&item) || class_name.contains(&item))
    })
}

fn is_system_shell_surface(process: &str, title: &str, class_name: &str) -> bool {
    if matches!(
        process,
        "shellexperiencehost.exe"
            | "shellexperiencehost"
            | "startmenuexperiencehost.exe"
            | "startmenuexperiencehost"
            | "searchhost.exe"
            | "searchhost"
            | "textinputhost.exe"
            | "textinputhost"
            | "widgets.exe"
            | "widgets"
            | "lockapp.exe"
            | "lockapp"
    ) {
        return true;
    }

    matches!(
        class_name,
        "progman"
            | "workerw"
            | "shell_traywnd"
            | "shell_secondarytraywnd"
            | "controlcenterwindow"
            | "xamlexplorerhostislandwindow_wasdk"
            | "notifyiconoverflowwindow"
            | "toplevelwindowforoverflowxamlisland"
            | "edgeuiinputtopwndclass"
    ) || (process == "explorer.exe" || process == "explorer")
        && class_name == "applicationframewindow"
        && title.trim().is_empty()
}

fn detect_window_edge<'a>(
    rect: Rect,
    monitors: &'a [Monitor],
    config: &EdgeHideConfig,
) -> Option<(Edge, &'a Monitor)> {
    let monitor = monitor_for_rect(rect, monitors)?;
    let area = monitor.work_area;
    let distance = config.trigger_distance.max(1);

    for edge in edges_for_monitor(config, monitor) {
        let near = match edge {
            Edge::Left => {
                (config.distance_trigger_enabled && (rect.left - area.left).abs() <= distance)
                    || ratio_triggered(rect, area, Edge::Left, config)
            }
            Edge::Top => {
                (config.distance_trigger_enabled && (rect.top - area.top).abs() <= distance)
                    || ratio_triggered(rect, area, Edge::Top, config)
            }
            Edge::Right => {
                (config.distance_trigger_enabled && (rect.right - area.right).abs() <= distance)
                    || ratio_triggered(rect, area, Edge::Right, config)
            }
            Edge::Bottom => {
                (config.distance_trigger_enabled && (rect.bottom - area.bottom).abs() <= distance)
                    || ratio_triggered(rect, area, Edge::Bottom, config)
            }
        };
        if near && is_edge_exposed_for_rect(monitor, *edge, rect, monitors) {
            return Some((*edge, monitor));
        }
    }

    None
}

fn edges_for_monitor<'a>(config: &'a EdgeHideConfig, monitor: &Monitor) -> &'a [Edge] {
    let monitor_id = monitor.id();
    config
        .monitor_profiles
        .iter()
        .find(|profile| profile.monitor_id == monitor_id)
        .map(|profile| profile.edges.as_slice())
        .unwrap_or(config.edges.as_slice())
}

fn is_edge_exposed_for_rect(
    monitor: &Monitor,
    edge: Edge,
    rect: Rect,
    monitors: &[Monitor],
) -> bool {
    const ADJACENCY_TOLERANCE: i32 = 1;

    !monitors.iter().any(|other| {
        if std::ptr::eq(other, monitor) {
            return false;
        }

        let perpendicular_overlap = match edge {
            Edge::Left | Edge::Right => {
                rect.bottom.min(other.bounds.bottom) - rect.top.max(other.bounds.top) > 0
            }
            Edge::Top | Edge::Bottom => {
                rect.right.min(other.bounds.right) - rect.left.max(other.bounds.left) > 0
            }
        };
        if !perpendicular_overlap {
            return false;
        }

        match edge {
            Edge::Left => {
                other.bounds.left < monitor.bounds.left
                    && other.bounds.right >= monitor.bounds.left - ADJACENCY_TOLERANCE
            }
            Edge::Right => {
                other.bounds.right > monitor.bounds.right
                    && other.bounds.left <= monitor.bounds.right + ADJACENCY_TOLERANCE
            }
            Edge::Top => {
                other.bounds.top < monitor.bounds.top
                    && other.bounds.bottom >= monitor.bounds.top - ADJACENCY_TOLERANCE
            }
            Edge::Bottom => {
                other.bounds.bottom > monitor.bounds.bottom
                    && other.bounds.top <= monitor.bounds.bottom + ADJACENCY_TOLERANCE
            }
        }
    })
}

fn monitor_for_rect(rect: Rect, monitors: &[Monitor]) -> Option<&Monitor> {
    monitors
        .iter()
        .max_by_key(|monitor| monitor_rect_score(rect, monitor.bounds))
}

fn monitor_for_edge_restore(
    restore_rect: Rect,
    edge: Edge,
    monitors: &[Monitor],
) -> Option<&Monitor> {
    if let Some(monitor) = monitor_for_rect(restore_rect, monitors) {
        let adjusted = restore_rect_for(restore_rect, monitor.work_area);
        if is_edge_exposed_for_rect(monitor, edge, adjusted, monitors) {
            return Some(monitor);
        }
    }

    monitors
        .iter()
        .filter(|monitor| {
            let adjusted = restore_rect_for(restore_rect, monitor.work_area);
            is_edge_exposed_for_rect(monitor, edge, adjusted, monitors)
        })
        .max_by_key(|monitor| monitor_rect_score(restore_rect, monitor.bounds))
}

fn monitor_rect_score(rect: Rect, area: Rect) -> (bool, i64, i64) {
    let overlap_width = (rect.right.min(area.right) - rect.left.max(area.left)).max(0) as i64;
    let overlap_height = (rect.bottom.min(area.bottom) - rect.top.max(area.top)).max(0) as i64;
    let overlap_area = overlap_width * overlap_height;

    let dx = if rect.right < area.left {
        (area.left - rect.right) as i64
    } else if rect.left > area.right {
        (rect.left - area.right) as i64
    } else {
        0
    };
    let dy = if rect.bottom < area.top {
        (area.top - rect.bottom) as i64
    } else if rect.top > area.bottom {
        (rect.top - area.bottom) as i64
    } else {
        0
    };
    let distance_squared = dx * dx + dy * dy;

    (overlap_area > 0, overlap_area, -distance_squared)
}

fn render_monitor_for_rect(rect: Rect, monitors: &[Monitor]) -> Option<&Monitor> {
    let monitor = monitor_for_rect(rect, monitors)?;
    let overlap_width = rect.right.min(monitor.bounds.right) - rect.left.max(monitor.bounds.left);
    let overlap_height = rect.bottom.min(monitor.bounds.bottom) - rect.top.max(monitor.bounds.top);
    (overlap_width > 0 && overlap_height > 0).then_some(monitor)
}

fn exposed_monitor_for_restore(
    restore_rect: Rect,
    edge: Edge,
    monitors: &[Monitor],
) -> Option<&Monitor> {
    let monitor = render_monitor_for_rect(restore_rect, monitors)?;
    is_edge_exposed_for_rect(monitor, edge, restore_rect, monitors).then_some(monitor)
}

fn outside_amount(rect: Rect, area: Rect, edge: Edge) -> i32 {
    match edge {
        Edge::Left => (area.left - rect.left).max(0),
        Edge::Top => (area.top - rect.top).max(0),
        Edge::Right => (rect.right - area.right).max(0),
        Edge::Bottom => (rect.bottom - area.bottom).max(0),
    }
}

fn ratio_triggered(rect: Rect, area: Rect, edge: Edge, config: &EdgeHideConfig) -> bool {
    if !config.ratio_trigger_enabled {
        return false;
    }

    let outside = outside_amount(rect, area, edge) as i64;
    let dimension = match edge {
        Edge::Left | Edge::Right => rect.width(),
        Edge::Top | Edge::Bottom => rect.height(),
    }
    .max(1) as i64;
    outside > 0 && outside * 100 >= dimension * config.trigger_ratio.clamp(1, 100) as i64
}

fn rects_roughly_equal(a: Rect, b: Rect) -> bool {
    (a.left - b.left).abs() <= 2
        && (a.top - b.top).abs() <= 2
        && (a.right - b.right).abs() <= 2
        && (a.bottom - b.bottom).abs() <= 2
}

fn live_window_matches_hidden_rect(hidden_rect: Rect, state: EdgeHideLiveState) -> bool {
    matches!(state, EdgeHideLiveState::Visible(rect) if rects_roughly_equal(rect, hidden_rect))
}

fn live_window_matches_rect(rect: Rect, state: EdgeHideLiveState) -> bool {
    matches!(state, EdgeHideLiveState::Visible(actual) if rects_roughly_equal(actual, rect))
}

fn restore_rect_for(original: Rect, work_area: Rect) -> Rect {
    let width = original.width();
    let height = original.height();
    let area_width = work_area.width();
    let area_height = work_area.height();

    let left = if width >= area_width {
        work_area.left
    } else {
        original.left.clamp(work_area.left, work_area.right - width)
    };
    let top = if height >= area_height {
        work_area.top
    } else {
        original.top.clamp(work_area.top, work_area.bottom - height)
    };

    Rect {
        left,
        top,
        right: (left + width).min(work_area.right),
        bottom: (top + height).min(work_area.bottom),
    }
}

fn hidden_rect_for(restore_rect: Rect, work_area: Rect, edge: Edge, strip_size: i32) -> Rect {
    let strip = strip_size.max(4);
    let w = restore_rect.width();
    let h = restore_rect.height();
    match edge {
        Edge::Left => Rect {
            left: work_area.left - (w - strip),
            top: restore_rect.top,
            right: work_area.left + strip,
            bottom: restore_rect.top + h,
        },
        Edge::Right => Rect {
            left: work_area.right - strip,
            top: restore_rect.top,
            right: work_area.right + (w - strip),
            bottom: restore_rect.top + h,
        },
        Edge::Top => Rect {
            left: restore_rect.left,
            top: work_area.top - (h - strip),
            right: restore_rect.left + w,
            bottom: work_area.top + strip,
        },
        Edge::Bottom => Rect {
            left: restore_rect.left,
            top: work_area.bottom - strip,
            right: restore_rect.left + w,
            bottom: work_area.bottom + (h - strip),
        },
    }
}

fn edge_preview_rect(rect: Rect, work_area: Rect, edge: Edge, strip_size: i32) -> Rect {
    let restore_rect = restore_rect_for(rect, work_area);
    let hidden_rect = hidden_rect_for(restore_rect, work_area, edge, strip_size);
    let visible_strip = strip_rect(hidden_rect, work_area);
    let thickness = EDGE_HIDE_PREVIEW_THICKNESS.max(1);

    match edge {
        Edge::Left => Rect {
            left: work_area.left,
            top: visible_strip.top,
            right: (work_area.left + thickness).min(work_area.right),
            bottom: visible_strip.bottom,
        },
        Edge::Top => Rect {
            left: visible_strip.left,
            top: work_area.top,
            right: visible_strip.right,
            bottom: (work_area.top + thickness).min(work_area.bottom),
        },
        Edge::Right => Rect {
            left: (work_area.right - thickness).max(work_area.left),
            top: visible_strip.top,
            right: work_area.right,
            bottom: visible_strip.bottom,
        },
        Edge::Bottom => Rect {
            left: visible_strip.left,
            top: (work_area.bottom - thickness).max(work_area.top),
            right: visible_strip.right,
            bottom: work_area.bottom,
        },
    }
}

fn strip_rect(hidden_rect: Rect, work_area: Rect) -> Rect {
    Rect {
        left: hidden_rect.left.max(work_area.left),
        top: hidden_rect.top.max(work_area.top),
        right: hidden_rect.right.min(work_area.right),
        bottom: hidden_rect.bottom.min(work_area.bottom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EdgeHideConfig, EdgeHideMonitorProfile};

    fn monitor() -> Monitor {
        Monitor {
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
        }
    }

    fn window(rect: Rect) -> WindowInfo {
        window_with_handle(WindowHandle(42), rect)
    }

    fn window_with_handle(handle: WindowHandle, rect: Rect) -> WindowInfo {
        WindowInfo {
            handle,
            rect,
            title: "Editor".to_string(),
            class_name: "Window".to_string(),
            process_name: "editor.exe".to_string(),
            maximized: false,
            transient: false,
            arranged: false,
            topmost: false,
        }
    }

    fn left_and_primary_monitors() -> [Monitor; 2] {
        [
            monitor(),
            Monitor {
                bounds: Rect {
                    left: -1920,
                    top: 0,
                    right: 0,
                    bottom: 1080,
                },
                work_area: Rect {
                    left: -1920,
                    top: 0,
                    right: 0,
                    bottom: 1040,
                },
                primary: false,
                device_id: [0; 128],
            },
        ]
    }

    fn collapsed_controller_on_left_monitor() -> EdgeHideController {
        let mut controller = EdgeHideController::new();
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: Rect {
                    left: 0,
                    top: 120,
                    right: 600,
                    bottom: 700,
                },
                hidden_rect: Rect {
                    left: -592,
                    top: 120,
                    right: 8,
                    bottom: 700,
                },
                original_topmost: false,
                was_foreground: false,
            },
        );
        controller
    }

    fn collapsed_controller_on_removed_left_monitor() -> EdgeHideController {
        let mut controller = EdgeHideController::new();
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: Rect {
                    left: -1920,
                    top: 120,
                    right: -1320,
                    bottom: 700,
                },
                hidden_rect: Rect {
                    left: -2504,
                    top: 120,
                    right: -1904,
                    bottom: 700,
                },
                original_topmost: false,
                was_foreground: false,
            },
        );
        controller
    }

    #[test]
    fn collapse_preview_uses_the_eventual_visible_strip_length() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        let controller = EdgeHideController::new();
        let monitors = [monitor()];
        let target = window(Rect {
            left: 0,
            top: 120,
            right: 640,
            bottom: 620,
        });

        assert_eq!(
            controller.collapse_preview(&config, &monitors, &target, target.rect),
            Some(Rect {
                left: 0,
                top: 120,
                right: 4,
                bottom: 620,
            })
        );
    }

    #[test]
    fn collapse_preview_remains_available_after_edge_hide_state_changes() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;
        let monitors = [monitor()];
        let start = Instant::now();
        let original = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });
        let moved = window(Rect {
            left: 1320,
            top: 120,
            right: 1920,
            bottom: 700,
        });
        let mut controller = EdgeHideController::new();

        let _ = controller.tick(
            start,
            &config,
            Point { x: 900, y: 500 },
            &monitors,
            Some(&original),
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(1),
                &config,
                Point { x: 900, y: 500 },
                &monitors,
                Some(&original),
            ),
            Some(EdgeHideCommand::Collapse {
                handle: original.handle,
                rect: Rect {
                    left: -584,
                    top: 120,
                    right: 16,
                    bottom: 700,
                },
            })
        );
        assert_eq!(
            controller.collapse_preview(&config, &monitors, &moved, moved.rect),
            Some(Rect {
                left: 1916,
                top: 120,
                right: 1920,
                bottom: 700,
            })
        );

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(2),
                &config,
                Point { x: 2, y: 200 },
                &monitors,
                None,
            ),
            Some(EdgeHideCommand::Restore {
                handle: original.handle,
                rect: original.rect,
                topmost: false,
            })
        );
        assert_eq!(
            controller.collapse_preview(&config, &monitors, &moved, moved.rect),
            Some(Rect {
                left: 1916,
                top: 120,
                right: 1920,
                bottom: 700,
            })
        );
    }

    #[test]
    fn collapse_preview_switch_defaults_on_and_hides_when_disabled() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        let controller = EdgeHideController::new();
        let monitors = [monitor()];
        let target = window(Rect {
            left: 300,
            top: 540,
            right: 900,
            bottom: 1040,
        });

        assert!(config.show_preview);
        assert_eq!(
            controller.collapse_preview(&config, &monitors, &target, target.rect),
            Some(Rect {
                left: 300,
                top: 1036,
                right: 900,
                bottom: 1040,
            })
        );

        config.show_preview = false;
        assert_eq!(
            controller.collapse_preview(&config, &monitors, &target, target.rect),
            None
        );
    }

    #[test]
    fn chooses_edge_directions_from_the_window_monitor_profile() {
        let mut config = EdgeHideConfig::default();
        config.monitor_profiles = vec![
            EdgeHideMonitorProfile {
                monitor_id: "display:0:0:1920:1080".to_string(),
                edges: vec![Edge::Top],
            },
            EdgeHideMonitorProfile {
                monitor_id: "display:-1920:0:0:1080".to_string(),
                edges: vec![],
            },
        ];
        let monitors = left_and_primary_monitors();

        assert_eq!(
            detect_window_edge(
                Rect {
                    left: 400,
                    top: 0,
                    right: 1000,
                    bottom: 500,
                },
                &monitors,
                &config,
            )
            .map(|(edge, _)| edge),
            Some(Edge::Top)
        );
        assert!(detect_window_edge(
            Rect {
                left: -1600,
                top: 0,
                right: -1000,
                bottom: 500,
            },
            &monitors,
            &config,
        )
        .is_none());
    }

    #[test]
    fn collapses_after_delay() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 100;

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let window = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });

        assert_eq!(
            controller.tick(
                start,
                &config,
                Point { x: 800, y: 500 },
                &[monitor()],
                Some(&window)
            ),
            None
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(101),
                &config,
                Point { x: 800, y: 500 },
                &[monitor()],
                Some(&window),
            ),
            Some(EdgeHideCommand::Collapse {
                handle: WindowHandle(42),
                rect: Rect {
                    left: -584,
                    top: 120,
                    right: 16,
                    bottom: 700,
                },
            })
        );
    }

    #[test]
    fn restores_when_cursor_reaches_strip() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let window = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });

        let _ = controller.tick(
            start,
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&window),
        );
        let _ = controller.tick(
            start + Duration::from_millis(1),
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&window),
        );

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(2),
                &config,
                Point { x: 2, y: 200 },
                &[monitor()],
                None,
            ),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: window.rect,
                topmost: false,
            })
        );
    }

    #[test]
    fn restores_when_disabled_after_collapse() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let window = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });

        let _ = controller.tick(
            start,
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&window),
        );
        let _ = controller.tick(
            start + Duration::from_millis(1),
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&window),
        );

        config.enabled = false;
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(2),
                &config,
                Point { x: 900, y: 500 },
                &[monitor()],
                None,
            ),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: window.rect,
                topmost: false,
            })
        );
    }

    #[test]
    fn collapses_when_window_is_one_third_outside_screen() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let window = window(Rect {
            left: -220,
            top: 120,
            right: 380,
            bottom: 700,
        });

        let _ = controller.tick(
            start,
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&window),
        );

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(1),
                &config,
                Point { x: 900, y: 500 },
                &[monitor()],
                Some(&window),
            ),
            Some(EdgeHideCommand::Collapse {
                handle: WindowHandle(42),
                rect: Rect {
                    left: -584,
                    top: 120,
                    right: 16,
                    bottom: 700,
                },
            })
        );
    }

    #[test]
    fn restores_outside_window_to_a_fully_visible_rect() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let window = window(Rect {
            left: -220,
            top: 120,
            right: 380,
            bottom: 700,
        });

        let _ = controller.tick(
            start,
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&window),
        );
        let _ = controller.tick(
            start + Duration::from_millis(1),
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&window),
        );

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(2),
                &config,
                Point { x: 2, y: 200 },
                &[monitor()],
                None,
            ),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: Rect {
                    left: 0,
                    top: 120,
                    right: 600,
                    bottom: 700,
                },
                topmost: false,
            })
        );
    }

    #[test]
    fn multiple_windows_can_stay_collapsed_and_restore_independently() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let left_window = window_with_handle(
            WindowHandle(42),
            Rect {
                left: 0,
                top: 120,
                right: 600,
                bottom: 700,
            },
        );
        let right_window = window_with_handle(
            WindowHandle(43),
            Rect {
                left: 1320,
                top: 220,
                right: 1920,
                bottom: 800,
            },
        );

        let _ = controller.tick(
            start,
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&left_window),
        );
        assert!(matches!(
            controller.tick(
                start + Duration::from_millis(1),
                &config,
                Point { x: 900, y: 500 },
                &[monitor()],
                Some(&left_window),
            ),
            Some(EdgeHideCommand::Collapse {
                handle: WindowHandle(42),
                ..
            })
        ));

        let _ = controller.tick(
            start + Duration::from_millis(2),
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&right_window),
        );
        assert!(matches!(
            controller.tick(
                start + Duration::from_millis(3),
                &config,
                Point { x: 900, y: 500 },
                &[monitor()],
                Some(&right_window),
            ),
            Some(EdgeHideCommand::Collapse {
                handle: WindowHandle(43),
                ..
            })
        ));

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(4),
                &config,
                Point { x: 1918, y: 240 },
                &[monitor()],
                None,
            ),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(43),
                rect: right_window.rect,
                topmost: false,
            })
        );

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(5),
                &config,
                Point { x: 2, y: 140 },
                &[monitor()],
                None,
            ),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: left_window.rect,
                topmost: false,
            })
        );
    }

    #[test]
    fn cancels_pending_when_window_moves_back_to_center() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 100;

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let edge_window = window(Rect {
            left: -220,
            top: 120,
            right: 380,
            bottom: 700,
        });
        let centered_window = window(Rect {
            left: 500,
            top: 120,
            right: 1100,
            bottom: 700,
        });

        let _ = controller.tick(
            start,
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&edge_window),
        );

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(101),
                &config,
                Point { x: 900, y: 500 },
                &[monitor()],
                Some(&centered_window),
            ),
            None
        );
    }

    #[test]
    fn moving_expanded_window_to_center_prevents_recollapse() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;
        config.restore_delay_ms = 10;

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let edge_window = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });
        let centered_window = window(Rect {
            left: 500,
            top: 120,
            right: 1100,
            bottom: 700,
        });

        let _ = controller.tick(
            start,
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&edge_window),
        );
        let _ = controller.tick(
            start + Duration::from_millis(1),
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&edge_window),
        );
        let _ = controller.tick(
            start + Duration::from_millis(2),
            &config,
            Point { x: 2, y: 200 },
            &[monitor()],
            None,
        );

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(20),
                &config,
                Point { x: 1300, y: 800 },
                &[monitor()],
                Some(&centered_window),
            ),
            None
        );
    }

    #[test]
    fn manually_dragged_out_collapsed_window_can_collapse_again() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let edge_window = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });
        let centered_window = window(Rect {
            left: 500,
            top: 120,
            right: 1100,
            bottom: 700,
        });

        controller.tick(
            start,
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&edge_window),
        );
        assert!(matches!(
            controller.tick(
                start + Duration::from_millis(1),
                &config,
                Point { x: 900, y: 500 },
                &[monitor()],
                Some(&edge_window),
            ),
            Some(EdgeHideCommand::Collapse { .. })
        ));

        // The user grabs the visible strip and drags the actual window to the center.
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(2),
                &config,
                Point { x: 700, y: 140 },
                &[monitor()],
                Some(&centered_window),
            ),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: centered_window.rect,
                topmost: false,
            })
        );

        controller.tick(
            start + Duration::from_millis(3),
            &config,
            Point { x: 300, y: 140 },
            &[monitor()],
            Some(&edge_window),
        );
        assert!(matches!(
            controller.tick(
                start + Duration::from_millis(4),
                &config,
                Point { x: 300, y: 140 },
                &[monitor()],
                Some(&edge_window),
            ),
            Some(EdgeHideCommand::Collapse {
                handle: WindowHandle(42),
                ..
            })
        ));
    }

    #[test]
    fn far_left_window_uses_left_monitor_instead_of_primary_fallback() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let window = window(Rect {
            left: -2300,
            top: 120,
            right: -1700,
            bottom: 700,
        });
        let monitors = left_and_primary_monitors();

        controller.tick(
            start,
            &config,
            Point { x: -1800, y: 500 },
            &monitors,
            Some(&window),
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(1),
                &config,
                Point { x: -1800, y: 500 },
                &monitors,
                Some(&window),
            ),
            Some(EdgeHideCommand::Collapse {
                handle: WindowHandle(42),
                rect: Rect {
                    left: -2504,
                    top: 120,
                    right: -1904,
                    bottom: 700,
                },
            })
        );
    }

    #[test]
    fn does_not_collapse_at_vertical_seam_between_monitors() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;
        let monitors = left_and_primary_monitors();
        let start = Instant::now();

        for (index, rect) in [
            Rect {
                left: 0,
                top: 120,
                right: 600,
                bottom: 700,
            },
            Rect {
                left: -600,
                top: 120,
                right: 0,
                bottom: 700,
            },
        ]
        .into_iter()
        .enumerate()
        {
            let mut controller = EdgeHideController::new();
            let seam_window = window_with_handle(WindowHandle(50 + index as isize), rect);
            assert_eq!(
                controller.tick(
                    start,
                    &config,
                    Point { x: -10, y: 500 },
                    &monitors,
                    Some(&seam_window),
                ),
                None
            );
            assert_eq!(
                controller.tick(
                    start + Duration::from_millis(1),
                    &config,
                    Point { x: -10, y: 500 },
                    &monitors,
                    Some(&seam_window),
                ),
                None
            );
        }
    }

    #[test]
    fn partially_covered_monitor_edge_only_collapses_on_exposed_segment() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;
        let monitors = [
            monitor(),
            Monitor {
                bounds: Rect {
                    left: -1280,
                    top: 300,
                    right: 0,
                    bottom: 1024,
                },
                work_area: Rect {
                    left: -1280,
                    top: 300,
                    right: 0,
                    bottom: 984,
                },
                primary: false,
                device_id: [0; 128],
            },
        ];
        let start = Instant::now();

        let covered = window_with_handle(
            WindowHandle(60),
            Rect {
                left: 0,
                top: 400,
                right: 600,
                bottom: 800,
            },
        );
        let exposed = window_with_handle(
            WindowHandle(61),
            Rect {
                left: 0,
                top: 20,
                right: 600,
                bottom: 250,
            },
        );

        let mut covered_controller = EdgeHideController::new();
        covered_controller.tick(
            start,
            &config,
            Point { x: 300, y: 500 },
            &monitors,
            Some(&covered),
        );
        assert_eq!(
            covered_controller.tick(
                start + Duration::from_millis(1),
                &config,
                Point { x: 300, y: 500 },
                &monitors,
                Some(&covered),
            ),
            None
        );

        let mut exposed_controller = EdgeHideController::new();
        exposed_controller.tick(
            start,
            &config,
            Point { x: 300, y: 100 },
            &monitors,
            Some(&exposed),
        );
        assert!(matches!(
            exposed_controller.tick(
                start + Duration::from_millis(1),
                &config,
                Point { x: 300, y: 100 },
                &monitors,
                Some(&exposed),
            ),
            Some(EdgeHideCommand::Collapse { .. })
        ));
    }

    #[test]
    fn does_not_collapse_at_horizontal_seam_between_monitors() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;
        let monitors = [
            monitor(),
            Monitor {
                bounds: Rect {
                    left: 0,
                    top: -1080,
                    right: 1920,
                    bottom: 0,
                },
                work_area: Rect {
                    left: 0,
                    top: -1080,
                    right: 1920,
                    bottom: 0,
                },
                primary: false,
                device_id: [0; 128],
            },
        ];
        let start = Instant::now();

        for (index, rect) in [
            Rect {
                left: 400,
                top: 0,
                right: 1000,
                bottom: 500,
            },
            Rect {
                left: 400,
                top: -500,
                right: 1000,
                bottom: 0,
            },
        ]
        .into_iter()
        .enumerate()
        {
            let mut controller = EdgeHideController::new();
            let seam_window = window_with_handle(WindowHandle(70 + index as isize), rect);
            controller.tick(
                start,
                &config,
                Point { x: 600, y: -10 },
                &monitors,
                Some(&seam_window),
            );
            assert_eq!(
                controller.tick(
                    start + Duration::from_millis(1),
                    &config,
                    Point { x: 600, y: -10 },
                    &monitors,
                    Some(&seam_window),
                ),
                None
            );
        }
    }

    #[test]
    fn restores_expanded_window_when_disabled() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 0;
        config.restore_delay_ms = 99999; // will not collapse back on its own

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let win = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });

        // tick once to enter Pending, once to Collapse
        controller.tick(
            start,
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&win),
        );
        controller.tick(
            start + Duration::from_millis(1),
            &config,
            Point { x: 900, y: 500 },
            &[monitor()],
            Some(&win),
        );
        // cursor near strip => Expand
        controller.tick(
            start + Duration::from_millis(2),
            &config,
            Point { x: 2, y: 200 },
            &[monitor()],
            None,
        );

        // now disable
        config.enabled = false;
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(3),
                &config,
                Point { x: 900, y: 500 },
                &[monitor()],
                None
            ),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: win.rect,
                topmost: false,
            })
        );
    }

    #[test]
    fn waits_for_left_button_release_before_starting_collapse_delay() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 100;

        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let edge_window = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });

        assert_eq!(
            controller.tick_with_left_button_state(
                start,
                &config,
                Point { x: 20, y: 200 },
                &[monitor()],
                Some(&edge_window),
                true,
            ),
            None
        );
        assert_eq!(
            controller.tick_with_left_button_state(
                start + Duration::from_millis(500),
                &config,
                Point { x: 20, y: 200 },
                &[monitor()],
                Some(&edge_window),
                true,
            ),
            None
        );

        assert_eq!(
            controller.tick_with_left_button_state(
                start + Duration::from_millis(501),
                &config,
                Point { x: 20, y: 200 },
                &[monitor()],
                Some(&edge_window),
                false,
            ),
            None
        );
        assert!(matches!(
            controller.tick_with_left_button_state(
                start + Duration::from_millis(602),
                &config,
                Point { x: 20, y: 200 },
                &[monitor()],
                Some(&edge_window),
                false,
            ),
            Some(EdgeHideCommand::Collapse { .. })
        ));
    }

    #[test]
    fn distance_and_ratio_triggers_can_be_controlled_independently() {
        let monitors = [monitor()];
        let near_edge = Rect {
            left: 12,
            top: 120,
            right: 612,
            bottom: 700,
        };
        let partly_outside = Rect {
            left: -180,
            top: 120,
            right: 420,
            bottom: 700,
        };
        let mut config = EdgeHideConfig::default();
        config.distance_trigger_enabled = true;
        config.ratio_trigger_enabled = false;

        assert_eq!(
            detect_window_edge(near_edge, &monitors, &config).map(|(edge, _)| edge),
            Some(Edge::Left)
        );
        assert!(detect_window_edge(partly_outside, &monitors, &config).is_none());

        config.distance_trigger_enabled = false;
        config.ratio_trigger_enabled = true;
        config.trigger_ratio = 30;

        assert!(detect_window_edge(near_edge, &monitors, &config).is_none());
        assert_eq!(
            detect_window_edge(partly_outside, &monitors, &config).map(|(edge, _)| edge),
            Some(Edge::Left)
        );
    }

    #[test]
    fn transient_owned_windows_are_never_edge_hide_candidates() {
        let config = EdgeHideConfig::default();
        let mut popup = window(Rect {
            left: 0,
            top: 700,
            right: 360,
            bottom: 1000,
        });
        popup.transient = true;

        assert!(!is_candidate_window(&popup, &config));
    }

    #[test]
    fn windows_shell_flyouts_are_never_edge_hide_candidates() {
        let config = EdgeHideConfig::default();
        let mut notification_center = window(Rect {
            left: 1500,
            top: 0,
            right: 1920,
            bottom: 1000,
        });
        notification_center.process_name = "explorer.exe".to_string();
        notification_center.class_name = "XamlExplorerHostIslandWindow_WASDK".to_string();
        notification_center.title.clear();

        assert!(!is_candidate_window(&notification_center, &config));

        notification_center.process_name = "ShellExperienceHost.exe".to_string();
        notification_center.class_name = "Windows.UI.Core.CoreWindow".to_string();
        assert!(!is_candidate_window(&notification_center, &config));

        notification_center.process_name = "explorer.exe".to_string();
        notification_center.class_name = "ApplicationFrameWindow".to_string();
        notification_center.title.clear();
        assert!(!is_candidate_window(&notification_center, &config));

        notification_center.class_name = "CabinetWClass".to_string();
        notification_center.title = "Downloads".to_string();
        assert!(is_candidate_window(&notification_center, &config));
    }

    #[test]
    fn pruning_destroyed_windows_removes_their_strip_hints() {
        let mut controller = EdgeHideController::new();
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: Rect {
                    left: 0,
                    top: 120,
                    right: 600,
                    bottom: 700,
                },
                hidden_rect: Rect {
                    left: -592,
                    top: 120,
                    right: 8,
                    bottom: 700,
                },
                original_topmost: false,
                was_foreground: false,
            },
        );
        assert_eq!(
            controller
                .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                    EdgeHideLiveState::Visible(hidden)
                })
                .len(),
            1
        );

        controller.prune_invalid_windows(|_| false);

        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                EdgeHideLiveState::Visible(hidden)
            })
            .is_empty());
    }

    #[test]
    fn moved_collapsed_windows_no_longer_render_stale_strip_hints() {
        let mut controller = EdgeHideController::new();
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: Rect {
                    left: 0,
                    top: 120,
                    right: 600,
                    bottom: 700,
                },
                hidden_rect: Rect {
                    left: -592,
                    top: 120,
                    right: 8,
                    bottom: 700,
                },
                original_topmost: false,
                was_foreground: false,
            },
        );
        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, _| {
                EdgeHideLiveState::Visible(Rect {
                    left: 240,
                    top: 180,
                    right: 840,
                    bottom: 780,
                })
            })
            .is_empty());
    }

    #[test]
    fn collapsed_windows_do_not_render_strips_without_a_current_monitor() {
        let mut controller = EdgeHideController::new();
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: Rect {
                    left: 0,
                    top: 120,
                    right: 600,
                    bottom: 700,
                },
                hidden_rect: Rect {
                    left: -592,
                    top: 120,
                    right: 8,
                    bottom: 700,
                },
                original_topmost: false,
                was_foreground: false,
            },
        );

        assert!(controller
            .collapsed_strips_with_live_state(&[], |_, hidden| {
                EdgeHideLiveState::Visible(hidden)
            })
            .is_empty());
    }

    #[test]
    fn collapsed_windows_do_not_render_strips_for_a_removed_monitor() {
        let mut controller = EdgeHideController::new();
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: Rect {
                    left: -1920,
                    top: 120,
                    right: -1320,
                    bottom: 700,
                },
                hidden_rect: Rect {
                    left: -2504,
                    top: 120,
                    right: -1904,
                    bottom: 700,
                },
                original_topmost: false,
                was_foreground: false,
            },
        );

        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                EdgeHideLiveState::Visible(hidden)
            })
            .is_empty());
    }

    #[test]
    fn adding_an_adjacent_monitor_relocates_before_showing_the_new_outer_edge_hint() {
        let mut controller = collapsed_controller_on_left_monitor();
        let config = EdgeHideConfig {
            enabled: true,
            ..EdgeHideConfig::default()
        };
        let adjacent_monitors = [
            monitor(),
            Monitor {
                bounds: Rect {
                    left: -1920,
                    top: 0,
                    right: 0,
                    bottom: 1080,
                },
                work_area: Rect {
                    left: -1920,
                    top: 0,
                    right: 0,
                    bottom: 1040,
                },
                primary: false,
                device_id: [1; 128],
            },
        ];
        let hidden = Rect {
            left: -592,
            top: 120,
            right: 8,
            bottom: 700,
        };

        assert_eq!(
            controller.tick_with_live_state(
                Instant::now(),
                &config,
                Point { x: 1000, y: 900 },
                &adjacent_monitors,
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(hidden),
            ),
            Some(EdgeHideCommand::Collapse {
                handle: WindowHandle(42),
                rect: Rect {
                    left: -2504,
                    top: 120,
                    right: -1904,
                    bottom: 700,
                },
            })
        );
        assert!(controller
            .collapsed_strips_with_live_state(&adjacent_monitors, |_, _| {
                EdgeHideLiveState::Visible(hidden)
            })
            .is_empty());
    }

    #[test]
    fn hidden_collapsed_windows_do_not_render_stale_strip_hints() {
        let mut controller = EdgeHideController::new();
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: Rect {
                    left: 0,
                    top: 120,
                    right: 600,
                    bottom: 700,
                },
                hidden_rect: Rect {
                    left: -592,
                    top: 120,
                    right: 8,
                    bottom: 700,
                },
                original_topmost: false,
                was_foreground: false,
            },
        );

        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, _| {
                EdgeHideLiveState::Unavailable
            })
            .is_empty());
    }

    #[test]
    fn minimized_collapsed_windows_do_not_render_restore_strips() {
        let mut controller = EdgeHideController::new();
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: Rect {
                    left: 0,
                    top: 120,
                    right: 600,
                    bottom: 700,
                },
                hidden_rect: Rect {
                    left: -592,
                    top: 120,
                    right: 8,
                    bottom: 700,
                },
                original_topmost: false,
                was_foreground: false,
            },
        );

        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, _| EdgeHideLiveState::Minimized)
            .is_empty());
    }

    #[test]
    fn inactive_collapsed_windows_have_no_invisible_restore_hotzone() {
        let config = EdgeHideConfig {
            enabled: true,
            strip_size: 8,
            ..EdgeHideConfig::default()
        };
        for live_state in [
            EdgeHideLiveState::Unavailable,
            EdgeHideLiveState::Minimized,
            EdgeHideLiveState::Visible(Rect {
                left: 240,
                top: 180,
                right: 840,
                bottom: 780,
            }),
        ] {
            let mut controller = collapsed_controller_on_left_monitor();
            assert_eq!(
                controller.tick_with_live_state(
                    Instant::now(),
                    &config,
                    Point { x: 1, y: 300 },
                    &[monitor()],
                    None,
                    EdgeHideInput::default(),
                    |_, _| live_state,
                ),
                None
            );
            assert!(matches!(
                controller.states.get(&WindowHandle(42)),
                Some(EdgeHideState::Collapsed { .. })
            ));
        }
    }

    #[test]
    fn transient_live_query_failure_does_not_fake_a_foreground_activation() {
        let config = EdgeHideConfig {
            enabled: true,
            strip_size: 8,
            ..EdgeHideConfig::default()
        };
        let hidden_rect = Rect {
            left: -592,
            top: 120,
            right: 8,
            bottom: 700,
        };
        let foreground = window(hidden_rect);
        let mut controller = collapsed_controller_on_left_monitor();

        assert_eq!(
            controller.tick_with_live_state(
                Instant::now(),
                &config,
                Point { x: 1000, y: 900 },
                &[monitor()],
                Some(&foreground),
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Unavailable,
            ),
            None
        );
        assert_eq!(
            controller.tick_with_live_state(
                Instant::now(),
                &config,
                Point { x: 1000, y: 900 },
                &[monitor()],
                Some(&foreground),
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(hidden_rect),
            ),
            None
        );
    }

    #[test]
    fn monitor_reconciliation_moves_the_window_before_reenabling_its_strip() {
        let config = EdgeHideConfig {
            enabled: true,
            ..EdgeHideConfig::default()
        };
        let old_hidden_rect = Rect {
            left: -2504,
            top: 120,
            right: -1904,
            bottom: 700,
        };
        let mut controller = collapsed_controller_on_removed_left_monitor();

        let relocation = controller
            .tick_with_live_state(
                Instant::now(),
                &config,
                Point { x: 1, y: 300 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(old_hidden_rect),
            )
            .expect("the collapsed window should move to the current monitor");
        let EdgeHideCommand::Collapse {
            rect: relocated_hidden,
            ..
        } = relocation
        else {
            panic!("monitor reconciliation should issue a collapse relocation")
        };
        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, _| {
                EdgeHideLiveState::Visible(old_hidden_rect)
            })
            .is_empty());

        controller.command_succeeded(
            relocation,
            EdgeHideLiveState::Visible(relocated_hidden),
            Instant::now(),
        );

        assert_eq!(
            controller
                .collapsed_strips_with_live_state(&[monitor()], |_, _| {
                    EdgeHideLiveState::Visible(relocated_hidden)
                })
                .len(),
            1
        );
        assert!(matches!(
            controller.tick_with_live_state(
                Instant::now(),
                &config,
                Point { x: 1, y: 300 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(relocated_hidden),
            ),
            Some(EdgeHideCommand::Restore { .. })
        ));
    }

    #[test]
    fn monitor_reconciliation_adopts_a_window_already_moved_by_windows() {
        let config = EdgeHideConfig {
            enabled: true,
            ..EdgeHideConfig::default()
        };
        let relocated_hidden = Rect {
            left: -584,
            top: 120,
            right: 16,
            bottom: 700,
        };
        let mut controller = collapsed_controller_on_removed_left_monitor();

        assert_eq!(
            controller.tick_with_live_state(
                Instant::now(),
                &config,
                Point { x: 1000, y: 900 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(relocated_hidden),
            ),
            None
        );
        assert!(matches!(
            controller.states.get(&WindowHandle(42)),
            Some(EdgeHideState::Collapsed {
                restore_rect,
                hidden_rect,
                ..
            }) if *restore_rect == Rect {
                left: 0,
                top: 120,
                right: 600,
                bottom: 700,
            } && *hidden_rect == relocated_hidden
        ));
        assert_eq!(
            controller
                .collapsed_strips_with_live_state(&[monitor()], |_, _| {
                    EdgeHideLiveState::Visible(relocated_hidden)
                })
                .len(),
            1
        );
    }

    #[test]
    fn relocation_success_is_not_committed_without_matching_live_geometry() {
        let config = EdgeHideConfig {
            enabled: true,
            ..EdgeHideConfig::default()
        };
        let old_hidden_rect = Rect {
            left: -2504,
            top: 120,
            right: -1904,
            bottom: 700,
        };
        let actual_rect = Rect {
            left: 200,
            top: 160,
            right: 800,
            bottom: 740,
        };
        let mut controller = collapsed_controller_on_removed_left_monitor();
        let relocation = controller
            .tick_with_live_state(
                Instant::now(),
                &config,
                Point { x: 1000, y: 900 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(old_hidden_rect),
            )
            .expect("relocation command");

        let cleanup = controller
            .command_succeeded(
                relocation,
                EdgeHideLiveState::Visible(actual_rect),
                Instant::now(),
            )
            .expect("unexpected relocation geometry should be cleaned up");
        assert_eq!(
            cleanup,
            EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: actual_rect,
                topmost: false,
            }
        );
        assert!(matches!(
            controller.states.get(&WindowHandle(42)),
            Some(EdgeHideState::CleaningUp { .. })
        ));
        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, _| {
                EdgeHideLiveState::Visible(actual_rect)
            })
            .is_empty());

        controller.command_succeeded(
            cleanup,
            EdgeHideLiveState::Visible(actual_rect),
            Instant::now(),
        );

        assert!(!controller.states.contains_key(&WindowHandle(42)));
    }

    #[test]
    fn failed_monitor_relocation_rolls_back_and_can_retry() {
        let config = EdgeHideConfig {
            enabled: true,
            ..EdgeHideConfig::default()
        };
        let old_hidden_rect = Rect {
            left: -2504,
            top: 120,
            right: -1904,
            bottom: 700,
        };
        let mut controller = collapsed_controller_on_removed_left_monitor();
        let relocation = controller
            .tick_with_live_state(
                Instant::now(),
                &config,
                Point { x: 1, y: 300 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(old_hidden_rect),
            )
            .expect("first relocation attempt");

        let failed_at = Instant::now();
        controller.command_failed(relocation, failed_at);

        assert!(matches!(
            controller.states.get(&WindowHandle(42)),
            Some(EdgeHideState::Collapsed { hidden_rect, .. }) if *hidden_rect == old_hidden_rect
        ));
        assert!(matches!(
            controller.tick_with_live_state(
                failed_at + RELOCATION_RETRY_DELAY,
                &config,
                Point { x: 1, y: 300 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(old_hidden_rect),
            ),
            Some(EdgeHideCommand::Collapse { .. })
        ));
    }

    #[test]
    fn failed_monitor_relocation_does_not_block_other_restore_hotzones() {
        let config = EdgeHideConfig {
            enabled: true,
            ..EdgeHideConfig::default()
        };
        let old_hidden_rect = Rect {
            left: -2504,
            top: 120,
            right: -1904,
            bottom: 700,
        };
        let other_hidden_rect = Rect {
            left: -584,
            top: 720,
            right: 16,
            bottom: 1000,
        };
        let mut controller = collapsed_controller_on_removed_left_monitor();
        controller.states.insert(
            WindowHandle(43),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: Rect {
                    left: 0,
                    top: 720,
                    right: 600,
                    bottom: 1000,
                },
                hidden_rect: other_hidden_rect,
                original_topmost: false,
                was_foreground: false,
            },
        );
        let started_at = Instant::now();
        let relocation = controller
            .tick_with_live_state(
                started_at,
                &config,
                Point { x: 1000, y: 900 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |handle, _| {
                    EdgeHideLiveState::Visible(if handle == WindowHandle(42) {
                        old_hidden_rect
                    } else {
                        other_hidden_rect
                    })
                },
            )
            .expect("first window relocation");
        controller.command_failed(relocation, started_at);

        assert_eq!(
            controller.tick_with_live_state(
                started_at + Duration::from_millis(1),
                &config,
                Point { x: 1, y: 800 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |handle, _| {
                    EdgeHideLiveState::Visible(if handle == WindowHandle(42) {
                        old_hidden_rect
                    } else {
                        other_hidden_rect
                    })
                },
            ),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(43),
                rect: Rect {
                    left: 0,
                    top: 720,
                    right: 600,
                    bottom: 1000,
                },
                topmost: false,
            })
        );
    }

    #[test]
    fn transient_popup_interrupts_the_parent_window_collapse_delay() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 100;
        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let parent = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });
        let mut popup = window_with_handle(
            WindowHandle(43),
            Rect {
                left: 0,
                top: 700,
                right: 360,
                bottom: 1000,
            },
        );
        popup.transient = true;

        assert_eq!(
            controller.tick(
                start,
                &config,
                Point { x: 20, y: 200 },
                &[monitor()],
                Some(&parent),
            ),
            None
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(50),
                &config,
                Point { x: 20, y: 800 },
                &[monitor()],
                Some(&popup),
            ),
            None
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(200),
                &config,
                Point { x: 20, y: 200 },
                &[monitor()],
                Some(&parent),
            ),
            None
        );
    }

    #[test]
    fn windows_snap_arranged_windows_are_never_edge_hide_candidates() {
        let config = EdgeHideConfig::default();
        let mut snapped = window(Rect {
            left: 0,
            top: 0,
            right: 960,
            bottom: 1040,
        });
        snapped.arranged = true;

        assert!(!is_candidate_window(&snapped, &config));
    }

    #[test]
    fn in_window_context_menu_suspends_collapse_until_dismissed() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.collapse_delay_ms = 100;
        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let edge_window = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });

        assert_eq!(
            controller.tick_with_input(
                start,
                &config,
                Point { x: 20, y: 200 },
                &[monitor()],
                Some(&edge_window),
                EdgeHideInput {
                    right_button_pressed: true,
                    ..Default::default()
                },
            ),
            None
        );
        assert_eq!(
            controller.tick_with_input(
                start + Duration::from_secs(5),
                &config,
                Point { x: 20, y: 200 },
                &[monitor()],
                Some(&edge_window),
                EdgeHideInput::default(),
            ),
            None
        );
        assert_eq!(
            controller.tick_with_input(
                start + Duration::from_secs(5) + Duration::from_millis(1),
                &config,
                Point { x: 20, y: 200 },
                &[monitor()],
                Some(&edge_window),
                EdgeHideInput {
                    context_menu_dismissed: true,
                    ..Default::default()
                },
            ),
            None
        );
        assert!(matches!(
            controller.tick_with_input(
                start + Duration::from_secs(5) + Duration::from_millis(102),
                &config,
                Point { x: 20, y: 200 },
                &[monitor()],
                Some(&edge_window),
                EdgeHideInput::default(),
            ),
            Some(EdgeHideCommand::Collapse { .. })
        ));
    }

    #[test]
    fn expanded_window_moved_to_another_edge_collapses_at_the_new_edge() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.edges = vec![Edge::Right, Edge::Bottom];
        config.collapse_delay_ms = 0;
        config.restore_delay_ms = 0;
        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let right_window = window(Rect {
            left: 1320,
            top: 120,
            right: 1920,
            bottom: 700,
        });

        controller.tick(
            start,
            &config,
            Point { x: 1500, y: 500 },
            &[monitor()],
            Some(&right_window),
        );
        controller.tick(
            start + Duration::from_millis(1),
            &config,
            Point { x: 1500, y: 500 },
            &[monitor()],
            Some(&right_window),
        );
        controller.tick(
            start + Duration::from_millis(2),
            &config,
            Point { x: 1910, y: 200 },
            &[monitor()],
            None,
        );

        let bottom_window = window(Rect {
            left: 600,
            top: 460,
            right: 1200,
            bottom: 1040,
        });
        assert_eq!(
            controller.tick_with_left_button_state(
                start + Duration::from_millis(3),
                &config,
                Point { x: 800, y: 1000 },
                &[monitor()],
                Some(&bottom_window),
                true,
            ),
            None
        );
        assert_eq!(
            controller.tick_with_left_button_state(
                start + Duration::from_millis(4),
                &config,
                Point { x: 800, y: 1000 },
                &[monitor()],
                Some(&bottom_window),
                false,
            ),
            None
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(5),
                &config,
                Point { x: 800, y: 1000 },
                &[monitor()],
                Some(&bottom_window),
            ),
            Some(EdgeHideCommand::Collapse {
                handle: WindowHandle(42),
                rect: Rect {
                    left: 600,
                    top: 1024,
                    right: 1200,
                    bottom: 1604,
                },
            })
        );
    }

    #[test]
    fn foreground_activation_restores_and_only_rearms_after_focus_is_lost() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.edges = vec![Edge::Right];
        config.collapse_delay_ms = 0;
        config.restore_delay_ms = 50;
        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let visible = window(Rect {
            left: 1320,
            top: 120,
            right: 1920,
            bottom: 700,
        });
        controller.tick(
            start,
            &config,
            Point { x: 1500, y: 500 },
            &[monitor()],
            Some(&visible),
        );
        let collapse = controller.tick(
            start + Duration::from_millis(1),
            &config,
            Point { x: 1500, y: 500 },
            &[monitor()],
            Some(&visible),
        );
        let Some(EdgeHideCommand::Collapse { rect: hidden, .. }) = collapse else {
            panic!("window should collapse first");
        };

        let other = window_with_handle(
            WindowHandle(99),
            Rect {
                left: 200,
                top: 200,
                right: 800,
                bottom: 700,
            },
        );
        controller.tick(
            start + Duration::from_millis(2),
            &config,
            Point { x: 1000, y: 1000 },
            &[monitor()],
            Some(&other),
        );
        let collapsed_foreground = window(hidden);
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(3),
                &config,
                Point { x: 1000, y: 1060 },
                &[monitor()],
                Some(&collapsed_foreground),
            ),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: visible.rect,
                topmost: false,
            })
        );

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(100),
                &config,
                Point { x: 1000, y: 1060 },
                &[monitor()],
                Some(&visible),
            ),
            None
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(101),
                &config,
                Point { x: 1500, y: 500 },
                &[monitor()],
                Some(&visible),
            ),
            None
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(102),
                &config,
                Point { x: 1000, y: 1000 },
                &[monitor()],
                Some(&visible),
            ),
            None
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(153),
                &config,
                Point { x: 1000, y: 1000 },
                &[monitor()],
                Some(&visible),
            ),
            None
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(154),
                &config,
                Point { x: 1000, y: 1000 },
                &[monitor()],
                None,
            ),
            None
        );
        assert!(matches!(
            controller.tick(
                start + Duration::from_millis(205),
                &config,
                Point { x: 1000, y: 1000 },
                &[monitor()],
                None,
            ),
            Some(EdgeHideCommand::Collapse { rect, .. }) if rect == hidden
        ));
    }

    #[test]
    fn minimized_window_focus_fallback_does_not_restore_a_collapsed_window() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.edges = vec![Edge::Right];
        config.collapse_delay_ms = 0;
        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let visible = window(Rect {
            left: 1320,
            top: 120,
            right: 1920,
            bottom: 700,
        });

        controller.tick(
            start,
            &config,
            Point { x: 1500, y: 500 },
            &[monitor()],
            Some(&visible),
        );
        let Some(EdgeHideCommand::Collapse { rect: hidden, .. }) = controller.tick(
            start + Duration::from_millis(1),
            &config,
            Point { x: 1500, y: 500 },
            &[monitor()],
            Some(&visible),
        ) else {
            panic!("window should collapse first");
        };

        let other = window_with_handle(
            WindowHandle(99),
            Rect {
                left: 200,
                top: 200,
                right: 800,
                bottom: 700,
            },
        );
        controller.tick(
            start + Duration::from_millis(2),
            &config,
            Point { x: 1000, y: 1000 },
            &[monitor()],
            Some(&other),
        );

        let collapsed_foreground = window(hidden);
        assert_eq!(
            controller.tick_with_input(
                start + Duration::from_millis(3),
                &config,
                Point { x: 1000, y: 1000 },
                &[monitor()],
                Some(&collapsed_foreground),
                EdgeHideInput {
                    suppress_foreground_restore: true,
                    ..Default::default()
                },
            ),
            None
        );
        assert_eq!(
            controller
                .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                    EdgeHideLiveState::Visible(hidden)
                })
                .len(),
            1
        );

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(4),
                &config,
                Point { x: 1000, y: 1000 },
                &[monitor()],
                Some(&collapsed_foreground),
            ),
            None
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(5),
                &config,
                Point { x: 1918, y: 200 },
                &[monitor()],
                Some(&collapsed_foreground),
            ),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: visible.rect,
                topmost: false,
            })
        );
    }

    #[test]
    fn restoring_a_collapsed_window_preserves_its_original_topmost_state() {
        let mut config = EdgeHideConfig::default();
        config.enabled = true;
        config.edges = vec![Edge::Right];
        config.collapse_delay_ms = 0;
        let mut controller = EdgeHideController::new();
        let start = Instant::now();
        let mut visible = window(Rect {
            left: 1320,
            top: 120,
            right: 1920,
            bottom: 700,
        });
        visible.topmost = true;

        controller.tick(
            start,
            &config,
            Point { x: 1500, y: 500 },
            &[monitor()],
            Some(&visible),
        );
        assert!(matches!(
            controller.tick(
                start + Duration::from_millis(1),
                &config,
                Point { x: 1500, y: 500 },
                &[monitor()],
                Some(&visible),
            ),
            Some(EdgeHideCommand::Collapse { .. })
        ));
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(2),
                &config,
                Point { x: 1910, y: 200 },
                &[monitor()],
                None,
            ),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: visible.rect,
                topmost: true,
            })
        );
    }

    #[test]
    fn failed_collapse_keeps_cleanup_state_until_restore_succeeds() {
        let config = EdgeHideConfig {
            enabled: true,
            collapse_delay_ms: 0,
            ..EdgeHideConfig::default()
        };
        let mut controller = EdgeHideController::new();
        let visible = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });
        let start = Instant::now();

        assert_eq!(
            controller.tick(
                start,
                &config,
                Point { x: 300, y: 300 },
                &[monitor()],
                Some(&visible),
            ),
            None
        );
        let command = controller
            .tick(
                start + Duration::from_millis(1),
                &config,
                Point { x: 300, y: 300 },
                &[monitor()],
                Some(&visible),
            )
            .expect("collapse command");
        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                EdgeHideLiveState::Visible(hidden)
            })
            .is_empty());

        let failed_at = start + Duration::from_millis(2);
        controller.command_failed(command, failed_at);

        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                EdgeHideLiveState::Visible(hidden)
            })
            .is_empty());
        assert!(matches!(
            controller.states.get(&visible.handle),
            Some(EdgeHideState::CleaningUp { .. })
        ));
        assert_eq!(
            controller.tick_with_live_state(
                failed_at + RELOCATION_RETRY_DELAY - Duration::from_millis(1),
                &config,
                Point { x: 300, y: 300 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(visible.rect),
            ),
            None
        );
        let cleanup = controller
            .tick_with_live_state(
                failed_at + RELOCATION_RETRY_DELAY,
                &config,
                Point { x: 300, y: 300 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(visible.rect),
            )
            .expect("cleanup restore should retry");
        assert_eq!(
            cleanup,
            EdgeHideCommand::Restore {
                handle: visible.handle,
                rect: visible.rect,
                topmost: false,
            }
        );

        controller.command_succeeded(
            cleanup,
            EdgeHideLiveState::Visible(visible.rect),
            failed_at + RELOCATION_RETRY_DELAY,
        );

        assert!(!controller.states.contains_key(&visible.handle));
    }

    #[test]
    fn successful_collapse_is_cleaned_up_when_live_geometry_did_not_move() {
        let config = EdgeHideConfig {
            enabled: true,
            collapse_delay_ms: 0,
            ..EdgeHideConfig::default()
        };
        let mut controller = EdgeHideController::new();
        let visible = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });
        let start = Instant::now();
        controller.tick(
            start,
            &config,
            Point { x: 300, y: 300 },
            &[monitor()],
            Some(&visible),
        );
        let command = controller
            .tick(
                start + Duration::from_millis(1),
                &config,
                Point { x: 300, y: 300 },
                &[monitor()],
                Some(&visible),
            )
            .expect("collapse command");

        let cleanup = controller
            .command_succeeded(
                command,
                EdgeHideLiveState::Visible(visible.rect),
                start + Duration::from_millis(1),
            )
            .expect("mismatched geometry should be restored");
        assert_eq!(
            cleanup,
            EdgeHideCommand::Restore {
                handle: visible.handle,
                rect: visible.rect,
                topmost: false,
            }
        );
        assert!(matches!(
            controller.states.get(&visible.handle),
            Some(EdgeHideState::CleaningUp { .. })
        ));
        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                EdgeHideLiveState::Visible(hidden)
            })
            .is_empty());

        let failed_at = start + Duration::from_millis(2);
        controller.command_failed(cleanup, failed_at);
        let retry = controller
            .tick_with_live_state(
                failed_at + RELOCATION_RETRY_DELAY,
                &config,
                Point { x: 300, y: 300 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(visible.rect),
            )
            .expect("failed cleanup should retry");
        assert_eq!(retry, cleanup);
        controller.command_succeeded(
            retry,
            EdgeHideLiveState::Visible(visible.rect),
            failed_at + RELOCATION_RETRY_DELAY,
        );

        assert!(!controller.states.contains_key(&visible.handle));
    }

    #[test]
    fn unavailable_collapse_never_exposes_hint_and_cleans_up_after_query_recovers() {
        let config = EdgeHideConfig {
            enabled: true,
            collapse_delay_ms: 0,
            ..EdgeHideConfig::default()
        };
        let mut controller = EdgeHideController::new();
        let visible = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });
        let start = Instant::now();
        controller.tick(
            start,
            &config,
            Point { x: 300, y: 300 },
            &[monitor()],
            Some(&visible),
        );
        let command = controller
            .tick(
                start + Duration::from_millis(1),
                &config,
                Point { x: 300, y: 300 },
                &[monitor()],
                Some(&visible),
            )
            .expect("collapse command");

        assert_eq!(
            controller.command_succeeded(
                command,
                EdgeHideLiveState::Unavailable,
                start + Duration::from_millis(1),
            ),
            None
        );
        assert!(matches!(
            controller.states.get(&visible.handle),
            Some(EdgeHideState::Collapsing { .. })
        ));
        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                EdgeHideLiveState::Visible(hidden)
            })
            .is_empty());

        let cleanup = controller
            .tick_with_live_state(
                start + Duration::from_millis(2),
                &config,
                Point { x: 300, y: 300 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(visible.rect),
            )
            .expect("recovered query should clean up the mismatched window");
        assert_eq!(
            cleanup,
            EdgeHideCommand::Restore {
                handle: visible.handle,
                rect: visible.rect,
                topmost: false,
            }
        );
        assert!(matches!(
            controller.states.get(&visible.handle),
            Some(EdgeHideState::CleaningUp { .. })
        ));
        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                EdgeHideLiveState::Visible(hidden)
            })
            .is_empty());

        controller.command_succeeded(
            cleanup,
            EdgeHideLiveState::Visible(visible.rect),
            start + Duration::from_millis(2),
        );

        assert!(!controller.states.contains_key(&visible.handle));
    }

    #[test]
    fn failed_restore_returns_to_the_collapsed_state() {
        let config = EdgeHideConfig {
            enabled: true,
            collapse_delay_ms: 0,
            ..EdgeHideConfig::default()
        };
        let mut controller = EdgeHideController::new();
        let visible = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });
        let start = Instant::now();
        controller.tick(
            start,
            &config,
            Point { x: 300, y: 300 },
            &[monitor()],
            Some(&visible),
        );
        controller.tick(
            start + Duration::from_millis(1),
            &config,
            Point { x: 300, y: 300 },
            &[monitor()],
            Some(&visible),
        );
        let command = controller
            .tick(
                start + Duration::from_millis(2),
                &config,
                Point { x: 1, y: 300 },
                &[monitor()],
                None,
            )
            .expect("restore command");

        let failed_at = start + Duration::from_millis(3);
        controller.command_failed(command, failed_at);

        assert!(matches!(
            controller.states.get(&visible.handle),
            Some(EdgeHideState::Expanding { .. })
        ));
        assert_eq!(
            controller
                .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                    EdgeHideLiveState::Visible(hidden)
                })
                .len(),
            0
        );
        controller.tick_with_live_state(
            failed_at + RELOCATION_RETRY_DELAY,
            &config,
            Point { x: 500, y: 500 },
            &[monitor()],
            None,
            EdgeHideInput::default(),
            |_, _| {
                EdgeHideLiveState::Visible(Rect {
                    left: -584,
                    top: 120,
                    right: 16,
                    bottom: 700,
                })
            },
        );
        assert_eq!(
            controller
                .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                    EdgeHideLiveState::Visible(hidden)
                })
                .len(),
            1
        );
    }

    #[test]
    fn restore_success_requires_live_geometry_before_expanding() {
        let config = EdgeHideConfig {
            enabled: true,
            ..EdgeHideConfig::default()
        };
        let visible_rect = Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        };
        let hidden_rect = Rect {
            left: -584,
            top: 120,
            right: 16,
            bottom: 700,
        };
        let mut controller = EdgeHideController::new();
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: visible_rect,
                hidden_rect,
                original_topmost: false,
                was_foreground: false,
            },
        );
        let start = Instant::now();
        let command = controller
            .tick_with_live_state(
                start,
                &config,
                Point { x: 1, y: 300 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(hidden_rect),
            )
            .expect("restore command");
        assert_eq!(
            command,
            EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: visible_rect,
                topmost: false,
            }
        );
        assert!(matches!(
            controller.states.get(&WindowHandle(42)),
            Some(EdgeHideState::Expanding { .. })
        ));

        controller.command_succeeded(command, EdgeHideLiveState::Visible(hidden_rect), start);
        assert!(matches!(
            controller.states.get(&WindowHandle(42)),
            Some(EdgeHideState::Collapsed { .. })
        ));
        assert_eq!(
            controller
                .collapsed_strips_with_live_state(&[monitor()], |_, _| {
                    EdgeHideLiveState::Visible(hidden_rect)
                })
                .len(),
            1
        );

        let command = controller
            .tick_with_live_state(
                start + Duration::from_millis(1),
                &config,
                Point { x: 1, y: 300 },
                &[monitor()],
                None,
                EdgeHideInput::default(),
                |_, _| EdgeHideLiveState::Visible(hidden_rect),
            )
            .expect("second restore command");
        controller.command_succeeded(command, EdgeHideLiveState::Unavailable, start);
        assert!(matches!(
            controller.states.get(&WindowHandle(42)),
            Some(EdgeHideState::Expanding { .. })
        ));
        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, _| {
                EdgeHideLiveState::Visible(hidden_rect)
            })
            .is_empty());

        controller.tick_with_live_state(
            start + RELOCATION_RETRY_DELAY,
            &config,
            Point { x: 500, y: 500 },
            &[monitor()],
            None,
            EdgeHideInput::default(),
            |_, _| EdgeHideLiveState::Visible(visible_rect),
        );
        assert!(matches!(
            controller.states.get(&WindowHandle(42)),
            Some(EdgeHideState::Expanded { .. })
        ));
    }

    #[test]
    fn failed_batch_restore_keeps_state_for_a_later_retry() {
        let mut controller = EdgeHideController::new();
        let restore_rect = Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        };
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect,
                hidden_rect: Rect {
                    left: -584,
                    top: 120,
                    right: 16,
                    bottom: 700,
                },
                original_topmost: true,
                was_foreground: false,
            },
        );

        controller.prepare_restore_all(&[monitor()], 16);
        let command = controller.restore_if_needed().expect("batch restore");
        assert_eq!(
            command,
            EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: restore_rect,
                topmost: true,
            }
        );
        assert!(controller.states.is_empty());

        let failed_at = Instant::now();
        controller.command_failed(command, failed_at);
        assert!(matches!(
            controller.states.get(&WindowHandle(42)),
            Some(EdgeHideState::CleaningUp { .. })
        ));
        let retry = controller.restore_if_needed().expect("batch restore retry");
        assert_eq!(retry, command);
        controller.command_succeeded(
            retry,
            EdgeHideLiveState::Visible(restore_rect),
            failed_at + RELOCATION_RETRY_DELAY,
        );
        assert!(controller.states.is_empty());
    }

    #[test]
    fn invalid_window_handles_are_pruned_with_their_hints() {
        let mut controller = EdgeHideController::new();
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: Rect {
                    left: 0,
                    top: 120,
                    right: 600,
                    bottom: 700,
                },
                hidden_rect: Rect {
                    left: -584,
                    top: 120,
                    right: 16,
                    bottom: 700,
                },
                original_topmost: false,
                was_foreground: false,
            },
        );

        controller.prune_invalid_windows(|_| false);

        assert!(controller.states.is_empty());
        assert!(controller
            .collapsed_strips_with_live_state(&[monitor()], |_, hidden| {
                EdgeHideLiveState::Visible(hidden)
            })
            .is_empty());
    }

    #[test]
    fn disabled_restore_reclamps_collapsed_window_to_current_work_area() {
        let mut controller = EdgeHideController::new();
        controller.states.insert(
            WindowHandle(42),
            EdgeHideState::Collapsed {
                edge: Edge::Left,
                restore_rect: Rect {
                    left: -1920,
                    top: 120,
                    right: -1320,
                    bottom: 700,
                },
                hidden_rect: Rect {
                    left: -2504,
                    top: 120,
                    right: -1904,
                    bottom: 700,
                },
                original_topmost: false,
                was_foreground: false,
            },
        );

        controller.prepare_restore_all(&[monitor()], 16);

        assert_eq!(
            controller.restore_if_needed(),
            Some(EdgeHideCommand::Restore {
                handle: WindowHandle(42),
                rect: Rect {
                    left: 0,
                    top: 120,
                    right: 600,
                    bottom: 700,
                },
                topmost: false,
            })
        );
    }

    #[test]
    fn foreground_expanded_window_does_not_recollapse_while_in_use() {
        let config = EdgeHideConfig {
            enabled: true,
            collapse_delay_ms: 0,
            restore_delay_ms: 10,
            ..EdgeHideConfig::default()
        };
        let mut controller = EdgeHideController::new();
        let visible = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });
        let start = Instant::now();
        controller.tick(
            start,
            &config,
            Point { x: 300, y: 300 },
            &[monitor()],
            Some(&visible),
        );
        controller.tick(
            start + Duration::from_millis(1),
            &config,
            Point { x: 300, y: 300 },
            &[monitor()],
            Some(&visible),
        );
        controller.tick(
            start + Duration::from_millis(2),
            &config,
            Point { x: 1, y: 300 },
            &[monitor()],
            None,
        );

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(30),
                &config,
                Point { x: 1200, y: 900 },
                &[monitor()],
                Some(&visible),
            ),
            None
        );
        assert!(matches!(
            controller.states.get(&visible.handle),
            Some(EdgeHideState::Expanded {
                leave_since: None,
                ..
            })
        ));
    }

    #[test]
    fn foreground_expanded_window_recollapses_when_hold_is_disabled() {
        let config = EdgeHideConfig {
            enabled: true,
            keep_expanded_when_foreground: false,
            collapse_delay_ms: 0,
            restore_delay_ms: 10,
            ..EdgeHideConfig::default()
        };
        let mut controller = EdgeHideController::new();
        let visible = window(Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        });
        let start = Instant::now();
        controller.tick(
            start,
            &config,
            Point { x: 300, y: 300 },
            &[monitor()],
            Some(&visible),
        );
        let Some(EdgeHideCommand::Collapse { rect: hidden, .. }) = controller.tick(
            start + Duration::from_millis(1),
            &config,
            Point { x: 300, y: 300 },
            &[monitor()],
            Some(&visible),
        ) else {
            panic!("window should collapse first");
        };
        assert!(matches!(
            controller.tick(
                start + Duration::from_millis(2),
                &config,
                Point { x: 1, y: 300 },
                &[monitor()],
                None,
            ),
            Some(EdgeHideCommand::Restore { .. })
        ));

        assert_eq!(
            controller.tick(
                start + Duration::from_millis(3),
                &config,
                Point { x: 1200, y: 900 },
                &[monitor()],
                Some(&visible),
            ),
            None
        );
        assert_eq!(
            controller.tick(
                start + Duration::from_millis(14),
                &config,
                Point { x: 1200, y: 900 },
                &[monitor()],
                Some(&visible),
            ),
            Some(EdgeHideCommand::Collapse {
                handle: visible.handle,
                rect: hidden,
            })
        );
    }

    #[test]
    fn expanded_window_adopts_an_explicit_topmost_change() {
        let config = EdgeHideConfig {
            enabled: true,
            ..EdgeHideConfig::default()
        };
        let mut controller = EdgeHideController::new();
        let mut visible = window(Rect {
            left: 200,
            top: 120,
            right: 800,
            bottom: 700,
        });
        visible.topmost = true;
        controller.states.insert(
            visible.handle,
            EdgeHideState::Expanded {
                edge: Edge::Left,
                restore_rect: visible.rect,
                hidden_rect: Rect {
                    left: -584,
                    top: 120,
                    right: 16,
                    bottom: 700,
                },
                original_topmost: false,
                pointer_entered: true,
                leave_since: None,
            },
        );

        assert_eq!(
            controller.tick(
                Instant::now(),
                &config,
                Point { x: 400, y: 300 },
                &[monitor()],
                Some(&visible),
            ),
            None
        );
        assert_eq!(
            controller.restore_if_needed(),
            Some(EdgeHideCommand::Restore {
                handle: visible.handle,
                rect: visible.rect,
                topmost: true,
            })
        );
    }
}
