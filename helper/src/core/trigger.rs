use crate::config::{HotzoneId, TriggerKind};
use crate::platform::InputState;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct HotzoneTriggerController {
    current_hotzone: Option<HotzoneId>,
    entered_at: Instant,
    hover_triggered_for_entry: bool,
    slide_accumulator: i32,
    continuous_motion: HashMap<(HotzoneId, TriggerKind), f32>,
    last_triggered_at: HashMap<(HotzoneId, TriggerKind), Instant>,
}

impl HotzoneTriggerController {
    pub fn new(now: Instant) -> Self {
        Self {
            current_hotzone: None,
            entered_at: now,
            hover_triggered_for_entry: false,
            slide_accumulator: 0,
            continuous_motion: HashMap::new(),
            last_triggered_at: HashMap::new(),
        }
    }

    pub fn observe(&mut self, now: Instant, detected: Option<HotzoneId>) {
        if detected != self.current_hotzone {
            self.current_hotzone = detected;
            self.entered_at = now;
            self.hover_triggered_for_entry = false;
            self.slide_accumulator = 0;
            self.continuous_motion.clear();
        }
    }

    pub fn suspend(&mut self, now: Instant) {
        self.current_hotzone = None;
        self.entered_at = now;
        self.hover_triggered_for_entry = false;
        self.slide_accumulator = 0;
        self.continuous_motion.clear();
    }

    pub fn record_slide_motion(&mut self, motion: i32) {
        self.slide_accumulator = (self.slide_accumulator + motion).clamp(-200, 200);
    }

    pub fn clear_slide_motion(&mut self) {
        self.slide_accumulator = 0;
    }

    pub fn accumulate_continuous_motion(
        &mut self,
        now: Instant,
        hotzone_id: HotzoneId,
        trigger: TriggerKind,
        motion: f32,
        cooldown: Duration,
    ) -> Option<f32> {
        if self.current_hotzone != Some(hotzone_id) || !motion.is_finite() || motion < 0.0 {
            return None;
        }

        let key = (hotzone_id, trigger);
        if motion > 0.0 {
            let pending = self.continuous_motion.entry(key).or_default();
            *pending = (*pending + motion).min(12.0);
        }
        if !self.continuous_motion.contains_key(&key) {
            return None;
        }
        if !self.cooldown_elapsed(now, hotzone_id, trigger, cooldown) {
            return None;
        }

        let accumulated = self.continuous_motion.remove(&key)?;
        self.last_triggered_at.insert((hotzone_id, trigger), now);
        Some(accumulated)
    }

    pub fn should_trigger(
        &mut self,
        now: Instant,
        hotzone_id: HotzoneId,
        trigger: TriggerKind,
        input: InputState,
        previous_input: InputState,
        hover_delay: Duration,
        cooldown: Duration,
    ) -> bool {
        if self.current_hotzone != Some(hotzone_id) {
            return false;
        }

        let trigger_event = match trigger {
            TriggerKind::Hover => {
                !self.hover_triggered_for_entry
                    && now.duration_since(self.entered_at) >= hover_delay
            }
            TriggerKind::LeftClick => input.left_clicked_since(previous_input),
            TriggerKind::RightClick => input.right_clicked_since(previous_input),
            TriggerKind::WheelUp => input.wheel_up_since(previous_input),
            TriggerKind::WheelDown => input.wheel_down_since(previous_input),
            TriggerKind::SlideForward => self.slide_accumulator >= 16,
            TriggerKind::SlideBackward => self.slide_accumulator <= -16,
        };

        if !trigger_event || !self.cooldown_elapsed(now, hotzone_id, trigger, cooldown) {
            return false;
        }

        if matches!(trigger, TriggerKind::Hover) {
            self.hover_triggered_for_entry = true;
        }
        if matches!(
            trigger,
            TriggerKind::SlideForward | TriggerKind::SlideBackward
        ) {
            self.slide_accumulator = 0;
        }
        self.last_triggered_at.insert((hotzone_id, trigger), now);
        true
    }

    fn cooldown_elapsed(
        &self,
        now: Instant,
        hotzone_id: HotzoneId,
        trigger: TriggerKind,
        cooldown: Duration,
    ) -> bool {
        self.last_triggered_at
            .get(&(hotzone_id, trigger))
            .is_none_or(|last| now.duration_since(*last) >= cooldown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(left_down: bool, right_down: bool, wheel_delta: i64) -> InputState {
        InputState {
            left_down,
            right_down,
            wheel_delta,
            ..Default::default()
        }
    }

    #[test]
    fn hover_triggers_once_per_entry() {
        let start = Instant::now();
        let mut controller = HotzoneTriggerController::new(start);
        let hover_delay = Duration::from_millis(100);
        let cooldown = Duration::from_millis(100);

        controller.observe(start, Some(HotzoneId::TopLeft));
        assert!(!controller.should_trigger(
            start + Duration::from_millis(99),
            HotzoneId::TopLeft,
            TriggerKind::Hover,
            InputState::default(),
            InputState::default(),
            hover_delay,
            cooldown,
        ));
        assert!(controller.should_trigger(
            start + Duration::from_millis(100),
            HotzoneId::TopLeft,
            TriggerKind::Hover,
            InputState::default(),
            InputState::default(),
            hover_delay,
            cooldown,
        ));
        assert!(!controller.should_trigger(
            start + Duration::from_millis(500),
            HotzoneId::TopLeft,
            TriggerKind::Hover,
            InputState::default(),
            InputState::default(),
            hover_delay,
            cooldown,
        ));
    }

    #[test]
    fn hover_rearms_after_leaving_and_reentering() {
        let start = Instant::now();
        let mut controller = HotzoneTriggerController::new(start);
        let hover_delay = Duration::from_millis(50);
        let cooldown = Duration::from_millis(10);

        controller.observe(start, Some(HotzoneId::TopLeft));
        assert!(controller.should_trigger(
            start + Duration::from_millis(50),
            HotzoneId::TopLeft,
            TriggerKind::Hover,
            InputState::default(),
            InputState::default(),
            hover_delay,
            cooldown,
        ));

        controller.observe(start + Duration::from_millis(60), None);
        controller.observe(start + Duration::from_millis(70), Some(HotzoneId::TopLeft));

        assert!(!controller.should_trigger(
            start + Duration::from_millis(100),
            HotzoneId::TopLeft,
            TriggerKind::Hover,
            InputState::default(),
            InputState::default(),
            hover_delay,
            cooldown,
        ));
        assert!(controller.should_trigger(
            start + Duration::from_millis(120),
            HotzoneId::TopLeft,
            TriggerKind::Hover,
            InputState::default(),
            InputState::default(),
            hover_delay,
            cooldown,
        ));
    }

    #[test]
    fn click_trigger_can_repeat_after_cooldown() {
        let start = Instant::now();
        let mut controller = HotzoneTriggerController::new(start);
        let hover_delay = Duration::from_millis(0);
        let cooldown = Duration::from_millis(100);

        controller.observe(start, Some(HotzoneId::Right));
        assert!(controller.should_trigger(
            start + Duration::from_millis(1),
            HotzoneId::Right,
            TriggerKind::LeftClick,
            input(true, false, 0),
            input(false, false, 0),
            hover_delay,
            cooldown,
        ));
        assert!(!controller.should_trigger(
            start + Duration::from_millis(50),
            HotzoneId::Right,
            TriggerKind::LeftClick,
            input(true, false, 0),
            input(false, false, 0),
            hover_delay,
            cooldown,
        ));
        assert!(controller.should_trigger(
            start + Duration::from_millis(150),
            HotzoneId::Right,
            TriggerKind::LeftClick,
            input(true, false, 0),
            input(false, false, 0),
            hover_delay,
            cooldown,
        ));
    }

    #[test]
    fn switching_hotzones_resets_hover_timing() {
        let start = Instant::now();
        let mut controller = HotzoneTriggerController::new(start);
        let hover_delay = Duration::from_millis(100);
        let cooldown = Duration::from_millis(0);

        controller.observe(start, Some(HotzoneId::TopLeft));
        controller.observe(start + Duration::from_millis(80), Some(HotzoneId::TopRight));

        assert!(!controller.should_trigger(
            start + Duration::from_millis(100),
            HotzoneId::TopRight,
            TriggerKind::Hover,
            InputState::default(),
            InputState::default(),
            hover_delay,
            cooldown,
        ));
        assert!(controller.should_trigger(
            start + Duration::from_millis(180),
            HotzoneId::TopRight,
            TriggerKind::Hover,
            InputState::default(),
            InputState::default(),
            hover_delay,
            cooldown,
        ));
    }

    #[test]
    fn suspension_discards_the_current_hotzone_entry() {
        let start = Instant::now();
        let mut controller = HotzoneTriggerController::new(start);

        controller.observe(start, Some(HotzoneId::TopLeft));
        controller.suspend(start + Duration::from_millis(500));

        assert!(!controller.should_trigger(
            start + Duration::from_millis(500),
            HotzoneId::TopLeft,
            TriggerKind::Hover,
            InputState::default(),
            InputState::default(),
            Duration::from_millis(100),
            Duration::from_millis(100),
        ));
    }

    #[test]
    fn wheel_directions_and_slides_are_independent_triggers() {
        let start = Instant::now();
        let mut controller = HotzoneTriggerController::new(start);
        controller.observe(start, Some(HotzoneId::Right));
        let delay = Duration::ZERO;
        let cooldown = Duration::ZERO;

        assert!(controller.should_trigger(
            start + Duration::from_millis(1),
            HotzoneId::Right,
            TriggerKind::WheelUp,
            input(false, false, 120),
            input(false, false, 0),
            delay,
            cooldown,
        ));
        assert!(!controller.should_trigger(
            start + Duration::from_millis(2),
            HotzoneId::Right,
            TriggerKind::WheelDown,
            input(false, false, 120),
            input(false, false, 0),
            delay,
            cooldown,
        ));
        controller.record_slide_motion(8);
        controller.record_slide_motion(8);
        assert!(controller.should_trigger(
            start + Duration::from_millis(3),
            HotzoneId::Right,
            TriggerKind::SlideForward,
            input(false, false, 120),
            input(false, false, 120),
            delay,
            cooldown,
        ));
        controller.record_slide_motion(-16);
        assert!(controller.should_trigger(
            start + Duration::from_millis(4),
            HotzoneId::Right,
            TriggerKind::SlideBackward,
            input(false, false, 120),
            input(false, false, 120),
            delay,
            cooldown,
        ));
    }

    #[test]
    fn continuous_motion_is_preserved_during_cooldown() {
        let start = Instant::now();
        let mut controller = HotzoneTriggerController::new(start);
        controller.observe(start, Some(HotzoneId::Right));
        let cooldown = Duration::from_millis(100);

        assert_eq!(
            controller.accumulate_continuous_motion(
                start,
                HotzoneId::Right,
                TriggerKind::WheelUp,
                1.0,
                cooldown,
            ),
            Some(1.0)
        );
        assert_eq!(
            controller.accumulate_continuous_motion(
                start + Duration::from_millis(40),
                HotzoneId::Right,
                TriggerKind::WheelUp,
                0.5,
                cooldown,
            ),
            None
        );
        assert_eq!(
            controller.accumulate_continuous_motion(
                start + Duration::from_millis(100),
                HotzoneId::Right,
                TriggerKind::WheelUp,
                0.0,
                cooldown,
            ),
            Some(0.5)
        );
    }
}
