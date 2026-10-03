//! Exactly one native listener launches commands for each managed helper.
use serde_json::Value;
use std::net::{Shutdown, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;
use tungstenite::{Message, WebSocket};

pub struct CommandBroker {
    stop: Arc<AtomicBool>,
    stream: TcpStream,
    thread: Option<JoinHandle<()>>,
}

impl CommandBroker {
    pub fn start(
        mut socket: WebSocket<TcpStream>,
        token: String,
        pid: u32,
    ) -> Result<Self, String> {
        let stream = socket
            .get_ref()
            .try_clone()
            .map_err(|error| error.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("desktop-command-broker".into())
            .spawn(move || {
                while !cancelled.load(Ordering::Acquire) {
                    match socket.read() {
                        Ok(Message::Text(text)) => {
                            if cancelled.load(Ordering::Acquire) {
                                break;
                            }
                            if let Some(command) = command_from_message(&text) {
                                if let Err(error) = launch_command(&command) {
                                    eprintln!("Command launch failed: {error}");
                                }
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        Ok(_) => {}
                        Err(tungstenite::Error::Io(error))
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => {
                            if cancelled.load(Ordering::Acquire) {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            if let Ok(connection) = crate::supervisor::ready_socket(&token, pid) {
                                socket = connection;
                            }
                        }
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            stop,
            stream,
            thread: Some(thread),
        })
    }
}

impl Drop for CommandBroker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.stream.shutdown(Shutdown::Both);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn command_from_message(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    if value.get("type")?.as_str()? != "desktop.command" {
        return None;
    }
    let command = value.get("data")?.get("command")?.as_str()?;
    (!command.trim().is_empty() && !command.contains('\0')).then(|| command.to_owned())
}

fn launch_command(command: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        if crate::windows_process::current_elevated()? {
            return Err("Desktop must run with ordinary permissions".into());
        }
        use std::os::windows::process::CommandExt;
        let executable = std::env::var_os("SystemRoot").ok_or("SystemRoot is unavailable")?;
        std::process::Command::new(
            std::path::PathBuf::from(executable)
                .join("System32")
                .join("cmd.exe"),
        )
        .args(["/D", "/S", "/C"])
        .raw_arg(format!("\"{command}\""))
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|error| error.to_string())?;
    }
    #[cfg(not(windows))]
    std::process::Command::new("sh")
        .args(["-c", command])
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn native_listener_runs_commands_with_ordinary_permissions() {
        use std::net::TcpListener;
        use std::time::{Duration, Instant};
        if crate::windows_process::current_elevated().unwrap() {
            assert!(launch_command("exit 0").is_err());
            return;
        }
        let output = std::env::temp_dir().join(format!("cw-command-{}.txt", uuid::Uuid::new_v4()));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let destination = output.to_string_lossy().replace('\'', "''");
        let command = format!("powershell.exe -NoProfile -NonInteractive -Command \"[IO.File]::WriteAllText('{destination}', ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator).ToString())\"");
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            socket
                .send(Message::Text(
                    serde_json::json!({"type":"desktop.command", "data":{"command":command}})
                        .to_string()
                        .into(),
                ))
                .unwrap();
            let _ = socket.read();
        });
        let stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let (socket, _) = tungstenite::client(format!("ws://{address}"), stream).unwrap();
        let broker =
            CommandBroker::start(socket, "unused-test-token".into(), std::process::id()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut result = String::new();
        while Instant::now() < deadline {
            result = std::fs::read_to_string(&output).unwrap_or_default();
            if !result.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        drop(broker);
        server.join().unwrap();
        let _ = std::fs::remove_file(output);
        assert_eq!(result, "False");
    }
    #[test]
    fn only_explicit_command_events_launch_programs() {
        assert_eq!(
            command_from_message(r#"{"type":"desktop.command","data":{"command":"echo hello"}}"#)
                .as_deref(),
            Some("echo hello")
        );
        for message in [
            r#"{"type":"action.triggered","data":{"command":"echo hello"}}"#,
            r#"{"type":"desktop.command","data":{"command":" "}}"#,
            "invalid",
        ] {
            assert!(command_from_message(message).is_none());
        }
    }
}
