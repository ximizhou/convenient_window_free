use anyhow::{bail, Context, Result};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

// Drain both pipes while waiting, so a verbose child cannot block the worker.
pub(super) fn run(command: &mut Command, timeout: Duration) -> Result<String> {
    command
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .process_group(0);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("无法启动 {}", command.get_program().to_string_lossy()))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || read_output(stdout));
    let err = std::thread::spawn(move || read_output(stderr));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            result => {
                unsafe {
                    kill(-(child.id() as i32), 9);
                }
                let _ = child.kill();
                let _ = child.wait();
                break match result {
                    Err(error) => Err(anyhow::Error::from(error)),
                    _ => Err(anyhow::anyhow!("设备控制命令超时")),
                };
            }
        }
    };
    // Descendants must not keep the captured pipes open after the command exits.
    unsafe {
        kill(-(child.id() as i32), 9);
    }
    let stdout = out
        .join()
        .map_err(|_| anyhow::anyhow!("读取命令输出失败"))??;
    let stderr = err
        .join()
        .map_err(|_| anyhow::anyhow!("读取命令错误失败"))??;
    let status = status?;
    if !status.success() {
        bail!(
            "设备控制命令失败 ({status})：{} {}",
            stderr.trim(),
            stdout.trim()
        );
    }
    Ok(stdout)
}

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

fn read_output(mut pipe: impl Read) -> std::io::Result<String> {
    let mut buffer = [0; 4096];
    let mut output = Vec::new();
    loop {
        let length = pipe.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        let keep = length.min(MAX_OUTPUT_BYTES.saturating_sub(output.len()));
        output.extend_from_slice(&buffer[..keep]);
    }
    Ok(String::from_utf8_lossy(&output).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_large_output_and_bounds_diagnostics() {
        let output = run(
            Command::new("sh").args([
                "-c",
                "head -c 1200000 /dev/zero; head -c 1200000 /dev/zero >&2",
            ]),
            Duration::from_secs(3),
        )
        .unwrap();
        assert_eq!(output.len(), MAX_OUTPUT_BYTES);
    }

    #[test]
    fn reports_failure_and_kills_a_stalled_command() {
        assert!(run(
            Command::new("sh").args(["-c", "echo denied >&2; exit 2"]),
            Duration::from_secs(1)
        )
        .unwrap_err()
        .to_string()
        .contains("denied"));
        let start = Instant::now();
        assert!(run(
            Command::new("sh").args(["-c", "sleep 10 & wait"]),
            Duration::from_millis(50)
        )
        .unwrap_err()
        .to_string()
        .contains("超时"));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
