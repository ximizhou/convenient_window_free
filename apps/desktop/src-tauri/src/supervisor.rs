use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::{header::SEC_WEBSOCKET_PROTOCOL, HeaderValue};
use tungstenite::{client, Message};

const HELPER_EXE: &str = "magic-corners-helper.exe";
const HELPER_LOG: &str = "magic-corners-helper.log";
const TOKEN_FILE: &str = "auth-token";
const INSTANCE_CONFLICT: &str = "HELPER_INSTANCE_CONFLICT";
const HELPER_ADDRESS: &str = "127.0.0.1:56873";
const START_TIMEOUT: Duration = Duration::from_secs(6);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResult {
    pub already_running: bool,
    pub data_dir: String,
    pub helper_path: String,
    pub token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopResult {
    pub was_running: bool,
    pub graceful: bool,
}

pub struct HelperProcess {
    child: Option<Child>,
    last_exit_code: Option<i32>,
    last_error: Option<String>,
}

impl Default for HelperProcess {
    fn default() -> Self {
        Self {
            child: None,
            last_exit_code: None,
            last_error: None,
        }
    }
}

impl HelperProcess {
    pub fn start(&mut self, payload_dir: &Path, data_dir: &Path) -> Result<StartResult, String> {
        self.refresh();
        let helper_path = validate_payload(payload_dir)?;
        if self.child.is_some() {
            let token = read_valid_token(&data_dir.join(TOKEN_FILE))?;
            return Ok(StartResult {
                already_running: true,
                data_dir: path_string(data_dir),
                helper_path: path_string(&helper_path),
                token,
            });
        }

        fs::create_dir_all(data_dir)
            .map_err(|error| format!("无法创建 helper 数据目录：{error}"))?;
        let log_path = data_dir.join(HELPER_LOG);
        let log_offset = fs::metadata(&log_path).map(|meta| meta.len()).unwrap_or(0);
        let mut command = Command::new(&helper_path);
        command
            .arg("--data-dir")
            .arg(data_dir)
            .current_dir(payload_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let child = command.spawn().map_err(|error| {
            let message = format!("无法启动后台助手 {}：{error}", helper_path.display());
            self.last_error = Some(message.clone());
            message
        })?;
        self.child = Some(child);
        self.last_error = None;
        self.last_exit_code = None;

        let deadline = Instant::now() + START_TIMEOUT;
        loop {
            self.refresh();
            if self.child.is_none() {
                let new_log = read_log_since(&log_path, log_offset);
                let message = if new_log.contains(INSTANCE_CONFLICT) {
                    "后台助手已由 uTools 或另一便捷窗口实例占用，请先退出占用方后重试".to_string()
                } else {
                    format!(
                        "后台助手启动后立即退出（exit={}）：{}",
                        self.last_exit_code
                            .map(|code| code.to_string())
                            .unwrap_or_else(|| "unknown".to_string()),
                        last_nonempty_line(&new_log).unwrap_or("没有可用日志")
                    )
                };
                self.last_error = Some(message.clone());
                return Err(message);
            }

            if let Ok(token) = read_valid_token(&data_dir.join(TOKEN_FILE)) {
                if helper_is_ready(&token).is_ok() {
                    return Ok(StartResult {
                        already_running: false,
                        data_dir: path_string(data_dir),
                        helper_path: path_string(&helper_path),
                        token,
                    });
                }
            }
            if Instant::now() >= deadline {
                let _ = self.force_kill();
                let detail = last_nonempty_line(&read_log_since(&log_path, log_offset))
                    .unwrap_or("helper 未在超时前开放本地 IPC")
                    .to_string();
                let message = format!("后台助手启动超时：{detail}");
                self.last_error = Some(message.clone());
                return Err(message);
            }
            std::thread::sleep(Duration::from_millis(80));
        }
    }

    pub fn stop(&mut self, data_dir: &Path) -> Result<StopResult, String> {
        self.refresh();
        if self.child.is_none() {
            return Ok(StopResult {
                was_running: false,
                graceful: true,
            });
        }

        let graceful_request = read_valid_token(&data_dir.join(TOKEN_FILE))
            .and_then(|token| request_helper_stop(&token))
            .is_ok();
        let deadline = Instant::now() + STOP_TIMEOUT;
        while Instant::now() < deadline {
            self.refresh();
            if self.child.is_none() {
                return Ok(StopResult {
                    was_running: true,
                    graceful: graceful_request,
                });
            }
            std::thread::sleep(Duration::from_millis(60));
        }

        self.force_kill()?;
        Ok(StopResult {
            was_running: true,
            graceful: false,
        })
    }

    pub fn running(&mut self) -> bool {
        self.refresh();
        self.child.is_some()
    }

    pub fn last_exit_code(&self) -> Option<i32> {
        self.last_exit_code
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.clone()
    }

    fn refresh(&mut self) {
        let exit_status = match self.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(status) => status,
                Err(error) => {
                    self.last_error = Some(format!("无法读取 helper 进程状态：{error}"));
                    None
                }
            },
            None => None,
        };
        if let Some(status) = exit_status {
            self.last_exit_code = status.code();
            self.child = None;
        }
    }

    fn force_kill(&mut self) -> Result<(), String> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        child
            .kill()
            .map_err(|error| format!("无法终止 helper：{error}"))?;
        let status = child
            .wait()
            .map_err(|error| format!("无法等待 helper 退出：{error}"))?;
        self.last_exit_code = status.code();
        Ok(())
    }
}

pub fn validate_payload(payload_dir: &Path) -> Result<PathBuf, String> {
    let helper = payload_dir.join(HELPER_EXE);
    if !helper.is_file() {
        return Err(format!("后台助手文件缺失：{}", helper.display()));
    }
    let unwind = payload_dir.join("libunwind.dll");
    if !unwind.is_file() {
        return Err(format!("后台助手运行库缺失：{}", unwind.display()));
    }
    let has_std = fs::read_dir(payload_dir)
        .map_err(|error| format!("无法检查后台助手目录：{error}"))?
        .filter_map(Result::ok)
        .any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            entry.path().is_file() && name.starts_with("std-") && name.ends_with(".dll")
        });
    if !has_std {
        return Err("后台助手运行库缺失：未找到 std-*.dll".to_string());
    }
    Ok(helper)
}

pub fn payload_size(payload_dir: &Path) -> u64 {
    fs::read_dir(payload_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

pub fn read_valid_token(path: &Path) -> Result<String, String> {
    let token =
        fs::read_to_string(path).map_err(|error| format!("无法读取 helper 令牌：{error}"))?;
    let token = token.trim();
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("helper 令牌格式无效".to_string());
    }
    Ok(token.to_string())
}

pub fn log_tail(path: &Path, max_lines: usize) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines = content.lines().collect::<Vec<_>>();
    lines[lines.len().saturating_sub(max_lines)..]
        .iter()
        .map(|line| line.to_string())
        .collect()
}

fn helper_is_ready(token: &str) -> Result<(), String> {
    let mut socket = connect_authenticated(token)?;
    let message = socket
        .read()
        .map_err(|error| format!("helper 就绪消息读取失败：{error}"))?;
    let value: Value = serde_json::from_str(
        message
            .to_text()
            .map_err(|error| format!("helper 就绪消息不是文本：{error}"))?,
    )
    .map_err(|error| format!("helper 就绪消息无效：{error}"))?;
    if value.get("type").and_then(Value::as_str) != Some("helper.ready") {
        return Err("helper 未返回就绪消息".to_string());
    }
    let _ = socket.close(None);
    Ok(())
}

fn request_helper_stop(token: &str) -> Result<(), String> {
    let mut socket = connect_authenticated(token)?;
    let _ = socket
        .read()
        .map_err(|error| format!("helper 就绪消息读取失败：{error}"))?;
    let message = serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "type": "helper.stop",
        "time": timestamp_ms(),
        "data": {}
    });
    socket
        .send(Message::Text(message.to_string().into()))
        .map_err(|error| format!("helper 停止请求发送失败：{error}"))?;
    let _ = socket.close(None);
    Ok(())
}

fn connect_authenticated(token: &str) -> Result<tungstenite::WebSocket<TcpStream>, String> {
    let address: SocketAddr = HELPER_ADDRESS
        .parse()
        .map_err(|error| format!("helper 地址无效：{error}"))?;
    let stream = TcpStream::connect_timeout(&address, Duration::from_millis(350))
        .map_err(|error| format!("helper IPC 连接失败：{error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(750)))
        .map_err(|error| format!("无法设置 helper 读取超时：{error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_millis(750)))
        .map_err(|error| format!("无法设置 helper 写入超时：{error}"))?;
    let mut request = format!("ws://{HELPER_ADDRESS}")
        .into_client_request()
        .map_err(|error| format!("helper IPC 请求无效：{error}"))?;
    request.headers_mut().insert(
        SEC_WEBSOCKET_PROTOCOL,
        HeaderValue::from_str(token).map_err(|error| format!("helper 令牌无效：{error}"))?,
    );
    client(request, stream)
        .map(|(socket, _)| socket)
        .map_err(|error| format!("helper IPC 握手失败：{error}"))
}

fn read_log_since(path: &Path, offset: u64) -> String {
    let Ok(bytes) = fs::read(path) else {
        return String::new();
    };
    let start = if bytes.len() as u64 >= offset {
        offset as usize
    } else {
        0
    };
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

fn last_nonempty_line(content: &str) -> Option<&str> {
    content.lines().rev().find(|line| !line.trim().is_empty())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "convenient-window-payload-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn sidecar_requires_executable_and_every_gnu_runtime_class() {
        let directory = test_dir();
        fs::write(directory.join(HELPER_EXE), b"exe").unwrap();
        assert!(validate_payload(&directory)
            .unwrap_err()
            .contains("libunwind"));
        fs::write(directory.join("libunwind.dll"), b"dll").unwrap();
        assert!(validate_payload(&directory)
            .unwrap_err()
            .contains("std-*.dll"));
        fs::write(directory.join("std-test.dll"), b"dll").unwrap();
        assert_eq!(
            validate_payload(&directory).unwrap(),
            directory.join(HELPER_EXE)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn authentication_token_is_not_accepted_unless_complete() {
        let directory = test_dir();
        let path = directory.join(TOKEN_FILE);
        fs::write(&path, "a".repeat(63)).unwrap();
        assert!(read_valid_token(&path).is_err());
        fs::write(&path, "a".repeat(64)).unwrap();
        assert_eq!(read_valid_token(&path).unwrap(), "a".repeat(64));
        fs::remove_dir_all(directory).unwrap();
    }
}
