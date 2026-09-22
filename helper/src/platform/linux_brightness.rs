use crate::platform::adjustment::Level;
use crate::platform::brightness::adjusted_brightness;
use crate::platform::command::run;
use crate::platform::Monitor;
use anyhow::{bail, ensure, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn adjust(monitor: &Monitor, delta: f32) -> Result<Level> {
    let (name, edid) = output_identity(monitor)?;
    let connector = find_connector(Path::new("/sys/class/drm"), &edid)?;
    let internal = is_internal(&name);
    if internal {
        let device = find_backlight(Path::new("/sys/class/backlight"), &connector)?;
        let current = read_number(&device.join("brightness"))?;
        let max = read_number(&device.join("max_brightness"))?;
        let next = adjusted_brightness(0, current, max, delta)?;
        if next != current {
            verify_connector(&connector, &edid)?;
            let name = device
                .file_name()
                .and_then(|name| name.to_str())
                .context("无效的背光设备名")?;
            run(
                Command::new("busctl").args([
                    "--system",
                    "--timeout=3s",
                    "call",
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1/session/auto",
                    "org.freedesktop.login1.Session",
                    "SetBrightness",
                    "ssu",
                    "backlight",
                    name,
                    &next.to_string(),
                ]),
                COMMAND_TIMEOUT,
            )
            .context("无法设置内屏背光，请检查 logind 会话与设备权限")?;
        }
        Level::brightness(0, read_number(&device.join("brightness"))?, max, name)
    } else {
        let bus = i2c_bus(&connector)?;
        let output = run(
            Command::new("ddcutil").args(["--bus", &bus, "--terse", "getvcp", "10"]),
            COMMAND_TIMEOUT,
        )
        .context("外屏需要 ddcutil、I²C 访问权限及 DDC/CI 支持")?;
        let (current, max) = parse_vcp(&output)?;
        let next = adjusted_brightness(0, current, max, delta)?;
        if next != current {
            verify_connector(&connector, &edid)?;
            ensure!(i2c_bus(&connector)? == bus, "显示器连接已变更");
            run(
                Command::new("ddcutil").args(["--bus", &bus, "setvcp", "10", &next.to_string()]),
                COMMAND_TIMEOUT,
            )?;
        }
        let (current, max) = parse_vcp(&run(
            Command::new("ddcutil").args(["--bus", &bus, "--terse", "getvcp", "10"]),
            COMMAND_TIMEOUT,
        )?)?;
        Level::brightness(0, current, max, name)
    }
}

fn output_identity(monitor: &Monitor) -> Result<(String, Vec<u8>)> {
    let (connection, screen) = x11rb::connect(None)?;
    let root = connection.setup().roots[screen].root;
    let resources = connection
        .randr_get_screen_resources_current(root)?
        .reply()?;
    let length = monitor
        .device_id
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(128);
    let expected = String::from_utf16_lossy(&monitor.device_id[..length]);
    let edid_atom = connection.intern_atom(true, b"EDID")?.reply()?.atom;
    ensure!(edid_atom != 0, "显示器未提供 EDID，无法确定亮度设备");
    for output in resources.outputs {
        let info = connection
            .randr_get_output_info(output, resources.config_timestamp)?
            .reply()?;
        if info.name != expected.as_bytes() || info.crtc == 0 {
            continue;
        }
        let crtc = connection
            .randr_get_crtc_info(info.crtc, resources.config_timestamp)?
            .reply()?;
        ensure!(
            crtc.outputs.len() == 1,
            "镜像显示包含多个输出，无法唯一确定亮度设备"
        );
        ensure!(crtc.mode != 0, "目标显示器已断开");
        let property = connection
            .randr_get_output_property(output, edid_atom, AtomEnum::ANY, 0, 1024, false, false)?
            .reply()?;
        ensure!(
            property.format == 8 && valid_edid(&property.data),
            "显示器 EDID 无效"
        );
        return Ok((expected, property.data));
    }
    bail!("目标显示器已断开")
}

fn valid_edid(edid: &[u8]) -> bool {
    edid.len() >= 128
        && edid.len() % 128 == 0
        && edid.starts_with(&[0, 255, 255, 255, 255, 255, 255, 0])
        && edid.chunks_exact(128).all(|block| {
            block
                .iter()
                .fold(0u8, |sum, value| sum.wrapping_add(*value))
                == 0
        })
}

fn find_connector(root: &Path, edid: &[u8]) -> Result<PathBuf> {
    ensure!(valid_edid(edid), "显示器 EDID 无效");
    let candidates: Vec<_> = fs::read_dir(root)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            fs::read_to_string(path.join("status")).is_ok_and(|status| status.trim() == "connected")
        })
        .filter(|path| fs::read(path.join("edid")).is_ok_and(|value| value == edid))
        .collect();
    ensure!(candidates.len() == 1, "无法唯一匹配显示器与 DRM 接口");
    Ok(candidates[0].clone())
}

fn verify_connector(connector: &Path, edid: &[u8]) -> Result<()> {
    ensure!(
        fs::read_to_string(connector.join("status"))?.trim() == "connected"
            && fs::read(connector.join("edid"))? == edid,
        "目标显示器已断开或变更"
    );
    Ok(())
}

fn is_internal(name: &str) -> bool {
    name.starts_with("eDP") || name.starts_with("LVDS") || name.starts_with("DSI")
}

fn find_backlight(root: &Path, connector: &Path) -> Result<PathBuf> {
    let physical = fs::canonicalize(connector)?;
    let mut candidates = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let Ok(device) = fs::canonicalize(path.join("device")) else {
            continue;
        };
        // Drivers expose the backlight below either the connector or its GPU.
        if physical.starts_with(&device) || device.starts_with(&physical) {
            candidates.push(path);
        }
    }
    ensure!(candidates.len() == 1, "无法唯一匹配内屏背光设备");
    // A GPU-wide backlight cannot distinguish two internal panels.
    let siblings = connector.parent().context("无效的 DRM 接口")?;
    let card = connector
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split_once('-'))
        .context("无效的 DRM 接口名")?
        .0;
    let internal_count = fs::read_dir(siblings)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let name = entry.file_name();
            let Some((prefix, suffix)) = name.to_str().and_then(|name| name.split_once('-')) else {
                return false;
            };
            prefix == card
                && is_internal(suffix)
                && fs::read_to_string(entry.path().join("status"))
                    .is_ok_and(|status| status.trim() == "connected")
        })
        .count();
    ensure!(
        internal_count == 1,
        "同一显卡连接多个内屏，无法唯一匹配背光设备"
    );
    Ok(candidates.remove(0))
}

fn i2c_bus(connector: &Path) -> Result<String> {
    let path = fs::canonicalize(connector.join("ddc")).context("显示器未提供 DDC I²C 通道")?;
    let bus = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("i2c-"))
        .context("无效的 I²C 通道")?;
    ensure!(
        !bus.is_empty() && bus.bytes().all(|byte| byte.is_ascii_digit()),
        "无效的 I²C 通道"
    );
    Ok(bus.to_string())
}

fn read_number(path: &Path) -> Result<u32> {
    fs::read_to_string(path)?
        .trim()
        .parse()
        .with_context(|| format!("读取背光数值失败：{}", path.display()))
}

fn parse_vcp(output: &str) -> Result<(u32, u32)> {
    let rows: Vec<_> = output
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .filter(|row| row.first() == Some(&"VCP"))
        .collect();
    ensure!(rows.len() == 1, "ddcutil 未返回唯一的亮度读数");
    let row = &rows[0];
    ensure!(
        row.len() == 5 && row[1].eq_ignore_ascii_case("10") && row[2] == "C",
        "显示器未提供连续亮度控制"
    );
    let current = row[3].parse()?;
    let max = row[4].parse()?;
    ensure!(max > 0 && current <= max, "显示器返回无效亮度范围");
    Ok((current, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn ddc_values_keep_the_device_range_and_reject_invalid_replies() {
        assert_eq!(parse_vcp("VCP 10 C 128 255\n").unwrap(), (128, 255));
        for output in [
            "VCP 10 ERR",
            "VCP 10 NC 1 100",
            "VCP 12 C 50 100",
            "VCP 10 C 101 100",
            "VCP 10 C 0 0",
            "VCP 10 C 1 100\nVCP 10 C 2 100",
        ] {
            assert!(parse_vcp(output).is_err(), "{output}");
        }
    }

    #[test]
    fn connector_matching_rejects_duplicates_and_disconnected_monitors() {
        let root = std::env::temp_dir().join(format!("brightness-drm-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let mut edid = vec![0; 128];
        edid[..8].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 0]);
        edid[127] = 6;
        assert!(valid_edid(&edid));
        let first = root.join("card0-DP-1");
        let second = root.join("card0-DP-2");
        for connector in [&first, &second] {
            fs::create_dir(connector).unwrap();
            fs::write(connector.join("edid"), &edid).unwrap();
            fs::write(connector.join("status"), "connected\n").unwrap();
        }
        assert!(find_connector(&root, &edid).is_err());
        fs::write(second.join("status"), "disconnected\n").unwrap();
        assert_eq!(find_connector(&root, &edid).unwrap(), first);
        fs::write(first.join("status"), "disconnected\n").unwrap();
        assert!(verify_connector(&first, &edid).is_err());
        assert!(find_connector(&root, &edid).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backlights_must_belong_to_the_target_gpu_and_a_single_internal_panel() {
        let root =
            std::env::temp_dir().join(format!("brightness-backlight-{}", uuid::Uuid::new_v4()));
        let drm = root.join("drm");
        let backlights = root.join("backlight");
        let gpu = root.join("devices/gpu0");
        let panel = gpu.join("card0-eDP-1");
        fs::create_dir_all(&panel).unwrap();
        fs::create_dir_all(&drm).unwrap();
        fs::create_dir_all(backlights.join("intel_backlight")).unwrap();
        fs::write(panel.join("status"), "connected\n").unwrap();
        symlink(&panel, drm.join("card0-eDP-1")).unwrap();
        symlink(&gpu, backlights.join("intel_backlight/device")).unwrap();
        assert_eq!(
            find_backlight(&backlights, &drm.join("card0-eDP-1")).unwrap(),
            backlights.join("intel_backlight")
        );

        let second = gpu.join("card0-eDP-2");
        fs::create_dir_all(&second).unwrap();
        fs::write(second.join("status"), "connected\n").unwrap();
        symlink(&second, drm.join("card0-eDP-2")).unwrap();
        assert!(find_backlight(&backlights, &drm.join("card0-eDP-1")).is_err());
        fs::write(second.join("status"), "disconnected\n").unwrap();
        fs::create_dir_all(backlights.join("duplicate")).unwrap();
        symlink(&gpu, backlights.join("duplicate/device")).unwrap();
        assert!(find_backlight(&backlights, &drm.join("card0-eDP-1")).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
