use super::keyboard::key_input;
use anyhow::{bail, Result};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, VIRTUAL_KEY, VK_VOLUME_DOWN, VK_VOLUME_UP,
};

/// Nominal size of one volume step, as a fraction of the full scale. A media key
/// press moves the endpoint by whatever step the shell uses, not by a value we
/// choose, so this is only our model of that step: measured as exactly 2% on a
/// Windows 11 machine whose `GetVolumeStepInfo` reported 51 steps. Treat a
/// mismatch on other configurations as a scaling error, not a correctness one -
/// the direction and the relative magnitude still hold.
const NOMINAL_STEP: f32 = 0.02;

/// `core::actions::scaled_adjustment_delta` clamps one adjustment to ±0.12, i.e.
/// six nominal steps. Bound the burst at that so a direct caller cannot turn one
/// dispatch into an arbitrarily long key sequence.
const MAX_PRESSES: u32 = 6;

/// Accumulated motion is worthless once the gesture that produced it is over, so
/// drop a carried remainder that has not been touched for this long.
const CARRY_IDLE_TIMEOUT: Duration = Duration::from_secs(1);

/// Sub-step motion left over from earlier adjustments, and when it was recorded.
static CARRY: Mutex<Option<(f32, Instant)>> = Mutex::new(None);

pub fn adjust_volume(delta: f32) -> Result<()> {
    if !delta.is_finite() || delta == 0.0 {
        bail!("volume delta must be a finite non-zero value");
    }
    let key = if delta > 0.0 {
        VK_VOLUME_UP
    } else {
        VK_VOLUME_DOWN
    };

    let now = Instant::now();
    let (presses, carry) = plan_presses(take_carry(now), delta);
    if presses == 0 {
        store_carry(carry, now);
        return Ok(());
    }

    // `take_carry` already cleared the old remainder, and nothing is written back
    // on the failure path: a partial insertion delivered an unknown fraction of
    // the request, and replaying the shortfall would move the volume long after
    // the gesture that asked for it ended.
    send_presses(key, presses)?;
    store_carry(carry, now);
    Ok(())
}

/// Converts an adjustment plus whatever sub-step motion is carried over into a
/// whole number of key presses, and returns the motion still left over.
///
/// The shell only accepts whole steps, so the sign-only dispatch this replaced
/// turned every adjustment into exactly one step: a slow, deliberate scroll moved
/// the volume as far as a fast one, and several small motions merged during a
/// cooldown moved it no further than a single notch. Carrying the remainder keeps
/// the delivered motion proportional to the requested motion over time.
fn plan_presses(carried: f32, delta: f32) -> (u32, f32) {
    if !delta.is_finite() {
        return (0, 0.0);
    }
    // Only a remainder pointing the same way as this adjustment is still owed;
    // otherwise reversing direction would first have to repay the old debt,
    // which reads as a dead zone right after a direction change.
    let carried = if carried.is_finite() && carried.signum() == delta.signum() {
        carried
    } else {
        0.0
    };

    let magnitude = (carried + delta).abs();
    // Accumulating in f32 can land a hair below a whole step (four 0.005 deltas
    // sum to just under 0.02), which would hold back a step the caller has paid
    // for. Tolerate that by a thousandth of a step.
    let whole = (magnitude / NOMINAL_STEP + 1e-3).floor();
    if whole >= MAX_PRESSES as f32 {
        return (MAX_PRESSES, 0.0);
    }
    let remainder = (magnitude - whole * NOMINAL_STEP).max(0.0);
    (whole as u32, remainder.copysign(delta))
}

fn take_carry(now: Instant) -> f32 {
    let mut carry = CARRY.lock().unwrap_or_else(|error| error.into_inner());
    match carry.take() {
        Some((value, recorded)) if now.duration_since(recorded) <= CARRY_IDLE_TIMEOUT => value,
        _ => 0.0,
    }
}

fn store_carry(value: f32, now: Instant) {
    let mut carry = CARRY.lock().unwrap_or_else(|error| error.into_inner());
    *carry = (value != 0.0).then_some((value, now));
}

/// Sends the presses as one insertion. A single `SendInput` batch of N press and
/// release pairs was measured to move the endpoint exactly N steps, with no gap
/// needed between pairs, so this stays on the dispatch thread rather than needing
/// the worker `brightness` uses.
fn send_presses(key: VIRTUAL_KEY, presses: u32) -> Result<()> {
    let mut inputs = Vec::with_capacity(presses as usize * 2);
    for _ in 0..presses {
        inputs.push(key_input(key, false));
        inputs.push(key_input(key, true));
    }
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) } as usize;
    if sent == inputs.len() {
        return Ok(());
    }

    let recovery = if pending_release(sent, inputs.len()) {
        vec![key_input(key, true)]
    } else {
        Vec::new()
    };
    let recovered = if recovery.is_empty() {
        0
    } else {
        (unsafe { SendInput(&recovery, std::mem::size_of::<INPUT>() as i32) }) as usize
    };
    bail!(
        "SendInput sent {sent} of {} volume key events; recovery sent {recovered} of {} events",
        inputs.len(),
        recovery.len()
    )
}

/// The planned sequence is press/release pairs for a single key, so a partial
/// insertion stops on a press exactly when an odd number of events landed. That
/// press leaves the volume key logically down, so release it before reporting the
/// failure, the way `keyboard::send_key_sequence` recovers shortcut injection.
fn pending_release(sent: usize, planned: usize) -> bool {
    sent < planned && sent % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds a sequence of adjustments through the planner the way repeated
    /// dispatches would, and reports the total presses emitted.
    fn replay(deltas: &[f32]) -> u32 {
        let mut carried = 0.0;
        let mut presses = 0;
        for delta in deltas {
            let (emitted, remainder) = plan_presses(carried, *delta);
            presses += emitted;
            carried = remainder;
        }
        presses
    }

    #[test]
    fn one_nominal_step_still_sends_exactly_one_press() {
        assert_eq!(plan_presses(0.0, 0.02).0, 1);
        assert_eq!(plan_presses(0.0, -0.02).0, 1);
    }

    #[test]
    fn a_larger_adjustment_sends_proportionally_more_presses() {
        assert_eq!(plan_presses(0.0, 0.04).0, 2);
        assert_eq!(plan_presses(0.0, 0.12).0, 6);
    }

    #[test]
    fn motion_below_one_step_is_carried_instead_of_rounded_up() {
        let (presses, carried) = plan_presses(0.0, 0.005);
        assert_eq!(presses, 0);
        assert!((carried - 0.005).abs() < 1e-6);
    }

    #[test]
    fn carried_motion_pays_out_once_it_reaches_a_whole_step() {
        assert_eq!(replay(&[0.005; 3]), 0);
        assert_eq!(replay(&[0.005; 4]), 1);
        assert_eq!(replay(&[0.005; 8]), 2);
    }

    #[test]
    fn the_same_total_costs_the_same_however_it_is_batched() {
        assert_eq!(replay(&[0.06]), replay(&[0.02; 3]));
        assert_eq!(replay(&[0.06]), replay(&[0.01; 6]));
        assert_eq!(replay(&[-0.06]), replay(&[-0.015; 4]));
    }

    #[test]
    fn a_partial_step_never_overshoots_the_request() {
        // 0.05 is two and a half steps: send two and keep the half.
        let (presses, carried) = plan_presses(0.0, 0.05);
        assert_eq!(presses, 2);
        assert!((carried - 0.01).abs() < 1e-6);
    }

    #[test]
    fn reversing_direction_drops_the_carried_remainder() {
        let (presses, carried) = plan_presses(0.015, -0.02);
        assert_eq!(presses, 1);
        assert!(carried.abs() < 1e-6);
    }

    #[test]
    fn a_burst_is_bounded_even_if_the_caller_skips_the_clamp() {
        let (presses, carried) = plan_presses(0.0, 5.0);
        assert_eq!(presses, MAX_PRESSES);
        assert_eq!(carried, 0.0);
    }

    #[test]
    fn non_finite_input_plans_nothing() {
        for delta in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(plan_presses(0.01, delta), (0, 0.0));
        }
        assert_eq!(plan_presses(f32::NAN, 0.02), (1, 0.0));
    }

    #[test]
    fn a_stale_remainder_is_discarded() {
        let recorded = Instant::now();
        store_carry(0.01, recorded);
        assert_eq!(
            take_carry(recorded + CARRY_IDLE_TIMEOUT + Duration::from_millis(1)),
            0.0
        );

        store_carry(0.01, recorded);
        assert_eq!(take_carry(recorded), 0.01);
    }

    #[test]
    fn a_partial_insertion_ending_on_a_press_is_released() {
        assert!(pending_release(1, 2));
        assert!(pending_release(3, 12));
    }

    #[test]
    fn complete_and_empty_insertions_leave_no_key_down() {
        assert!(!pending_release(0, 2));
        assert!(!pending_release(2, 2));
        assert!(!pending_release(12, 12));
    }

    #[test]
    fn invalid_deltas_are_rejected_before_any_key_is_injected() {
        for delta in [0.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(adjust_volume(delta).is_err());
        }
    }
}
