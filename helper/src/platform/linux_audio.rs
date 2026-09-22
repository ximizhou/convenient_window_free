use super::adjustment::Level;
use super::command::run;
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

const NORMAL: f32 = 65536.0;

#[derive(Deserialize)]
struct Channel {
    value: u32,
}

#[derive(Deserialize)]
struct Sink {
    index: u32,
    name: String,
    description: String,
    mute: bool,
    channel_map: String,
    volume: HashMap<String, Channel>,
}

fn pactl(args: &[&str]) -> Result<String> {
    run(Command::new("pactl").args(args), Duration::from_secs(2))
        .context("音量控制需要 pactl 和运行中的 PulseAudio 或 PipeWire Pulse 服务")
}

fn sinks() -> Result<Vec<Sink>> {
    Ok(serde_json::from_str(&pactl(&[
        "--format=json",
        "list",
        "sinks",
    ])?)?)
}

impl Sink {
    fn channels(&self) -> Result<Vec<f32>> {
        let values = self
            .channel_map
            .split(',')
            .map(|channel| {
                self.volume
                    .get(channel.trim())
                    .map(|v| v.value as f32 / NORMAL)
                    .context("音频设备返回了不完整的声道音量")
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(!values.is_empty(), "音频设备没有输出声道");
        Ok(values)
    }

    fn level(&self) -> Result<Level> {
        Ok(Level {
            value: self.channels()?.into_iter().fold(0.0, f32::max),
            muted: self.mute,
            device_name: self.description.clone(),
        })
    }
}

pub(crate) fn adjust_system_volume(delta: f32) -> Result<Level> {
    let name = pactl(&["get-default-sink"])?;
    let sink = sinks()?
        .into_iter()
        .find(|s| s.name == name.trim())
        .context("未找到默认音频输出设备")?;
    // Use the numeric sink ID throughout the transaction, even if the default changes.
    let id = sink.index.to_string();
    let channels = sink.channels()?;
    let current = channels.iter().copied().fold(0.0, f32::max);
    let next = (current + delta).clamp(0.0, 1.0);
    let values: Vec<String> = channels
        .iter()
        .map(|value| {
            let value = if current > 0.0 {
                value / current * next
            } else {
                next
            };
            (value * NORMAL).round().to_string()
        })
        .collect();
    let mut args = vec!["set-sink-volume", &id];
    args.extend(values.iter().map(String::as_str));
    pactl(&args)?;
    if delta > 0.0 && sink.mute {
        pactl(&["set-sink-mute", &id, "0"])?;
    }
    sinks()?
        .into_iter()
        .find(|s| s.index == sink.index && s.name == sink.name)
        .context("音频输出设备已断开")?
        .level()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated PulseAudio server with a cw_adjustment_test null sink"]
    fn real_pulse_volume_preserves_balance_and_reports_mute_and_limits() {
        assert_eq!(
            pactl(&["get-default-sink"]).unwrap().trim(),
            "cw_adjustment_test"
        );
        pactl(&["set-sink-volume", "cw_adjustment_test", "32768", "16384"]).unwrap();
        pactl(&["set-sink-mute", "cw_adjustment_test", "1"]).unwrap();
        let down = adjust_system_volume(-0.1).unwrap();
        assert!((down.value - 0.4).abs() < 0.001);
        assert!(down.muted);
        let up = adjust_system_volume(0.2).unwrap();
        assert!((up.value - 0.6).abs() < 0.001);
        assert!(!up.muted);
        let sink = sinks()
            .unwrap()
            .into_iter()
            .find(|s| s.name == "cw_adjustment_test")
            .unwrap();
        let channels = sink.channels().unwrap();
        assert!((channels[1] / channels[0] - 0.5).abs() < 0.001);
        assert_eq!(adjust_system_volume(1.0).unwrap().value, 1.0);
        assert_eq!(adjust_system_volume(-1.0).unwrap().value, 0.0);
        assert!((adjust_system_volume(0.02).unwrap().value - 0.02).abs() < 0.001);
    }

    #[test]
    fn channel_order_follows_the_device_map() {
        let sink: Sink = serde_json::from_value(serde_json::json!({
            "index": 1, "name": "sink", "description": "Speakers", "mute": true,
            "channel_map": "front-left,front-right",
            "volume": { "front-right": { "value": 16384 }, "front-left": { "value": 32768 } }
        }))
        .unwrap();
        assert_eq!(sink.channels().unwrap(), vec![0.5, 0.25]);
        assert_eq!(sink.level().unwrap().value, 0.5);
        assert!(sink.level().unwrap().muted);
        let mut invalid = sink;
        invalid.volume.remove("front-left");
        assert!(invalid.channels().is_err());
    }
}
