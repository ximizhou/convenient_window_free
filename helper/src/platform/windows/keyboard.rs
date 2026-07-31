use anyhow::{bail, Result};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3,
    VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_MEDIA_PLAY_PAUSE, VK_MENU, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SHIFT, VK_SPACE, VK_TAB, VK_UP, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
};

pub fn send_shortcut_with_modifiers(shortcut: &str, routing_modifiers: u8) -> Result<()> {
    let keys = parse_shortcut(shortcut)?;
    send_key_sequence(&keys, &active_routing_keys(routing_modifiers))
}

pub fn show_desktop_with_modifiers(routing_modifiers: u8) -> Result<()> {
    send_key_sequence(
        &[VK_LWIN, VIRTUAL_KEY(b'D' as u16)],
        &active_routing_keys(routing_modifiers),
    )
}

fn parse_shortcut(shortcut: &str) -> Result<Vec<VIRTUAL_KEY>> {
    let mut keys = Vec::new();
    for part in shortcut.split('+').map(|item| item.trim().to_lowercase()) {
        if part.is_empty() {
            continue;
        }

        match parse_key(&part) {
            Some(key) => keys.push(key),
            None => bail!("unsupported shortcut key: {part}"),
        }
    }

    if keys.is_empty() {
        bail!("shortcut is empty");
    }

    Ok(keys)
}

fn parse_key(part: &str) -> Option<VIRTUAL_KEY> {
    match part {
        "ctrl" | "control" => Some(VK_CONTROL),
        "shift" => Some(VK_SHIFT),
        "alt" => Some(VK_MENU),
        "win" | "meta" => Some(VK_LWIN),
        "enter" | "return" => Some(VK_RETURN),
        "esc" | "escape" => Some(VK_ESCAPE),
        "tab" => Some(VK_TAB),
        "space" => Some(VK_SPACE),
        "left" => Some(VK_LEFT),
        "right" => Some(VK_RIGHT),
        "up" => Some(VK_UP),
        "down" => Some(VK_DOWN),
        "volumeup" | "volume-up" => Some(VK_VOLUME_UP),
        "volumedown" | "volume-down" => Some(VK_VOLUME_DOWN),
        "volumemute" | "volume-mute" => Some(VK_VOLUME_MUTE),
        "mediaplaypause" | "media-play-pause" => Some(VK_MEDIA_PLAY_PAUSE),
        "f1" => Some(VK_F1),
        "f2" => Some(VK_F2),
        "f3" => Some(VK_F3),
        "f4" => Some(VK_F4),
        "f5" => Some(VK_F5),
        "f6" => Some(VK_F6),
        "f7" => Some(VK_F7),
        "f8" => Some(VK_F8),
        "f9" => Some(VK_F9),
        "f10" => Some(VK_F10),
        "f11" => Some(VK_F11),
        "f12" => Some(VK_F12),
        _ => parse_ascii_key(part),
    }
}

fn parse_ascii_key(part: &str) -> Option<VIRTUAL_KEY> {
    let bytes = part.as_bytes();
    if bytes.len() != 1 {
        return None;
    }

    let byte = bytes[0].to_ascii_uppercase();
    if byte.is_ascii_alphanumeric() {
        Some(VIRTUAL_KEY(byte as u16))
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlannedKeyInput {
    key: VIRTUAL_KEY,
    key_up: bool,
}

fn send_key_sequence(keys: &[VIRTUAL_KEY], routing_keys: &[VIRTUAL_KEY]) -> Result<()> {
    let planned = planned_key_sequence(keys, routing_keys);
    let inputs = planned
        .iter()
        .map(|event| key_input(event.key, event.key_up))
        .collect::<Vec<_>>();
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) } as usize;
    if sent == inputs.len() {
        return Ok(());
    }

    let recovery = recovery_key_sequence(keys, routing_keys, &planned, sent);
    let recovery_inputs = recovery
        .iter()
        .map(|event| key_input(event.key, event.key_up))
        .collect::<Vec<_>>();
    let recovered = if recovery_inputs.is_empty() {
        0
    } else {
        (unsafe { SendInput(&recovery_inputs, std::mem::size_of::<INPUT>() as i32) }) as usize
    };
    bail!(
        "SendInput sent {sent} of {} events; recovery sent {recovered} of {} events",
        inputs.len(),
        recovery_inputs.len()
    )
}

fn planned_key_sequence(
    keys: &[VIRTUAL_KEY],
    routing_keys: &[VIRTUAL_KEY],
) -> Vec<PlannedKeyInput> {
    let mut inputs = Vec::with_capacity(keys.len() * 2 + routing_keys.len() * 2);
    inputs.extend(
        routing_keys
            .iter()
            .rev()
            .map(|key| planned_key_input(*key, true)),
    );
    inputs.extend(keys.iter().map(|key| planned_key_input(*key, false)));
    inputs.extend(keys.iter().rev().map(|key| planned_key_input(*key, true)));
    inputs.extend(
        routing_keys
            .iter()
            .map(|key| planned_key_input(*key, false)),
    );
    inputs
}

fn recovery_key_sequence(
    keys: &[VIRTUAL_KEY],
    routing_keys: &[VIRTUAL_KEY],
    planned: &[PlannedKeyInput],
    sent: usize,
) -> Vec<PlannedKeyInput> {
    let mut tracked = Vec::with_capacity(keys.len() + routing_keys.len());
    for key in keys.iter().chain(routing_keys) {
        if !tracked.contains(key) {
            tracked.push(*key);
        }
    }
    let mut state = tracked
        .iter()
        .map(|key| (*key, routing_keys.contains(key)))
        .collect::<Vec<_>>();
    for event in planned.iter().take(sent.min(planned.len())) {
        if let Some((_, down)) = state.iter_mut().find(|(key, _)| *key == event.key) {
            *down = !event.key_up;
        }
    }

    let mut recovery = Vec::new();
    for (key, down) in state.iter().rev() {
        if *down && !routing_keys.contains(key) {
            recovery.push(planned_key_input(*key, true));
        }
    }
    for key in routing_keys {
        if state
            .iter()
            .find(|(tracked, _)| tracked == key)
            .is_some_and(|(_, down)| !*down)
        {
            recovery.push(planned_key_input(*key, false));
        }
    }
    recovery
}

fn planned_key_input(key: VIRTUAL_KEY, key_up: bool) -> PlannedKeyInput {
    PlannedKeyInput { key, key_up }
}

fn active_routing_keys(mask: u8) -> Vec<VIRTUAL_KEY> {
    let mut keys = Vec::with_capacity(4);
    append_active_modifier(
        &mut keys,
        mask & 1 != 0,
        VK_CONTROL,
        &[VK_LCONTROL, VK_RCONTROL],
    );
    append_active_modifier(&mut keys, mask & 2 != 0, VK_MENU, &[VK_LMENU, VK_RMENU]);
    append_active_modifier(&mut keys, mask & 4 != 0, VK_SHIFT, &[VK_LSHIFT, VK_RSHIFT]);
    append_active_modifier(&mut keys, mask & 8 != 0, VK_LWIN, &[VK_LWIN, VK_RWIN]);
    keys
}

fn append_active_modifier(
    output: &mut Vec<VIRTUAL_KEY>,
    routed: bool,
    generic: VIRTUAL_KEY,
    sided: &[VIRTUAL_KEY],
) {
    if !routed {
        return;
    }
    let start = output.len();
    output.extend(sided.iter().copied().filter(|key| key_is_down(*key)));
    if output.len() == start && key_is_down(generic) {
        output.push(generic);
    }
}

fn key_is_down(key: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(key.0 as i32) & 0x8000u16 as i16) != 0 }
}

fn key_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_letters_numbers_and_modifiers() {
        let keys = parse_shortcut("Ctrl+Alt+T").unwrap();
        assert_eq!(keys, vec![VK_CONTROL, VK_MENU, VIRTUAL_KEY(b'T' as u16)]);

        let keys = parse_shortcut("Win+1").unwrap();
        assert_eq!(keys, vec![VK_LWIN, VIRTUAL_KEY(b'1' as u16)]);

        assert_eq!(parse_shortcut("VolumeUp").unwrap(), vec![VK_VOLUME_UP]);
        assert_eq!(
            parse_shortcut("MediaPlayPause").unwrap(),
            vec![VK_MEDIA_PLAY_PAUSE]
        );
    }

    #[test]
    fn isolates_ctrl_before_show_desktop_and_restores_it_afterwards() {
        let sequence = planned_key_sequence(&[VK_LWIN, VIRTUAL_KEY(b'D' as u16)], &[VK_CONTROL]);
        assert_eq!(
            sequence,
            vec![
                planned_key_input(VK_CONTROL, true),
                planned_key_input(VK_LWIN, false),
                planned_key_input(VIRTUAL_KEY(b'D' as u16), false),
                planned_key_input(VIRTUAL_KEY(b'D' as u16), true),
                planned_key_input(VK_LWIN, true),
                planned_key_input(VK_CONTROL, false),
            ]
        );
    }

    #[test]
    fn isolates_shift_around_a_ctrl_shortcut() {
        let keys = parse_shortcut("Ctrl+C").unwrap();
        let sequence = planned_key_sequence(&keys, &[VK_LSHIFT]);
        assert_eq!(sequence.first(), Some(&planned_key_input(VK_LSHIFT, true)));
        assert_eq!(sequence.last(), Some(&planned_key_input(VK_LSHIFT, false)));
        assert!(sequence.contains(&planned_key_input(VK_CONTROL, false)));
        assert!(sequence.contains(&planned_key_input(VIRTUAL_KEY(b'C' as u16), true)));
    }

    #[test]
    fn every_partial_send_recovers_target_keys_and_routing_modifiers() {
        let keys = [VK_LWIN, VIRTUAL_KEY(b'D' as u16)];
        let routing = [VK_CONTROL, VK_LSHIFT];
        let planned = planned_key_sequence(&keys, &routing);
        let tracked = [VK_LWIN, VIRTUAL_KEY(b'D' as u16), VK_CONTROL, VK_LSHIFT];

        for sent in 0..planned.len() {
            let recovery = recovery_key_sequence(&keys, &routing, &planned, sent);
            for key in tracked {
                let down = key_state_after(
                    key,
                    routing.contains(&key),
                    planned.iter().take(sent).chain(&recovery),
                );
                assert_eq!(
                    down,
                    routing.contains(&key),
                    "sent prefix {sent}, key {:?}",
                    key
                );
            }
        }
    }

    fn key_state_after<'a>(
        key: VIRTUAL_KEY,
        initially_down: bool,
        events: impl Iterator<Item = &'a PlannedKeyInput>,
    ) -> bool {
        events
            .filter(|event| event.key == key)
            .fold(initially_down, |_, event| !event.key_up)
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(parse_shortcut("Ctrl+Nope").is_err());
    }
}
