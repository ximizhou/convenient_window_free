use crate::config::SCHEMA_VERSION;
use crate::core::window_layout;
use crate::ipc::errors::{ErrorCode, RuntimeError};
use crate::platform::WindowLayout;

pub const PROTOCOL_VERSION: u32 = 7;
use crate::config::AppConfig;
use crate::ipc::messages::HelperMessage;
use crate::logging;
use crate::usage::UsageTracker;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch, Mutex as AsyncMutex};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{ErrorResponse, Request, Response},
        http::{header::SEC_WEBSOCKET_PROTOCOL, HeaderValue, StatusCode},
        Message,
    },
};

pub struct WebSocketServer {
    addr: String,
    auth_token: String,
    config_tx: watch::Sender<AppConfig>,
    config_update_lock: Arc<AsyncMutex<()>>,
    event_tx: broadcast::Sender<HelperMessage>,
    shutdown_tx: broadcast::Sender<()>,
    usage: UsageTracker,
}

impl WebSocketServer {
    pub fn new(
        addr: impl Into<String>,
        auth_token: impl Into<String>,
        config_tx: watch::Sender<AppConfig>,
        event_tx: broadcast::Sender<HelperMessage>,
        shutdown_tx: broadcast::Sender<()>,
        usage: UsageTracker,
    ) -> Self {
        Self {
            addr: addr.into(),
            auth_token: auth_token.into(),
            config_tx,
            config_update_lock: Arc::new(AsyncMutex::new(())),
            event_tx,
            shutdown_tx,
            usage,
        }
    }

    pub async fn run(&self) -> Result<()> {
        logging::write_line(format!("websocket: binding {}", self.addr));
        let listener = bind_listener(&self.addr)?;
        logging::write_line(format!("websocket: listening {}", self.addr));
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                incoming = listener.accept() => {
                    let (stream, addr) = match incoming {
                        Ok(connection) => connection,
                        Err(error) => {
                            logging::write_line(format!("websocket: accept error {error}"));
                            continue;
                        }
                    };
                    if !addr.ip().is_loopback() {
                        continue;
                    }

                    let config_tx = self.config_tx.clone();
                    let config_update_lock = Arc::clone(&self.config_update_lock);
                    let auth_token = self.auth_token.clone();
                    let shutdown_tx = self.shutdown_tx.clone();
                    let usage = self.usage.clone();
                    let event_rx = self.event_tx.subscribe();
                    tokio::spawn(async move {
                        logging::write_line("websocket: accepted client");
                        if let Err(error) = handle_connection(stream, &auth_token, config_tx, config_update_lock, event_rx, shutdown_tx, usage).await {
                            logging::write_line(format!("websocket: connection error {error:#}"));
                        }
                    });
                }
                _ = shutdown_rx.recv() => break,
            }
        }

        Ok(())
    }
}

fn bind_listener(addr: &str) -> Result<TcpListener> {
    let addr: SocketAddr = addr.parse()?;
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    // After a clean stop, the server side of accepted connections lingers in
    // TIME_WAIT on the fixed port; on Linux that blocks rebinding unless
    // SO_REUSEADDR is set. Windows bind semantics do not block on the
    // TIME_WAIT of closed connections, and it must not get SO_REUSEADDR
    // because there it allows port hijacking.
    #[cfg(unix)]
    socket.set_reuse_address(true)?;
    socket.bind(&addr.into())?;
    socket.listen(128)?;
    socket.set_nonblocking(true)?;
    Ok(TcpListener::from_std(socket.into())?)
}

async fn handle_connection(
    stream: TcpStream,
    auth_token: &str,
    config_tx: watch::Sender<AppConfig>,
    config_update_lock: Arc<AsyncMutex<()>>,
    mut event_rx: broadcast::Receiver<HelperMessage>,
    shutdown_tx: broadcast::Sender<()>,
    usage: UsageTracker,
) -> Result<()> {
    let expected_token = auth_token.to_string();
    let socket = accept_hdr_async(stream, move |request: &Request, mut response: Response| {
        let authorized = request
            .headers()
            .get(SEC_WEBSOCKET_PROTOCOL)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|protocols| {
                protocols
                    .split(',')
                    .map(str::trim)
                    .any(|protocol| constant_time_eq(protocol, &expected_token))
            });

        if !authorized {
            return Err(unauthorized_response());
        }

        response.headers_mut().insert(
            SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_str(&expected_token).expect("generated auth token is a valid header"),
        );
        Ok(response)
    })
    .await?;
    let ocr_languages =
        tokio::task::spawn_blocking(|| crate::platform::available_ocr_languages().to_vec())
            .await
            .unwrap_or_default();
    let (mut writer, mut reader) = socket.split();
    let recording_connection = RecordingConnection(uuid::Uuid::new_v4().to_string());
    let mut recording_tick = tokio::time::interval(std::time::Duration::from_millis(16));
    recording_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    writer
        .send(Message::Text(
            serde_json::to_string(&HelperMessage::new(
                "helper.ready",
                serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "processId": std::process::id(),
                    "elevated": crate::desktop_owner::elevated(),
                    "desktopManaged": crate::desktop_owner::managed(),
                    "protocolVersion": PROTOCOL_VERSION,
                    "schemaVersion": SCHEMA_VERSION,
                    "platform": crate::platform::platform_info(),
                    "shortcutRecording": crate::platform::shortcut_recording::supported(),
                    "ocrLanguages": ocr_languages,
                    "usage": usage.snapshot()
                }),
            ))?
            .into(),
        ))
        .await?;

    loop {
        tokio::select! {
            _ = recording_tick.tick(), if crate::platform::shortcut_recording::has_work(&recording_connection.0)
                || crate::platform::shortcut_hotkeys::has_work(&recording_connection.0) => {
                while let Some(event) = crate::platform::shortcut_recording::take_event(&recording_connection.0) {
                    let message = HelperMessage::new("shortcut.recording", serde_json::to_value(event)?);
                    writer.send(Message::Text(serde_json::to_string(&message)?.into())).await?;
                }
                while let Some(event) = crate::platform::shortcut_hotkeys::take_event(&recording_connection.0) {
                    let message = HelperMessage::new("shortcut.hotkey.triggered", serde_json::to_value(event)?);
                    writer.send(Message::Text(serde_json::to_string(&message)?.into())).await?;
                }
            }
            incoming = reader.next() => {
                let Some(message) = incoming else {
                    break;
                };
                let message = message?;
                if !message.is_text() {
                    continue;
                }

                let text = message.to_text()?;
                let incoming: HelperMessage = match serde_json::from_str(text) {
                    Ok(m) => m,
                    Err(e) => {
                        logging::write_line(format!("websocket: parse error {e}"));
                        continue;
                    }
                };
                logging::write_line(format!(
                    "websocket: recv type={} bytes={}",
                    incoming.kind,
                    text.len()
                ));
                match incoming.kind.as_str() {
                    "window.layout.capture" => {
                        match window_layout::capture().await {
                            Ok(windows) => {
                                let captured = HelperMessage::new(
                                    "window.layout.captured",
                                    serde_json::json!({
                                        "requestId": incoming.id,
                                        "capturedAt": crate::ipc::messages::timestamp_ms(),
                                        "windows": windows,
                                    }),
                                );
                                writer.send(Message::Text(serde_json::to_string(&captured)?.into())).await?;
                            }
                            Err(error) => {
                                let error = HelperMessage::new(
                                    "runtime.error",
                                    RuntimeError::new(ErrorCode::WindowOperationFailed, format!("{error:#}"))
                                        .for_request(&incoming.id)
                                        .data(),
                                );
                                writer.send(Message::Text(serde_json::to_string(&error)?.into())).await?;
                            }
                        }
                    }
                    "window.layout.restore" => {
                        let layouts = match parse_window_layouts(&incoming.data) {
                            Ok(layouts) => layouts,
                            Err(error) => {
                                let error = HelperMessage::new(
                                    "runtime.error",
                                    error.for_request(&incoming.id).data(),
                                );
                                writer.send(Message::Text(serde_json::to_string(&error)?.into())).await?;
                                continue;
                            }
                        };
                        match window_layout::restore(layouts).await {
                            Ok(summary) => {
                                let restored = HelperMessage::new(
                                    "window.layout.restored",
                                    serde_json::json!({
                                        "requestId": incoming.id,
                                        "restored": summary.restored,
                                        "skipped": summary.skipped,
                                        "errors": summary.errors,
                                    }),
                                );
                                writer.send(Message::Text(serde_json::to_string(&restored)?.into())).await?;
                            }
                            Err(error) => {
                                let error = HelperMessage::new(
                                    "runtime.error",
                                    RuntimeError::new(ErrorCode::WindowOperationFailed, format!("{error:#}"))
                                        .for_request(&incoming.id)
                                        .data(),
                                );
                                writer.send(Message::Text(serde_json::to_string(&error)?.into())).await?;
                            }
                        }
                    }
                    "shortcut.record.begin" => {
                        let id = incoming.data.get("sessionId").and_then(Value::as_str).unwrap_or("");
                        if let Err(error) = crate::platform::shortcut_recording::begin(&recording_connection.0, id) {
                            let message = HelperMessage::new("shortcut.recording", serde_json::json!({
                                "sessionId": id, "phase": "cancelled", "reason": "unavailable", "requestId": incoming.id
                            }));
                            logging::write_line(format!("shortcut recording unavailable: {error}"));
                            writer.send(Message::Text(serde_json::to_string(&message)?.into())).await?;
                        }
                    }
                    "shortcut.record.cancel" => {
                        if let Some(id) = incoming.data.get("sessionId").and_then(Value::as_str) {
                            crate::platform::shortcut_recording::cancel(&recording_connection.0, Some(id));
                        }
                    }
                    "shortcut.hotkeys.sync" => {
                        let hotkeys = match parse_hotkeys(&incoming.data) {
                            Ok(hotkeys) => hotkeys,
                            Err(error) => {
                                let error = HelperMessage::new(
                                    "runtime.error",
                                    RuntimeError::new(ErrorCode::WindowOperationFailed, format!("{error:#}"))
                                        .for_request(&incoming.id)
                                        .data(),
                                );
                                writer.send(Message::Text(serde_json::to_string(&error)?.into())).await?;
                                continue;
                            }
                        };
                        let rejected = crate::platform::shortcut_hotkeys::sync(&recording_connection.0, &hotkeys);
                        let accepted = hotkeys.len().saturating_sub(rejected.len());
                        let synced = HelperMessage::new(
                            "shortcut.hotkeys.synced",
                            serde_json::json!({
                                "requestId": incoming.id,
                                "accepted": accepted,
                                "rejected": rejected,
                            }),
                        );
                        writer.send(Message::Text(serde_json::to_string(&synced)?.into())).await?;
                    }
                    "config.update" => {
                        let (config, revision, adjusted) = match parse_config_update(incoming.data) {
                            Ok(update) => update,
                            Err(e) => {
                                logging::write_line(format!("websocket: config deserialize error {e}"));
                                let error = HelperMessage::new("runtime.error", e.for_request(&incoming.id).data());
                                writer.send(Message::Text(serde_json::to_string(&error)?.into())).await?;
                                continue;
                            }
                        };
                        if let Err(error) = apply_config_update(&config_tx, &config_update_lock, config).await {
                            logging::write_line(format!("websocket: config save error {error:#}"));
                            let error = HelperMessage::new("runtime.error", RuntimeError::new(ErrorCode::ConfigSaveFailed, format!("{error:#}")).for_request(&incoming.id).data());
                            writer.send(Message::Text(serde_json::to_string(&error)?.into())).await?;
                            continue;
                        }
                        let applied = HelperMessage::new("config.applied", serde_json::json!({
                            "requestId": incoming.id,
                            "revision": revision,
                            "adjusted": adjusted
                        }));
                        writer.send(Message::Text(serde_json::to_string(&applied)?.into())).await?;
                    }
                    "helper.ping" => {
                        let pong = HelperMessage {
                            id: incoming.id,
                            kind: "helper.pong".to_string(),
                            time: crate::ipc::messages::timestamp_ms(),
                            data: Value::Null,
                        };
                        writer.send(Message::Text(serde_json::to_string(&pong)?.into())).await?;
                    }
                    "usage.get" => {
                        writer
                            .send(Message::Text(
                                serde_json::to_string(&usage.message())?.into(),
                            ))
                            .await?;
                    }
                    "helper.stop" => {
                        writer.send(Message::Close(None)).await?;
                        let _ = shutdown_tx.send(());
                        break;
                    }
                    _ => {
                        let error = HelperMessage::new("runtime.error", RuntimeError::new(ErrorCode::UnknownMessage, incoming.kind).for_request(&incoming.id).data());
                        writer.send(Message::Text(serde_json::to_string(&error)?.into())).await?;
                    }
                }
            }
            event = event_rx.recv() => {
                match event {
                    Ok(event) => {
                        writer.send(Message::Text(serde_json::to_string(&event)?.into())).await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    Ok(())
}

struct RecordingConnection(String);
impl Drop for RecordingConnection {
    fn drop(&mut self) {
        crate::platform::shortcut_recording::cancel(&self.0, None);
        // 连接断开后必须注销该连接注册的布局全局热键，避免快捷键残留在系统里。
        crate::platform::shortcut_hotkeys::clear(Some(&self.0));
    }
}

/// 解析 `shortcut.hotkeys.sync` 的 `{ hotkeys: [{ id, accelerator }] }` 数据。
/// 单条非法 accelerator 交给注册表逐条拒绝，整包结构非法才返回错误。
fn parse_hotkeys(data: &Value) -> Result<Vec<(String, String)>> {
    const MAX_SYNC_HOTKEYS: usize = 64;
    let raw = data.get("hotkeys").cloned().unwrap_or(Value::Null);
    let items = raw
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("hotkeys must be an array"))?;
    if items.len() > MAX_SYNC_HOTKEYS {
        anyhow::bail!("hotkey sync contains too many entries (maximum {MAX_SYNC_HOTKEYS})");
    }
    let mut hotkeys = Vec::with_capacity(items.len());
    for item in items {
        let id = item.get("id").and_then(Value::as_str).unwrap_or("").trim();
        let accelerator = item
            .get("accelerator")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if id.is_empty() {
            anyhow::bail!("hotkey entry id must not be empty");
        }
        hotkeys.push((id.to_owned(), accelerator.to_owned()));
    }
    Ok(hotkeys)
}

fn parse_window_layouts(data: &Value) -> Result<Vec<WindowLayout>, RuntimeError> {
    const MAX_LAYOUT_WINDOWS: usize = 512;
    let raw = data.get("windows").cloned().unwrap_or(Value::Null);
    let layouts: Vec<WindowLayout> = serde_json::from_value(raw).map_err(|error| {
        RuntimeError::new(
            ErrorCode::WindowOperationFailed,
            format!("invalid window layout payload: {error}"),
        )
    })?;
    if layouts.len() > MAX_LAYOUT_WINDOWS {
        return Err(RuntimeError::new(
            ErrorCode::WindowOperationFailed,
            format!("window layout contains too many entries (maximum {MAX_LAYOUT_WINDOWS})"),
        ));
    }
    Ok(layouts)
}

async fn apply_config_update(
    config_tx: &watch::Sender<AppConfig>,
    update_lock: &AsyncMutex<()>,
    config: AppConfig,
) -> Result<()> {
    apply_config_update_with(config_tx, update_lock, config, |persisted| {
        crate::save_config(&persisted)
    })
    .await
}

async fn apply_config_update_with<P>(
    config_tx: &watch::Sender<AppConfig>,
    update_lock: &AsyncMutex<()>,
    config: AppConfig,
    persist: P,
) -> Result<()>
where
    P: FnOnce(AppConfig) -> Result<()> + Send + 'static,
{
    let _guard = update_lock.lock().await;
    let persisted = config.clone();
    tokio::task::spawn_blocking(move || persist(persisted))
        .await
        .context("config persistence task failed")??;
    config_tx.send_replace(config);
    Ok(())
}

fn parse_config_update(data: Value) -> Result<(AppConfig, Option<u64>, bool), RuntimeError> {
    if data.get("protocolVersion").and_then(Value::as_u64) != Some(PROTOCOL_VERSION as u64)
        || data
            .get("config")
            .and_then(|v| v.get("schemaVersion"))
            .and_then(Value::as_u64)
            != Some(SCHEMA_VERSION as u64)
    {
        return Err(RuntimeError::new(
            ErrorCode::UnsupportedSchema,
            "Expected protocol 7 and settings schema 9",
        ));
    }
    let revision = data.get("revision").and_then(Value::as_u64);
    let raw_config = data.get("config").cloned().unwrap_or(Value::Null);
    let invalid =
        |error: serde_json::Error| RuntimeError::new(ErrorCode::InvalidConfig, error.to_string());
    let received = serde_json::from_value::<AppConfig>(raw_config).map_err(invalid)?;
    let before = serde_json::to_value(&received).map_err(invalid)?;
    let mut config = received.normalized();
    crate::platform::apply_capability_limits(&mut config);
    let adjusted = serde_json::to_value(&config).map_err(invalid)? != before;
    if let Some(labels) = data.get("gestureLabels").and_then(Value::as_object) {
        for gesture in &config.mouse_gestures.gestures {
            if let Some(label) = labels.get(&gesture.id).and_then(Value::as_str) {
                let label: String = label
                    .trim()
                    .chars()
                    .filter(|ch| !ch.is_control())
                    .take(80)
                    .collect();
                if !label.is_empty() {
                    config.gesture_labels.insert(gesture.id.clone(), label);
                }
            }
        }
    }
    Ok((config, revision, adjusted))
}

fn unauthorized_response() -> ErrorResponse {
    tokio_tungstenite::tungstenite::http::Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .body(Some("authentication required".to_string()))
        .expect("static unauthorized response is valid")
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.bytes()
        .zip(right.bytes())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Edge;
    use crate::core::edge_hide::CollapsedWindow;
    use crate::core::window_layout::{layouts_from_layout_targets, layouts_from_windows};
    use crate::platform::EdgeHiddenLayout;
    use crate::platform::WindowInfo;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn incompatible_config_is_rejected_before_normalization() {
        for data in [
            serde_json::json!({"schemaVersion": 7}),
            serde_json::json!({"config": {"schemaVersion": 9}}),
            serde_json::json!({"protocolVersion": 6, "config": {"schemaVersion": 9}}),
            serde_json::json!({"protocolVersion": 7, "config": {"schemaVersion": 7}}),
            serde_json::json!({"protocolVersion": 7, "config": {"schemaVersion": 8}}),
            serde_json::json!({"protocolVersion": 7, "config": {"schemaVersion": 10}}),
        ] {
            assert_eq!(
                parse_config_update(data).unwrap_err().code,
                ErrorCode::UnsupportedSchema
            );
        }
    }

    #[test]
    fn translated_labels_are_bounded_and_never_persisted() {
        let (config, _, _) = parse_config_update(serde_json::json!({
            "protocolVersion": 7,
            "config": {"schemaVersion": 9},
            "gestureLabels": {"gesture-up": " Up · Copy\n", "unknown": "ignored"}
        }))
        .unwrap();
        assert_eq!(config.gesture_labels.len(), 1);
        assert_eq!(
            config.gesture_label(&config.mouse_gestures.gestures[0]),
            Some("Up · Copy")
        );
        let serialized = serde_json::to_value(&config).unwrap();
        assert!(serialized.get("gestureLabels").is_none());
        assert!(serialized["mouseGestures"]["gestures"][0]
            .get("name")
            .is_none());
    }

    #[test]
    fn token_comparison_rejects_mismatches() {
        assert!(constant_time_eq("abc123", "abc123"));
        assert!(!constant_time_eq("abc123", "abc124"));
        assert!(!constant_time_eq("abc123", "abc1234"));
    }

    #[test]
    fn config_update_envelope_keeps_revision_and_reports_normalization() {
        let (config, revision, adjusted) = parse_config_update(serde_json::json!({
            "protocolVersion": 7,
            "revision": 7,
            "config": { "schemaVersion": 9, "enabled": true, "edgeSize": 999 }
        }))
        .unwrap();

        assert_eq!(revision, Some(7));
        assert_eq!(config.edge_size, 48);
        assert!(adjusted);

        let mut normalized_config = AppConfig::default().normalized();
        crate::platform::apply_capability_limits(&mut normalized_config);
        let normalized = serde_json::to_value(normalized_config).unwrap();
        let (_, revision, adjusted) = parse_config_update(serde_json::json!({
            "protocolVersion": 7,
            "revision": 8,
            "config": normalized
        }))
        .unwrap();
        assert_eq!(revision, Some(8));
        assert!(!adjusted);
    }

    #[test]
    fn layout_capture_assigns_occurrence_per_stable_identity() {
        let windows = vec![
            WindowInfo {
                handle: crate::platform::WindowHandle(1),
                rect: crate::platform::Rect {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100,
                },
                title: "Editor".into(),
                class_name: "Code".into(),
                process_name: "code.exe".into(),
                maximized: false,
                transient: false,
                arranged: false,
                topmost: false,
            },
            WindowInfo {
                handle: crate::platform::WindowHandle(2),
                rect: crate::platform::Rect {
                    left: 100,
                    top: 0,
                    right: 200,
                    bottom: 100,
                },
                title: "Editor".into(),
                class_name: "Code".into(),
                process_name: "CODE.EXE".into(),
                maximized: true,
                transient: false,
                arranged: false,
                topmost: true,
            },
            WindowInfo {
                handle: crate::platform::WindowHandle(3),
                rect: crate::platform::Rect {
                    left: 0,
                    top: 0,
                    right: 10,
                    bottom: 10,
                },
                title: "Menu".into(),
                class_name: "Popup".into(),
                process_name: "code.exe".into(),
                maximized: false,
                transient: true,
                arranged: false,
                topmost: true,
            },
        ];

        let layouts = layouts_from_windows(windows);
        assert_eq!(layouts.len(), 2);
        assert_eq!(layouts[0].occurrence, 0);
        assert_eq!(layouts[1].occurrence, 1);
        assert_eq!(layouts[1].maximized, true);
        // z-order 在按身份排序后依然随窗口保留：0 = 枚举时最顶层。
        assert_eq!(layouts[0].z_index, 0);
        assert_eq!(layouts[1].z_index, 1);
    }

    #[test]
    fn layout_targets_forward_z_index_and_drop_snap_placement() {
        let targets = vec![
            // 可见 Snap 窗口：rect 是左半屏可见矩形，placement 被捕获逻辑丢弃。
            crate::platform::WindowLayoutTarget {
                handle: crate::platform::WindowHandle(1),
                z_index: 0,
                rect: crate::platform::Rect {
                    left: 0,
                    top: 0,
                    right: 960,
                    bottom: 1040,
                },
                placement_rect: None,
                title: "Snap Editor".into(),
                class_name: "Chrome_WidgetWin_1".into(),
                process_name: "code.exe".into(),
                minimized: false,
                maximized: false,
                transient: false,
                topmost: false,
            },
            crate::platform::WindowLayoutTarget {
                handle: crate::platform::WindowHandle(2),
                z_index: 1,
                rect: crate::platform::Rect {
                    left: 960,
                    top: 0,
                    right: 1920,
                    bottom: 1040,
                },
                placement_rect: Some(crate::platform::Rect {
                    left: 100,
                    top: 100,
                    right: 900,
                    bottom: 700,
                }),
                title: "A Normal Editor".into(),
                class_name: "Chrome_WidgetWin_1".into(),
                process_name: "code.exe".into(),
                minimized: false,
                maximized: false,
                transient: false,
                topmost: false,
            },
        ];

        let layouts = layouts_from_layout_targets(targets, &HashMap::new());
        assert_eq!(layouts.len(), 2);
        // 输出按稳定身份排序，与 z-order 无关；zIndex 字段是预览叠放顺序的唯一来源。
        assert_eq!(layouts[0].title, "A Normal Editor");
        assert_eq!(layouts[1].title, "Snap Editor");

        let serialized = serde_json::to_value(&layouts).unwrap();
        let serialized = serialized.as_array().unwrap();
        let find = |title: &str| {
            serialized
                .iter()
                .find(|value| value["title"] == title)
                .expect("layout is serialized")
        };
        assert_eq!(find("Snap Editor")["zIndex"], 0);
        assert_eq!(find("A Normal Editor")["zIndex"], 1);
        // Snap 快照没有 placementRect，恢复会走可见矩形 fallback。
        assert!(find("Snap Editor")["placementRect"].is_null());
        assert_eq!(
            find("Snap Editor")["rect"],
            serde_json::json!({"left": 0, "top": 0, "right": 960, "bottom": 1040})
        );
    }

    #[test]
    fn old_layout_snapshots_without_z_index_default_to_zero() {
        let layout: WindowLayout = serde_json::from_value(serde_json::json!({
            "processName": "app",
            "className": "Window",
            "title": "title",
            "rect": {"left": 0, "top": 0, "right": 10, "bottom": 10},
            "maximized": false,
            "topmost": false
        }))
        .unwrap();
        assert_eq!(layout.z_index, 0);
        assert_eq!(layout.placement_rect, None);
    }

    #[test]
    fn layout_restore_payload_is_bounded_and_keeps_request_errors_typed() {
        let error = parse_window_layouts(&serde_json::json!({"windows": []}));
        assert!(error.is_ok());

        let error = parse_window_layouts(&serde_json::json!({"windows": "bad"})).unwrap_err();
        assert_eq!(error.code, ErrorCode::WindowOperationFailed);

        let oversized = serde_json::json!({
            "windows": (0..513).map(|_| serde_json::json!({
                "processName": "app",
                "className": "Window",
                "title": "title",
                "occurrence": 0,
                "rect": {"left": 0, "top": 0, "right": 10, "bottom": 10},
                "maximized": false,
                "topmost": false
            })).collect::<Vec<_>>()
        });
        let error = parse_window_layouts(&oversized).unwrap_err();
        assert!(error.details.contains("too many entries"));
    }

    #[test]
    fn layout_snapshot_round_trips_minimized_state_and_native_placement() {
        let value = serde_json::json!({
            "processName": "code.exe",
            "className": "Chrome_WidgetWin_1",
            "title": "Editor",
            "occurrence": 0,
            "zIndex": 3,
            "rect": {"left": 10, "top": 20, "right": 810, "bottom": 620},
            "placementRect": {"left": 8, "top": 10, "right": 808, "bottom": 610},
            "minimized": true,
            "maximized": false,
            "topmost": false
        });
        let layout: WindowLayout = serde_json::from_value(value.clone()).unwrap();
        let serialized = serde_json::to_value(layout).unwrap();
        assert_eq!(serialized["minimized"], true);
        assert_eq!(serialized["placementRect"], value["placementRect"]);
        assert_eq!(serialized["zIndex"], 3);
    }

    #[test]
    fn layout_capture_injects_edge_hidden_and_records_the_expanded_rect() {
        let restore_rect = crate::platform::Rect {
            left: 0,
            top: 120,
            right: 600,
            bottom: 700,
        };
        let hidden_rect = crate::platform::Rect {
            left: -584,
            top: 120,
            right: 16,
            bottom: 700,
        };
        let handle = crate::platform::WindowHandle(7);
        // The platform enumerates an edge-hidden window at its off-screen rect
        // and keeps a native placement for it; both must be replaced by the
        // expanded rectangle so the restore path can put it back on screen.
        let targets = vec![crate::platform::WindowLayoutTarget {
            handle,
            z_index: 0,
            rect: hidden_rect,
            placement_rect: Some(hidden_rect),
            title: "Editor".into(),
            class_name: "Window".into(),
            process_name: "editor.exe".into(),
            minimized: false,
            maximized: false,
            transient: false,
            topmost: true,
        }];
        let collapsed = HashMap::from([(
            handle.0,
            CollapsedWindow {
                handle,
                edge: Edge::Left,
                restore_rect,
                hidden_rect,
                original_topmost: false,
            },
        )]);

        let layouts = layouts_from_layout_targets(targets, &collapsed);
        assert_eq!(layouts.len(), 1);
        assert!(
            !layouts[0].topmost,
            "capture must not persist the temporary edge-hide topmost bit"
        );
        assert_eq!(layouts[0].rect, restore_rect);
        assert_eq!(layouts[0].placement_rect, None);
        assert_eq!(
            layouts[0].edge_hidden,
            Some(EdgeHiddenLayout {
                edge: Edge::Left,
                restore_rect,
                hidden_rect,
            })
        );

        let serialized = serde_json::to_value(&layouts[0]).unwrap();
        assert_eq!(serialized["edgeHidden"]["edge"], "left");
        assert_eq!(
            serialized["edgeHidden"]["restoreRect"],
            serde_json::json!({"left": 0, "top": 120, "right": 600, "bottom": 700})
        );
        assert_eq!(
            serialized["edgeHidden"]["hiddenRect"],
            serde_json::json!({"left": -584, "top": 120, "right": 16, "bottom": 700})
        );

        // A plain window keeps the legacy snapshot shape with no field at all.
        let plain = layouts_from_layout_targets(
            vec![crate::platform::WindowLayoutTarget {
                handle: crate::platform::WindowHandle(8),
                z_index: 1,
                rect: restore_rect,
                placement_rect: None,
                title: "Plain".into(),
                class_name: "Window".into(),
                process_name: "editor.exe".into(),
                minimized: false,
                maximized: false,
                transient: false,
                topmost: false,
            }],
            &HashMap::new(),
        );
        assert_eq!(plain[0].edge_hidden, None);
        assert!(serde_json::to_value(&plain[0])
            .unwrap()
            .get("edgeHidden")
            .is_none());
    }

    #[test]
    fn old_and_new_layout_payloads_parse_with_optional_edge_hidden() {
        let legacy: WindowLayout = serde_json::from_value(serde_json::json!({
            "processName": "app",
            "className": "Window",
            "title": "title",
            "rect": {"left": 0, "top": 0, "right": 10, "bottom": 10},
            "maximized": false,
            "topmost": false
        }))
        .unwrap();
        assert_eq!(legacy.edge_hidden, None);

        let payload = serde_json::json!({
            "windows": [{
                "processName": "app",
                "className": "Window",
                "title": "title",
                "rect": {"left": 0, "top": 120, "right": 600, "bottom": 700},
                "maximized": false,
                "topmost": false,
                "edgeHidden": {
                    "edge": "right",
                    "restoreRect": {"left": 1320, "top": 120, "right": 1920, "bottom": 700},
                    "hiddenRect": {"left": 1904, "top": 120, "right": 2504, "bottom": 700}
                }
            }]
        });
        let parsed = parse_window_layouts(&payload).unwrap();
        assert_eq!(
            parsed[0].edge_hidden,
            Some(EdgeHiddenLayout {
                edge: Edge::Right,
                restore_rect: crate::platform::Rect {
                    left: 1320,
                    top: 120,
                    right: 1920,
                    bottom: 700,
                },
                hidden_rect: crate::platform::Rect {
                    left: 1904,
                    top: 120,
                    right: 2504,
                    bottom: 700,
                },
            })
        );

        let invalid_edge = serde_json::json!({
            "windows": [{
                "processName": "app",
                "className": "Window",
                "title": "title",
                "rect": {"left": 0, "top": 0, "right": 10, "bottom": 10},
                "maximized": false,
                "topmost": false,
                "edgeHidden": {
                    "edge": "diagonal",
                    "restoreRect": {"left": 0, "top": 0, "right": 10, "bottom": 10},
                    "hiddenRect": {"left": 0, "top": 0, "right": 10, "bottom": 10}
                }
            }]
        });
        let error = parse_window_layouts(&invalid_edge).unwrap_err();
        assert_eq!(error.code, ErrorCode::WindowOperationFailed);
    }

    #[test]
    fn hotkey_sync_payload_is_validated_before_registration() {
        let hotkeys = parse_hotkeys(&serde_json::json!({
            "hotkeys": [
                {"id": "layout-1", "accelerator": "Ctrl+Alt+1"},
                {"id": "layout-2", "accelerator": "Win+Shift+F5"}
            ]
        }))
        .unwrap();
        assert_eq!(
            hotkeys,
            vec![
                ("layout-1".to_owned(), "Ctrl+Alt+1".to_owned()),
                ("layout-2".to_owned(), "Win+Shift+F5".to_owned()),
            ]
        );

        assert!(parse_hotkeys(&serde_json::json!({})).is_err());
        assert!(parse_hotkeys(&serde_json::json!({"hotkeys": "bad"})).is_err());
        assert!(parse_hotkeys(&serde_json::json!({
            "hotkeys": [{"id": "", "accelerator": "Ctrl+A"}]
        }))
        .is_err());

        let oversized: Vec<Value> = (0..65)
            .map(|index| {
                serde_json::json!({
                    "id": format!("layout-{index}"),
                    "accelerator": "Ctrl+A"
                })
            })
            .collect();
        let error = parse_hotkeys(&serde_json::json!({ "hotkeys": oversized })).unwrap_err();
        assert!(error.to_string().contains("too many entries"));
    }

    #[test]
    fn hotkey_sync_accepts_valid_entries_and_reports_rejections() {
        let owner = "test-connection";
        let hotkeys = parse_hotkeys(&serde_json::json!({
            "hotkeys": [
                {"id": "layout-1", "accelerator": "Ctrl+Alt+1"},
                {"id": "layout-2", "accelerator": "A"},
                {"id": "layout-3", "accelerator": "Ctrl+Alt+1"}
            ]
        }))
        .unwrap();
        let rejected = crate::platform::shortcut_hotkeys::sync(owner, &hotkeys);
        // 无修饰键与重复组合键都要逐条拒绝，整次同步不能失败。
        assert_eq!(rejected.len(), 2);
        assert!(rejected.iter().any(|item| item.id == "layout-2"));
        assert!(rejected.iter().any(|item| item.id == "layout-3"));
        assert_eq!(hotkeys.len().saturating_sub(rejected.len()), 1);
        crate::platform::shortcut_hotkeys::clear(Some(owner));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_config_persistence_is_serialized() {
        let (config_tx, _) = watch::channel(AppConfig::default());
        let update_lock = Arc::new(AsyncMutex::new(()));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));

        let run_update = |edge_size: i32| {
            let config_tx = config_tx.clone();
            let update_lock = Arc::clone(&update_lock);
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            tokio::spawn(async move {
                let config = AppConfig {
                    edge_size,
                    ..AppConfig::default()
                };
                apply_config_update_with(&config_tx, &update_lock, config, move |_| {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(current, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(30));
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
                .await
            })
        };

        let (first, second) = tokio::join!(run_update(18), run_update(22));
        first.unwrap().unwrap();
        second.unwrap().unwrap();

        assert_eq!(maximum.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_config_persistence_does_not_replace_runtime_config() {
        let original = AppConfig::default();
        let original_edge_size = original.edge_size;
        let (config_tx, config_rx) = watch::channel(original);
        let update_lock = AsyncMutex::new(());
        let changed = AppConfig {
            edge_size: original_edge_size + 4,
            ..AppConfig::default()
        };

        let result = apply_config_update_with(&config_tx, &update_lock, changed, |_| {
            anyhow::bail!("simulated disk failure")
        })
        .await;

        assert!(result.is_err());
        assert_eq!(config_rx.borrow().edge_size, original_edge_size);
    }
}
