#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod auth;
mod config;
mod core;
mod ipc;
mod logging;
mod paths;
mod platform;
mod single_instance;
mod storage;
mod usage;

use anyhow::{Context, Result};
use config::AppConfig;
use core::engine::Engine;
use ipc::messages::HelperMessage;
use ipc::websocket::WebSocketServer;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

fn config_path() -> Option<std::path::PathBuf> {
    paths::data_file("config.json")
}

fn load_config() -> Option<AppConfig> {
    let path = config_path()?;
    match storage::read_json_with_backup::<AppConfig>(&path) {
        Ok(Some((config, recovered))) => {
            if recovered {
                logging::write_line("main: recovered config from backup");
            }
            Some(config.normalized())
        }
        Ok(None) => None,
        Err(error) => {
            logging::write_line(format!("main: config load failed: {error:#}"));
            None
        }
    }
}

pub fn save_config(config: &AppConfig) -> Result<()> {
    static CONFIG_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = CONFIG_WRITE_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|_| anyhow::anyhow!("config write lock is poisoned"))?;
    let path = config_path().context("helper executable directory is unavailable")?;
    storage::write_json_with_backup(&path, config)
        .with_context(|| format!("write config to {}", path.display()))?;
    Ok(())
}

async fn supervise_runtime(
    mut server_task: JoinHandle<Result<()>>,
    mut engine_task: JoinHandle<Result<()>>,
    shutdown_tx: broadcast::Sender<()>,
) -> Result<()> {
    tokio::select! {
        server = &mut server_task => {
            let server = completed_task("websocket server", server);
            match &server {
                Ok(()) => logging::write_line("main: websocket server stopped"),
                Err(error) => logging::write_line(format!("main: websocket server task failed: {error:#}")),
            }
            let _ = shutdown_tx.send(());
            let engine = wait_for_task("engine", &mut engine_task).await;
            server.and(engine)
        }
        engine = &mut engine_task => {
            let engine = completed_task("engine", engine);
            match &engine {
                Ok(()) => logging::write_line("main: engine task stopped"),
                Err(error) => logging::write_line(format!("main: engine task failed: {error:#}")),
            }
            let _ = shutdown_tx.send(());
            let server = wait_for_task("websocket server", &mut server_task).await;
            engine.context("engine stopped unexpectedly").and(server)
        }
    }
}

fn completed_task(
    name: &str,
    joined: std::result::Result<Result<()>, tokio::task::JoinError>,
) -> Result<()> {
    joined.with_context(|| format!("{name} task failed"))?
}

async fn wait_for_task(name: &str, task: &mut JoinHandle<Result<()>>) -> Result<()> {
    match tokio::time::timeout(Duration::from_secs(2), &mut *task).await {
        Ok(joined) => joined.with_context(|| format!("{name} task failed"))?,
        Err(_) => {
            logging::write_line(format!("main: {name} task did not stop in time; aborting"));
            task.abort();
            anyhow::bail!("{name} task did not stop in time")
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    paths::initialize()?;
    logging::write_line("main: starting");
    let Some(_single_instance) = single_instance::SingleInstance::acquire()? else {
        const CONFLICT: &str =
            "HELPER_INSTANCE_CONFLICT: another Convenient Window helper is already running";
        logging::write_line(format!("main: {CONFLICT}"));
        anyhow::bail!(CONFLICT);
    };

    let initial_config = load_config().unwrap_or_default();
    let auth_token = auth::load_or_create_token()?;
    let (config_tx, config_rx) = watch::channel(initial_config);
    let (event_tx, _) = broadcast::channel::<HelperMessage>(128);
    let (shutdown_tx, _) = broadcast::channel::<()>(4);
    let usage = usage::UsageTracker::load(event_tx.clone())?;
    let mut usage_task = {
        let usage = usage.clone();
        let event_rx = event_tx.subscribe();
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move { usage.run(event_rx, shutdown_rx).await })
    };
    let engine = Arc::new(Engine::new(
        config_rx,
        event_tx.clone(),
        shutdown_tx.subscribe(),
    ));
    let server = WebSocketServer::new(
        "127.0.0.1:56873",
        auth_token,
        config_tx,
        event_tx,
        shutdown_tx.clone(),
        usage,
    );

    let server_task = tokio::spawn(async move { server.run().await });
    let engine_task = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move { engine.run().await })
    };

    logging::write_line("main: supervising websocket server and engine");
    let runtime_result = supervise_runtime(server_task, engine_task, shutdown_tx.clone()).await;
    let _ = shutdown_tx.send(());

    if tokio::time::timeout(Duration::from_secs(2), &mut usage_task)
        .await
        .is_err()
    {
        logging::write_line("main: usage task did not stop in time; aborting");
        usage_task.abort();
    }

    runtime_result
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn engine_failure_stops_the_server_and_returns_an_error() {
        let (shutdown_tx, _) = broadcast::channel(4);
        let stopped = Arc::new(AtomicBool::new(false));
        let server_stopped = Arc::clone(&stopped);
        let mut shutdown_rx = shutdown_tx.subscribe();
        let server = tokio::spawn(async move {
            let _ = shutdown_rx.recv().await;
            server_stopped.store(true, Ordering::SeqCst);
            Ok(())
        });
        let engine = tokio::spawn(async { anyhow::bail!("simulated engine failure") });

        let result = supervise_runtime(server, engine, shutdown_tx).await;

        assert!(result
            .unwrap_err()
            .to_string()
            .contains("engine stopped unexpectedly"));
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn engine_panic_still_stops_the_server() {
        let (shutdown_tx, _) = broadcast::channel(4);
        let stopped = Arc::new(AtomicBool::new(false));
        let server_stopped = Arc::clone(&stopped);
        let mut shutdown_rx = shutdown_tx.subscribe();
        let server = tokio::spawn(async move {
            let _ = shutdown_rx.recv().await;
            server_stopped.store(true, Ordering::SeqCst);
            Ok(())
        });
        let engine = tokio::spawn(async {
            panic!("simulated engine panic");
            #[allow(unreachable_code)]
            Ok(())
        });

        let result = supervise_runtime(server, engine, shutdown_tx).await;

        let error = result.unwrap_err().to_string();
        assert!(error.contains("engine stopped unexpectedly"));
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn server_panic_still_stops_the_engine() {
        let (shutdown_tx, _) = broadcast::channel(4);
        let stopped = Arc::new(AtomicBool::new(false));
        let engine_stopped = Arc::clone(&stopped);
        let mut shutdown_rx = shutdown_tx.subscribe();
        let server = tokio::spawn(async {
            panic!("simulated server panic");
            #[allow(unreachable_code)]
            Ok(())
        });
        let engine = tokio::spawn(async move {
            let _ = shutdown_rx.recv().await;
            engine_stopped.store(true, Ordering::SeqCst);
            Ok(())
        });

        let result = supervise_runtime(server, engine, shutdown_tx).await;

        assert!(result
            .unwrap_err()
            .to_string()
            .contains("websocket server task failed"));
        assert!(stopped.load(Ordering::SeqCst));
    }
}
