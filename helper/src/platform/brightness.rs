use crate::platform::{Monitor, Point};
use anyhow::{ensure, Context, Result};
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

struct WorkerState {
    pending: VecDeque<(Monitor, f32)>,
    error: Option<String>,
    started: bool,
}

impl WorkerState {
    fn enqueue(&mut self, monitor: Monitor, delta: f32) {
        if let Some((_, pending)) = self
            .pending
            .iter_mut()
            .find(|(target, _)| target.id() == monitor.id())
        {
            *pending = (*pending + delta).clamp(-1.0, 1.0);
        } else {
            self.pending.push_back((monitor, delta.clamp(-1.0, 1.0)));
        }
    }
}

static STATE: Mutex<WorkerState> = Mutex::new(WorkerState {
    pending: VecDeque::new(),
    error: None,
    started: false,
});
static WAKE: Condvar = Condvar::new();

pub fn adjust_brightness_at(delta: f32, point: Option<Point>) -> Result<()> {
    ensure!(
        delta.is_finite() && delta != 0.0,
        "invalid brightness delta"
    );
    let point = match point {
        Some(point) => point,
        None => super::cursor_position()?,
    };
    let monitor = monitor_at(super::monitors()?, point)?;

    let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
    if !state.started {
        std::thread::Builder::new()
            .name("brightness".into())
            .spawn(run_worker)?;
        state.started = true;
    }
    state.enqueue(monitor, delta);
    WAKE.notify_one();
    Ok(())
}

fn monitor_at(monitors: Vec<Monitor>, point: Point) -> Result<Monitor> {
    let mut matches = monitors
        .into_iter()
        .filter(|monitor| monitor.bounds.contains(point));
    let monitor = matches.next().context("未找到目标显示器")?;
    ensure!(
        matches.next().is_none(),
        "多个显示器覆盖触发位置，无法唯一确定亮度设备"
    );
    Ok(monitor)
}

pub fn take_brightness_error() -> Option<String> {
    STATE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .error
        .take()
}

fn run_worker() {
    loop {
        let (monitor, delta) = {
            let state = STATE.lock().unwrap_or_else(|error| error.into_inner());
            let mut state = WAKE
                .wait_while(state, |state| state.pending.is_empty())
                .unwrap_or_else(|error| error.into_inner());
            state.pending.pop_front().unwrap()
        };
        if delta == 0.0 {
            continue;
        }
        // Display drivers can block for hundreds of milliseconds. Keep them off the input loop.
        if let Err(error) = super::adjust_monitor_brightness(&monitor, delta) {
            STATE
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .error = Some(format!("亮度调节失败：{error:#}"));
        }
    }
}

pub(crate) fn adjusted_brightness(min: u32, current: u32, max: u32, delta: f32) -> Result<u32> {
    ensure!(
        min < max && (min..=max).contains(&current),
        "invalid brightness range"
    );
    ensure!(
        delta.is_finite() && delta != 0.0,
        "invalid brightness delta"
    );
    let step = (f64::from(max - min) * f64::from(delta)).round();
    let step = if delta > 0.0 {
        step.max(1.0)
    } else {
        step.min(-1.0)
    };
    Ok((f64::from(current) + step).clamp(f64::from(min), f64::from(max)) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Rect;

    #[test]
    fn target_selection_rejects_overlapping_screens_and_gaps() {
        let bounds = Rect {
            left: -100,
            top: 0,
            right: 0,
            bottom: 100,
        };
        let monitor = Monitor {
            bounds,
            work_area: bounds,
            primary: true,
            device_id: [0; 128],
        };
        assert_eq!(
            monitor_at(vec![monitor], Point { x: -50, y: 20 }).unwrap(),
            monitor
        );
        assert!(monitor_at(vec![monitor], Point { x: 0, y: 20 }).is_err());
        assert!(monitor_at(vec![monitor, monitor], Point { x: -50, y: 20 }).is_err());
    }

    #[test]
    fn brightness_steps_use_the_device_range_and_stop_at_its_limits() {
        assert_eq!(adjusted_brightness(0, 50, 100, 0.05).unwrap(), 55);
        assert_eq!(adjusted_brightness(20, 100, 220, -0.05).unwrap(), 90);
        assert_eq!(adjusted_brightness(20, 215, 220, 0.05).unwrap(), 220);
        assert_eq!(adjusted_brightness(20, 21, 220, -0.05).unwrap(), 20);
        assert_eq!(adjusted_brightness(0, 5, 10, 0.001).unwrap(), 6);
        assert!(adjusted_brightness(10, 5, 100, 0.05).is_err());
        assert!(adjusted_brightness(10, 10, 10, 0.05).is_err());
        assert!(adjusted_brightness(0, 50, 100, f32::NAN).is_err());
    }

    #[test]
    fn pending_scrolls_merge_per_display_and_keep_the_original_target() {
        let rect = Rect {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let first = Monitor {
            bounds: rect,
            work_area: rect,
            primary: true,
            device_id: [0; 128],
        };
        let second = Monitor {
            bounds: Rect {
                left: -100,
                right: 0,
                ..rect
            },
            ..first
        };
        let mut state = WorkerState {
            pending: VecDeque::new(),
            error: None,
            started: false,
        };
        state.enqueue(first, 0.05);
        state.enqueue(second, -0.05);
        state.enqueue(first, 0.05);
        assert_eq!(state.pending.pop_front(), Some((first, 0.1)));
        assert_eq!(state.pending.pop_front(), Some((second, -0.05)));
        state.enqueue(first, 0.05);
        state.enqueue(first, -0.05);
        assert_eq!(state.pending.pop_front(), Some((first, 0.0)));
    }
}
