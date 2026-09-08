use anyhow::{bail, Result};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_VOLUME_DOWN,
    VK_VOLUME_UP,
};

pub fn adjust_volume(delta: f32) -> Result<()> {
    if !delta.is_finite() || delta == 0.0 {
        bail!("volume delta must be a finite non-zero value");
    }
    let key = if delta > 0.0 {
        VK_VOLUME_UP
    } else {
        VK_VOLUME_DOWN
    };
    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        },
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) } as usize;
    if sent == inputs.len() {
        Ok(())
    } else {
        bail!("failed to send system volume key")
    }
}
