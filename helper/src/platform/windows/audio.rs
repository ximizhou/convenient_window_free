use super::keyboard::key_input;
use anyhow::{bail, Result};
use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, VK_VOLUME_DOWN, VK_VOLUME_UP};

pub fn adjust_volume(delta: f32) -> Result<()> {
    if !delta.is_finite() || delta == 0.0 {
        bail!("volume delta must be a finite non-zero value");
    }
    let key = if delta > 0.0 {
        VK_VOLUME_UP
    } else {
        VK_VOLUME_DOWN
    };
    let inputs = [key_input(key, false), key_input(key, true)];
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

    #[test]
    fn a_partial_insertion_ending_on_a_press_is_released() {
        assert!(pending_release(1, 2));
    }

    #[test]
    fn complete_and_empty_insertions_leave_no_key_down() {
        assert!(!pending_release(0, 2));
        assert!(!pending_release(2, 2));
    }

    #[test]
    fn invalid_deltas_are_rejected_before_any_key_is_injected() {
        for delta in [0.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(adjust_volume(delta).is_err());
        }
    }
}
