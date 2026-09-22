use super::{Monitor, Point};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Volume,
    Brightness,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub value: f32,
    pub muted: bool,
    pub device_name: String,
}

impl Level {
    pub fn brightness(min: u32, value: u32, max: u32, name: impl Into<String>) -> Result<Self> {
        ensure!(min < max && (min..=max).contains(&value), "无效的亮度读数");
        Ok(Self {
            value: (value - min) as f32 / (max - min) as f32,
            muted: false,
            device_name: name.into(),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feedback {
    pub interaction: u64,
    pub sequence: u64,
    pub kind: Kind,
    pub screen: super::Rect,
    pub level: Option<Level>,
    pub error: Option<String>,
    pub pending: bool,
}

#[derive(Clone, Debug)]
struct Request {
    interaction: u64,
    sequence: u64,
    monitor: Monitor,
    delta: f32,
    received: Instant,
}

#[derive(Default)]
struct Queue {
    pending: VecDeque<Request>,
    started: bool,
}

impl Queue {
    fn push(&mut self, request: Request) {
        // Merge only consecutive inputs for the same display and direction.
        // Reversals must still work when the device is already at a limit.
        if let Some(last) = self.pending.back_mut() {
            if last.interaction == request.interaction
                && last.monitor.id() == request.monitor.id()
                && last.delta.signum() == request.delta.signum()
            {
                last.delta = (last.delta + request.delta).clamp(-1.0, 1.0);
                last.sequence = request.sequence;
                last.received = request.received;
                return;
            }
        }
        self.pending.push_back(request);
        // Bound stale work when a device stops responding.
        while self.pending.len() > 32 {
            self.pending.pop_front();
        }
    }
}

struct Worker {
    queue: Mutex<Queue>,
    wake: Condvar,
}

impl Worker {
    const fn new() -> Self {
        Self {
            queue: Mutex::new(Queue {
                pending: VecDeque::new(),
                started: false,
            }),
            wake: Condvar::new(),
        }
    }

    fn submit(&'static self, kind: Kind, monitor: Monitor, delta: f32) -> Result<()> {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if !queue.started {
            std::thread::Builder::new()
                .name(format!("adjust-{kind:?}"))
                .spawn(move || self.run(kind))?;
            queue.started = true;
        }
        let request = FEEDBACK.lock().unwrap_or_else(|e| e.into_inner()).begin(
            kind,
            monitor,
            delta,
            Instant::now(),
        );
        queue.push(request);
        self.wake.notify_one();
        Ok(())
    }

    fn run(&self, kind: Kind) {
        loop {
            let request = {
                let queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
                let mut queue = self
                    .wake
                    .wait_while(queue, |q| q.pending.is_empty())
                    .unwrap_or_else(|e| e.into_inner());
                queue.pending.pop_front().unwrap()
            };
            // Each backend binds read, write, and readback to one output device.
            let result = if request.received.elapsed() > Duration::from_secs(1) {
                Err(anyhow::anyhow!("设备响应过慢，请重试"))
            } else {
                match kind {
                    Kind::Volume => super::adjust_system_volume(request.delta),
                    Kind::Brightness => {
                        super::adjust_monitor_brightness(&request.monitor, request.delta)
                    }
                }
            };
            FEEDBACK
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .complete(&request, result);
        }
    }
}

static VOLUME: Worker = Worker::new();
static BRIGHTNESS: Worker = Worker::new();
static FEEDBACK: Mutex<FeedbackState> = Mutex::new(FeedbackState::new());

struct Active {
    monitor: Monitor,
    last_input: Instant,
    requested: u64,
    completed: u64,
    feedback: Feedback,
}

struct FeedbackState {
    request_sequence: u64,
    event_sequence: u64,
    active: Option<Active>,
    dirty: bool,
}

impl FeedbackState {
    const fn new() -> Self {
        Self {
            request_sequence: 0,
            event_sequence: 0,
            active: None,
            dirty: false,
        }
    }

    fn begin(&mut self, kind: Kind, monitor: Monitor, delta: f32, now: Instant) -> Request {
        self.request_sequence += 1;
        self.event_sequence += 1;
        let continues = self.active.as_ref().is_some_and(|active| {
            active.feedback.kind == kind
                && active.monitor == monitor
                && now.duration_since(active.last_input) < Duration::from_millis(1_200)
        });
        if !continues {
            self.active = Some(Active {
                monitor,
                last_input: now,
                requested: self.request_sequence,
                completed: 0,
                feedback: Feedback {
                    interaction: self.request_sequence,
                    sequence: self.event_sequence,
                    kind,
                    screen: monitor.bounds,
                    level: None,
                    error: None,
                    pending: true,
                },
            });
        }
        let active = self.active.as_mut().unwrap();
        active.last_input = now;
        active.requested = self.request_sequence;
        active.feedback.sequence = self.event_sequence;
        active.feedback.pending = true;
        active.feedback.error = None;
        self.dirty = true;
        Request {
            interaction: active.feedback.interaction,
            sequence: self.request_sequence,
            monitor,
            delta,
            received: now,
        }
    }

    fn complete(&mut self, request: &Request, result: Result<Level>) {
        let Some(active) = &mut self.active else {
            return;
        };
        // Accept every completed readback in this interaction, even with newer input queued.
        // A switch away and back creates a new interaction and cannot revive old results.
        if request.interaction != active.feedback.interaction
            || request.sequence <= active.completed
        {
            return;
        }
        active.completed = request.sequence;
        self.event_sequence += 1;
        active.feedback.sequence = self.event_sequence;
        active.feedback.pending = request.sequence < active.requested;
        match result {
            Ok(level) => {
                active.feedback.level = Some(level);
                active.feedback.error = None;
            }
            Err(error) => {
                active.feedback.level = None;
                active.feedback.error = Some(format!("{error:#}"));
            }
        }
        self.dirty = true;
    }

    fn take(&mut self) -> Option<Feedback> {
        if !std::mem::take(&mut self.dirty) {
            return None;
        }
        self.active.as_ref().map(|active| active.feedback.clone())
    }
}

pub fn take_adjustment_feedback() -> Option<Feedback> {
    FEEDBACK.lock().unwrap_or_else(|e| e.into_inner()).take()
}

pub fn adjust_at(kind: Kind, delta: f32, point: Option<Point>) -> Result<()> {
    ensure!(
        delta.is_finite() && delta != 0.0,
        "invalid adjustment delta"
    );
    let point = match point {
        Some(point) => point,
        None => super::cursor_position()?,
    };
    let monitor = monitor_at(super::monitors()?, point)?;
    let worker = match kind {
        Kind::Volume => &VOLUME,
        Kind::Brightness => &BRIGHTNESS,
    };
    worker.submit(kind, monitor, delta.clamp(-1.0, 1.0))
}

fn monitor_at(monitors: Vec<Monitor>, point: Point) -> Result<Monitor> {
    let mut matches = monitors.into_iter().filter(|m| m.bounds.contains(point));
    let monitor = matches.next().context("未找到目标显示器")?;
    ensure!(
        matches.next().is_none(),
        "多个显示器覆盖触发位置，无法唯一确定目标"
    );
    Ok(monitor)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(sequence: u64, delta: f32, left: i32) -> Request {
        let bounds = super::super::Rect {
            left,
            top: 0,
            right: left + 100,
            bottom: 100,
        };
        Request {
            interaction: 1,
            sequence,
            delta,
            received: Instant::now(),
            monitor: Monitor {
                bounds,
                work_area: bounds,
                primary: left == 0,
                device_id: [0; 128],
            },
        }
    }

    #[test]
    fn merge_preserves_reversals_and_display_targets() {
        let mut queue = Queue::default();
        queue.push(request(1, 0.02, 0));
        queue.push(request(2, 0.03, 0));
        queue.push(request(3, -0.02, 0));
        queue.push(request(4, -0.02, 100));
        assert_eq!(queue.pending.len(), 3);
        let first = queue.pending.pop_front().unwrap();
        assert_eq!(first.sequence, 2);
        assert!((first.delta - 0.05).abs() < 1e-6);
        assert_eq!(queue.pending.pop_front().unwrap().delta, -0.02);
        assert_eq!(queue.pending.pop_front().unwrap().monitor.bounds.left, 100);
    }

    #[test]
    fn selection_rejects_mirrored_displays_and_gaps() {
        let monitor = request(1, 0.02, -100).monitor;
        assert_eq!(
            monitor_at(vec![monitor], Point { x: -50, y: 10 }).unwrap(),
            monitor
        );
        assert!(monitor_at(vec![monitor], Point { x: 10, y: 10 }).is_err());
        assert!(monitor_at(vec![monitor, monitor], Point { x: -50, y: 10 }).is_err());
    }

    fn level(value: f32) -> Result<Level> {
        Ok(Level {
            value,
            muted: false,
            device_name: "Test output".into(),
        })
    }

    #[test]
    fn readbacks_keep_advancing_while_newer_input_is_queued() {
        let now = Instant::now();
        let monitor = request(1, 0.02, 0).monitor;
        let mut state = FeedbackState::new();
        let mut running = state.begin(Kind::Volume, monitor, 0.02, now);
        let mut revision = state.take().unwrap().sequence;
        for index in 1..20 {
            let next = state.begin(
                Kind::Volume,
                monitor,
                0.02,
                now + Duration::from_millis(index * 40),
            );
            assert_eq!(next.interaction, running.interaction);
            state.complete(&running, level(index as f32 / 100.0));
            // A further input before the engine polls must retain the completed readback.
            let newest = state.begin(
                Kind::Volume,
                monitor,
                0.02,
                now + Duration::from_millis(index * 40 + 1),
            );
            let feedback = state.take().unwrap();
            assert!(feedback.sequence > revision);
            assert!(feedback.pending);
            assert_eq!(feedback.level.unwrap().value, index as f32 / 100.0);
            assert!(state.take().is_none());
            revision = feedback.sequence;
            running = newest;
        }
        state.complete(&running, level(0.4));
        let feedback = state.take().unwrap();
        assert!(!feedback.pending);
        assert_eq!(feedback.level.unwrap().value, 0.4);
        state.complete(&running, level(0.1));
        assert!(state.take().is_none());
    }

    #[test]
    fn changing_target_or_resuming_after_idle_discards_old_readbacks() {
        let now = Instant::now();
        let monitor = request(1, 0.02, 0).monitor;
        let other_monitor = request(1, 0.02, 100).monitor;
        for (kind, target, delay) in [
            (Kind::Brightness, monitor, Duration::ZERO),
            (Kind::Volume, other_monitor, Duration::ZERO),
            (Kind::Volume, monitor, Duration::from_secs(2)),
        ] {
            let mut state = FeedbackState::new();
            let old = state.begin(Kind::Volume, monitor, 0.02, now);
            state.complete(&old, level(0.4));
            state.begin(kind, target, 0.02, now + delay);
            let resumed = state.begin(Kind::Volume, monitor, 0.02, now + delay);
            let pending = state.take().unwrap();
            assert!(pending.level.is_none());
            assert_ne!(pending.interaction, old.interaction);
            state.complete(&old, level(0.5));
            assert!(state.take().is_none());
            state.complete(&resumed, level(0.6));
            assert_eq!(state.take().unwrap().level.unwrap().value, 0.6);
        }
    }

    #[test]
    fn queued_work_does_not_merge_across_interactions() {
        let mut queue = Queue::default();
        queue.push(request(1, 0.02, 0));
        let mut next = request(2, 0.02, 0);
        next.interaction = 2;
        queue.push(next);
        assert_eq!(queue.pending.len(), 2);
    }
}
