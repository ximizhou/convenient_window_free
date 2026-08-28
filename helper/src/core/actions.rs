use crate::config::{ActionKind, HotzoneAction};
use crate::ipc::messages::HelperMessage;
use crate::platform;
use crate::platform::Point;
use anyhow::{bail, Result};
use serde_json::json;
use std::process::Command;
use tokio::sync::broadcast;

pub struct ActionDispatcher {
    event_tx: broadcast::Sender<HelperMessage>,
}

impl ActionDispatcher {
    pub fn new(event_tx: broadcast::Sender<HelperMessage>) -> Self {
        Self { event_tx }
    }

    pub fn dispatch_with_modifiers(
        &self,
        action: &HotzoneAction,
        source: &str,
        routing_modifiers: u8,
    ) -> Result<()> {
        self.dispatch_scaled_at(action, source, 1.0, None, routing_modifiers)
    }

    pub fn dispatch_at_with_modifiers(
        &self,
        action: &HotzoneAction,
        source: &str,
        target_point: Option<Point>,
        routing_modifiers: u8,
    ) -> Result<()> {
        self.dispatch_scaled_at(action, source, 1.0, target_point, routing_modifiers)
    }

    pub fn dispatch_scaled_with_modifiers(
        &self,
        action: &HotzoneAction,
        source: &str,
        scale: f32,
        routing_modifiers: u8,
    ) -> Result<()> {
        self.dispatch_scaled_at(action, source, scale, None, routing_modifiers)
    }

    fn dispatch_scaled_at(
        &self,
        action: &HotzoneAction,
        source: &str,
        scale: f32,
        target_point: Option<Point>,
        routing_modifiers: u8,
    ) -> Result<()> {
        if action.kind == ActionKind::None {
            return Ok(());
        }
        match action.kind {
            ActionKind::None => Ok(()),
            ActionKind::Shortcut => {
                platform::send_shortcut_with_modifiers(
                    required_value(action, "shortcut")?,
                    routing_modifiers,
                )?;
                Ok(())
            }
            ActionKind::ShowDesktop => platform::show_desktop_with_modifiers(routing_modifiers),
            ActionKind::ToggleWindowTopmost => {
                let (title, topmost) = platform::toggle_window_topmost_at(target_point)?;
                let message = if title.trim().is_empty() {
                    if topmost {
                        "窗口已置顶".to_string()
                    } else {
                        "窗口已取消置顶".to_string()
                    }
                } else if topmost {
                    format!("已置顶：{title}")
                } else {
                    format!("已取消置顶：{title}")
                };
                let _ = self.event_tx.send(HelperMessage::new(
                    "runtime.status",
                    json!({ "message": message }),
                ));
                Ok(())
            }
            ActionKind::LockScreen => platform::lock_screen(),
            ActionKind::VolumeAdjust => platform::adjust_volume(scaled_volume_delta(
                required_value(action, "volume adjustment")?,
                scale,
            )?),
            ActionKind::OpenCommand => {
                open_command(required_value(action, "command")?)?;
                Ok(())
            }
            ActionKind::HostAction => {
                let value = required_value(action, "host action")?;
                let _ = self.event_tx.send(HelperMessage::new(
                    "host.action",
                    json!({
                        "kind": "redirect",
                        "value": value,
                        "source": source
                    }),
                ));
                Ok(())
            }
        }?;

        let _ = self.event_tx.send(HelperMessage::new(
            "action.triggered",
            json!({
                "source": source,
                "kind": action.kind,
                "value": action.value
            }),
        ));

        Ok(())
    }
}

fn open_command(command: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    Command::new("cmd").args(["/C", command]).spawn()?;
    #[cfg(not(target_os = "windows"))]
    Command::new("sh").args(["-c", command]).spawn()?;
    Ok(())
}

fn scaled_volume_delta(value: &str, scale: f32) -> Result<f32> {
    let base = value.trim().parse::<f32>()?;
    if !base.is_finite() || base == 0.0 || !scale.is_finite() || scale <= 0.0 {
        bail!("volume adjustment requires finite non-zero delta and positive scale");
    }
    Ok((base * scale).clamp(-0.12, 0.12))
}

fn required_value<'a>(action: &'a HotzoneAction, label: &str) -> Result<&'a str> {
    let Some(value) = action
        .value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        bail!("{label} action requires a non-empty value");
    };
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameterized_actions_reject_blank_values() {
        let (event_tx, _) = broadcast::channel(4);
        let dispatcher = ActionDispatcher::new(event_tx);

        for kind in [
            ActionKind::Shortcut,
            ActionKind::VolumeAdjust,
            ActionKind::OpenCommand,
            ActionKind::HostAction,
        ] {
            let action = HotzoneAction { kind, value: None };
            assert!(dispatcher
                .dispatch_with_modifiers(&action, "test", 0)
                .is_err());

            let action = HotzoneAction {
                kind,
                value: Some("   ".to_string()),
            };
            assert!(dispatcher
                .dispatch_with_modifiers(&action, "test", 0)
                .is_err());
        }
    }

    #[test]
    fn host_action_accepts_legacy_config_and_emits_a_generic_event() {
        let action: HotzoneAction = serde_json::from_value(json!({
            "kind": "utools-redirect",
            "value": "feature-code"
        }))
        .unwrap();
        assert_eq!(action.kind, ActionKind::HostAction);
        assert_eq!(
            serde_json::to_value(action.kind).unwrap(),
            json!("host-action")
        );

        let (event_tx, mut event_rx) = broadcast::channel(4);
        let dispatcher = ActionDispatcher::new(event_tx);
        dispatcher
            .dispatch_with_modifiers(&action, "gesture:test", 0)
            .unwrap();

        let event = event_rx.try_recv().unwrap();
        assert_eq!(event.kind, "host.action");
        assert_eq!(
            event.data,
            json!({
                "kind": "redirect",
                "value": "feature-code",
                "source": "gesture:test"
            })
        );
    }

    #[test]
    fn none_action_does_not_emit_a_fake_trigger_event() {
        let (event_tx, mut event_rx) = broadcast::channel(4);
        let dispatcher = ActionDispatcher::new(event_tx);

        dispatcher
            .dispatch_with_modifiers(&HotzoneAction::default(), "test", 0)
            .unwrap();

        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn volume_delta_scales_smoothly_and_stays_bounded() {
        assert_eq!(scaled_volume_delta("0.02", 2.0).unwrap(), 0.04);
        assert_eq!(scaled_volume_delta("-0.02", 0.5).unwrap(), -0.01);
        assert_eq!(scaled_volume_delta("0.02", 100.0).unwrap(), 0.12);
        assert!(scaled_volume_delta("nope", 1.0).is_err());
        assert!(scaled_volume_delta("0", 1.0).is_err());
    }
}
