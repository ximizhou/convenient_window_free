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
                    let event_tx = self.event_tx.clone();
                    let shutdown_tx = self.shutdown_tx.clone();
                    let usage = self.usage.clone();
                    let event_rx = self.event_tx.subscribe();
                    tokio::spawn(async move {
                        logging::write_line("websocket: accepted client");
                        if let Err(error) = handle_connection(stream, &auth_token, config_tx, config_update_lock, event_tx, event_rx, shutdown_tx, usage).await {
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
    event_tx: broadcast::Sender<HelperMessage>,
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
    writer
        .send(Message::Text(
            serde_json::to_string(&HelperMessage::new(
                "helper.ready",
                serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "protocolVersion": 6,
                    "platform": crate::platform::platform_info(),
                    "ocrLanguages": ocr_languages,
                    "usage": usage.snapshot()
                }),
            ))?
            .into(),
        ))
        .await?;

    loop {
        tokio::select! {
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
                    "config.update" => {
                        let (config, revision, adjusted) = match parse_config_update(incoming.data) {
                            Ok(update) => update,
                            Err(e) => {
                                logging::write_line(format!("websocket: config deserialize error {e}"));
                                let _ = event_tx.send(HelperMessage::new("runtime.error", serde_json::json!({
                                    "message": format!("invalid config: {e}")
                                })));
                                continue;
                            }
                        };
                        if let Err(error) = apply_config_update(&config_tx, &config_update_lock, config).await {
                            logging::write_line(format!("websocket: config save error {error:#}"));
                            let _ = event_tx.send(HelperMessage::new("runtime.error", serde_json::json!({
                                "message": format!("config could not be saved: {error:#}")
                            })));
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
                        let _ = event_tx.send(HelperMessage::new("runtime.error", serde_json::json!({
                            "message": format!("unknown message type: {}", incoming.kind)
                        })));
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

fn parse_config_update(data: Value) -> serde_json::Result<(AppConfig, Option<u64>, bool)> {
    let revision = data.get("revision").and_then(Value::as_u64);
    let raw_config = data.get("config").cloned().unwrap_or(data);
    let received = serde_json::from_value::<AppConfig>(raw_config)?;
    let before = serde_json::to_value(&received)?;
    let mut config = received.normalized();
    crate::platform::apply_capability_limits(&mut config);
    let adjusted = serde_json::to_value(&config)? != before;
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn token_comparison_rejects_mismatches() {
        assert!(constant_time_eq("abc123", "abc123"));
        assert!(!constant_time_eq("abc123", "abc124"));
        assert!(!constant_time_eq("abc123", "abc1234"));
    }

    #[test]
    fn config_update_envelope_keeps_revision_and_reports_normalization() {
        let (config, revision, adjusted) = parse_config_update(serde_json::json!({
            "revision": 7,
            "config": { "enabled": true, "edgeSize": 999 }
        }))
        .unwrap();

        assert_eq!(revision, Some(7));
        assert_eq!(config.edge_size, 48);
        assert!(adjusted);

        let normalized = serde_json::to_value(AppConfig::default().normalized()).unwrap();
        let (_, revision, adjusted) = parse_config_update(serde_json::json!({
            "revision": 8,
            "config": normalized
        }))
        .unwrap();
        assert_eq!(revision, Some(8));
        assert!(!adjusted);
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
