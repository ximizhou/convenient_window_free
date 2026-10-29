//! Fixed helper CLI bridge. The caller resolves and validates the bundled payload.
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn failure(code: &str, message: &str) -> Value {
    json!({"ok":false,"version":1,"items":[],"warnings":[],"error":{"code":code,"message":message}})
}

pub fn request(helper_path: PathBuf, request: Value) -> Result<Value, String> {
    if !cfg!(windows) {
        return Ok(failure(
            "unsupported",
            "Startup management is available on Windows",
        ));
    }
    let input = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
    if input.len() > 16 * 1024 {
        return Ok(failure("invalidRequest", "Invalid startup request"));
    }
    // This feature marker only prevents invoking an older helper that ignores the
    // new argument. Payload integrity is checked by the caller independently.
    const CAPABILITY: &[u8] = b"CW_STARTUP_MANAGER_PROTOCOL_V1";
    let compatible = std::fs::read(&helper_path).is_ok_and(|bytes| {
        bytes
            .windows(CAPABILITY.len())
            .any(|part| part == CAPABILITY)
    });
    if !compatible {
        return Ok(failure(
            "needsHelper",
            "Update the helper before managing startup apps",
        ));
    }
    let mut command = Command::new(helper_path);
    command
        .arg("--startup-manager")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return Ok(failure("unavailable", "Could not start startup manager")),
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Startup stdout unavailable".to_owned())?;
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut bytes = vec![];
        let result = stdout
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = tx.send(result);
    });
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Startup stdin unavailable".to_owned())?;
    // Keep the timeout effective even if a broken child never reads its stdin.
    let writer = std::thread::spawn(move || {
        let mut stdin = stdin;
        stdin.write_all(&input)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < Duration::from_secs(15) => {
                std::thread::sleep(Duration::from_millis(15))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = writer.join();
                let _ = reader.join();
                return Ok(failure(
                    "timeout",
                    "Startup operation timed out; refresh to check its result",
                ));
            }
        }
    };
    if writer.join().ok().and_then(Result::ok).is_none() {
        let _ = reader.join();
        return Ok(failure("unavailable", "Could not send startup request"));
    }
    let _ = reader.join();
    let output = match rx.recv().ok().and_then(Result::ok) {
        Some(bytes) if bytes.len() <= 1024 * 1024 => bytes,
        _ => {
            return Ok(failure(
                "invalidResponse",
                "Startup response is too large or unreadable",
            ))
        }
    };
    if !status.success() {
        return Ok(failure(
            "unavailable",
            "Startup manager exited unexpectedly",
        ));
    }
    match serde_json::from_slice::<Value>(&output) {
        Ok(value)
            if value["version"] == 1
                && value["ok"].is_boolean()
                && value["items"].is_array()
                && value["warnings"].is_array() =>
        {
            Ok(value)
        }
        _ => Ok(failure(
            "invalidResponse",
            "The helper does not support this startup manager version",
        )),
    }
}
