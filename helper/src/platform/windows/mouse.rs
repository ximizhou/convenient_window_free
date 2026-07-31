use crate::config::GestureTriggerButton;
use crate::platform::Point;
use anyhow::{bail, Result};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

pub const GESTURE_INJECT_TAG: usize = 0x5847_5354;

pub fn cursor_position() -> Result<Point> {
    let mut point = POINT::default();
    unsafe {
        GetCursorPos(&mut point)?;
    }

    Ok(Point {
        x: point.x,
        y: point.y,
    })
}

pub fn send_trigger_click(trigger: GestureTriggerButton) -> Result<()> {
    let (down, up, mouse_data) = match trigger {
        GestureTriggerButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
        GestureTriggerButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
        GestureTriggerButton::X1 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 1),
        GestureTriggerButton::X2 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 2),
    };
    let inputs = [mouse_input(down, mouse_data), mouse_input(up, mouse_data)];
    super::input::mark_helper_injected_events(inputs.len() as u8);
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        bail!(
            "SendInput only injected {sent} of {} mouse events",
            inputs.len()
        );
    }
    Ok(())
}

fn mouse_input(
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
    mouse_data: u32,
) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: mouse_data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: GESTURE_INJECT_TAG,
            },
        },
    }
}
