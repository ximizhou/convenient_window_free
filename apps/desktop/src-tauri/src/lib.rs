mod storage;
mod supervisor;
#[cfg(windows)]
mod windows_lifecycle;

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use supervisor::{HelperProcess, StartResult, StopResult};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const HELPER_VERSION: &str = "0.5.6";
const REPOSITORY_URL: &str = "https://github.com/ximizhou/convenient_window_free";
const SMOKE_EXIT_ENV: &str = "CONVENIENT_WINDOW_SMOKE_EXIT_MS";
const DATA_DIR_ENV: &str = "CONVENIENT_WINDOW_DATA_DIR";

#[derive(Clone)]
struct DesktopPaths {
    app_data_dir: PathBuf,
    helper_data_dir: PathBuf,
    settings_path: PathBuf,
    helper_payload_dir: PathBuf,
}

impl DesktopPaths {
    fn resolve(app: &AppHandle) -> Result<Self, String> {
        let app_data_dir = match explicit_data_dir()? {
            Some(path) => path,
            None => app
                .path()
                .app_local_data_dir()
                .map_err(|error| format!("无法定位应用数据目录：{error}"))?,
        };
        let helper_payload_dir = resolve_helper_payload_dir(app)?;
        Self::from_roots(app_data_dir, helper_payload_dir)
    }

    fn from_roots(app_data_dir: PathBuf, helper_payload_dir: PathBuf) -> Result<Self, String> {
        let helper_data_dir = app_data_dir.join("helper-data");
        let settings_path = app_data_dir.join("desktop-settings.json");
        std::fs::create_dir_all(&helper_data_dir)
            .map_err(|error| format!("无法创建应用数据目录：{error}"))?;
        migrate_legacy_desktop_settings(&settings_path, &helper_data_dir.join("config.json"))?;
        Ok(Self {
            app_data_dir,
            helper_data_dir,
            settings_path,
            helper_payload_dir,
        })
    }
}

struct DesktopState {
    paths: DesktopPaths,
    helper: Arc<Mutex<HelperProcess>>,
    settings_write_lock: Mutex<()>,
    shutdown_started: AtomicBool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopStatus {
    data_dir: String,
    helper_path: String,
    helper_exists: bool,
    helper_running: bool,
    helper_bytes: u64,
    helper_version: &'static str,
    helper_error: Option<String>,
    repository: &'static str,
    token: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopDiagnostics {
    app_data_dir: String,
    settings_path: String,
    helper_data_dir: String,
    helper_path: String,
    helper_running: bool,
    helper_payload_bytes: u64,
    last_exit_code: Option<i32>,
    last_error: Option<String>,
    log_tail: Vec<String>,
}

#[tauri::command]
fn desktop_status(state: State<'_, DesktopState>) -> Result<DesktopStatus, String> {
    let payload = supervisor::validate_payload(&state.paths.helper_payload_dir);
    let helper_path = payload.as_ref().cloned().unwrap_or_else(|_| {
        state
            .paths
            .helper_payload_dir
            .join("magic-corners-helper.exe")
    });
    let helper_error = payload.err();
    let token = supervisor::read_valid_token(&state.paths.helper_data_dir.join("auth-token")).ok();
    let helper_running = state
        .helper
        .lock()
        .map_err(|_| "helper 进程状态锁已损坏".to_string())?
        .running();
    Ok(DesktopStatus {
        data_dir: path_string(&state.paths.app_data_dir),
        helper_path: path_string(&helper_path),
        helper_exists: helper_error.is_none(),
        helper_running,
        helper_bytes: supervisor::payload_size(&state.paths.helper_payload_dir),
        helper_version: HELPER_VERSION,
        helper_error,
        repository: REPOSITORY_URL,
        token,
    })
}

#[tauri::command]
async fn start_helper(state: State<'_, DesktopState>) -> Result<StartResult, String> {
    let helper = Arc::clone(&state.helper);
    let payload_dir = state.paths.helper_payload_dir.clone();
    let data_dir = state.paths.helper_data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        helper
            .lock()
            .map_err(|_| "helper 进程状态锁已损坏".to_string())?
            .start(&payload_dir, &data_dir)
    })
    .await
    .map_err(|error| format!("helper 启动任务失败：{error}"))?
}

#[tauri::command]
async fn stop_helper(state: State<'_, DesktopState>) -> Result<StopResult, String> {
    let helper = Arc::clone(&state.helper);
    let data_dir = state.paths.helper_data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        helper
            .lock()
            .map_err(|_| "helper 进程状态锁已损坏".to_string())?
            .stop(&data_dir)
    })
    .await
    .map_err(|error| format!("helper 停止任务失败：{error}"))?
}

#[tauri::command]
fn load_config(state: State<'_, DesktopState>) -> Result<Option<Value>, String> {
    storage::read_json_with_backup(&state.paths.settings_path)
}

#[tauri::command]
fn save_config(settings: Value, state: State<'_, DesktopState>) -> Result<(), String> {
    let _guard = state
        .settings_write_lock
        .lock()
        .map_err(|_| "配置写入锁已损坏".to_string())?;
    storage::write_json_with_backup(&state.paths.settings_path, &settings)
}

#[tauri::command]
fn read_config_file(path: String) -> Result<String, String> {
    storage::read_config_file(Path::new(&path))
}

#[tauri::command]
fn write_config_file(path: String, content: String) -> Result<(), String> {
    storage::write_config_file(Path::new(&path), &content)
}

#[tauri::command]
fn diagnostics(state: State<'_, DesktopState>) -> Result<DesktopDiagnostics, String> {
    let mut helper = state
        .helper
        .lock()
        .map_err(|_| "helper 进程状态锁已损坏".to_string())?;
    let running = helper.running();
    let helper_path = state
        .paths
        .helper_payload_dir
        .join("magic-corners-helper.exe");
    Ok(DesktopDiagnostics {
        app_data_dir: path_string(&state.paths.app_data_dir),
        settings_path: path_string(&state.paths.settings_path),
        helper_data_dir: path_string(&state.paths.helper_data_dir),
        helper_path: path_string(&helper_path),
        helper_running: running,
        helper_payload_bytes: supervisor::payload_size(&state.paths.helper_payload_dir),
        last_exit_code: helper.last_exit_code(),
        last_error: helper.last_error(),
        log_tail: supervisor::log_tail(
            &state.paths.helper_data_dir.join("magic-corners-helper.log"),
            80,
        ),
    })
}

fn resolve_helper_payload_dir(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(explicit) = std::env::var_os("CONVENIENT_WINDOW_HELPER_DIR") {
        return Ok(PathBuf::from(explicit));
    }
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("无法定位应用资源目录：{error}"))?;
    let packaged = resource_dir.join("helper");
    if packaged.join("magic-corners-helper.exe").is_file() {
        return Ok(packaged);
    }
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("helper");
    if development.exists() {
        return Ok(development);
    }
    Ok(packaged)
}

fn explicit_data_dir() -> Result<Option<PathBuf>, String> {
    parse_explicit_data_dir(std::env::var_os(DATA_DIR_ENV))
}

fn parse_explicit_data_dir(value: Option<std::ffi::OsString>) -> Result<Option<PathBuf>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(format!("{DATA_DIR_ENV} 必须是绝对路径"));
    }
    Ok(Some(path))
}

fn migrate_legacy_desktop_settings(
    settings_path: &Path,
    helper_config_path: &Path,
) -> Result<(), String> {
    if settings_path.exists() {
        return Ok(());
    }
    if let Some(value) = storage::read_json_with_backup(helper_config_path)? {
        storage::write_json_with_backup(settings_path, &value)?;
    }
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "打开设置", true, None::<&str>)?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机自动启动",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出便捷窗口", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &autostart, &separator, &quit])?;
    let autostart_menu = autostart.clone();
    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("便捷窗口")
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "autostart" => {
                let desired = autostart_menu.is_checked().unwrap_or(false);
                let manager = app.autolaunch();
                let previous = manager.is_enabled().unwrap_or(!desired);
                let result = if desired {
                    manager.enable()
                } else {
                    manager.disable()
                };
                if result.is_err() {
                    let _ = autostart_menu.set_checked(previous);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn stop_managed_helper(app: &AppHandle) {
    let state = app.state::<DesktopState>();
    if state.shutdown_started.swap(true, Ordering::AcqRel) {
        return;
    }
    if let Ok(mut helper) = state.helper.lock() {
        let _ = helper.stop(&state.paths.helper_data_dir);
    };
}

fn schedule_smoke_exit(app: &AppHandle) {
    let value = std::env::var(SMOKE_EXIT_ENV).ok();
    let Some(delay) = parse_smoke_exit_delay(value.as_deref()) else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        app.exit(0);
    });
}

fn parse_smoke_exit_delay(value: Option<&str>) -> Option<std::time::Duration> {
    let milliseconds = value?.parse::<u64>().ok()?;
    (1_000..=120_000)
        .contains(&milliseconds)
        .then(|| std::time::Duration::from_millis(milliseconds))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let explicit_data_dir = explicit_data_dir().expect("invalid desktop data directory override");
    let mut context = tauri::generate_context!();
    let isolated_window = explicit_data_dir.as_ref().and_then(|data_dir| {
        let position = context
            .config()
            .app
            .windows
            .iter()
            .position(|window| window.label == "main")?;
        let config = context.config_mut().app.windows.remove(position);
        Some((config, data_dir.join("webview-data")))
    });
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .setup(move |app| {
            if let Some((config, webview_data_dir)) = &isolated_window {
                std::fs::create_dir_all(webview_data_dir)?;
                tauri::WebviewWindowBuilder::from_config(app, config)?
                    .data_directory(webview_data_dir.clone())
                    .build()?;
            }
            let paths = DesktopPaths::resolve(app.handle())
                .map_err(|error| std::io::Error::other(error))?;
            app.manage(DesktopState {
                paths,
                helper: Arc::new(Mutex::new(HelperProcess::default())),
                settings_write_lock: Mutex::new(()),
                shutdown_started: AtomicBool::new(false),
            });
            create_tray(app.handle())?;
            #[cfg(windows)]
            windows_lifecycle::listen_for_uninstall(app.handle().clone())
                .map_err(std::io::Error::other)?;
            let autostart = std::env::args().any(|argument| argument == "--autostart");
            if !autostart {
                show_main_window(app.handle());
            }
            schedule_smoke_exit(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            desktop_status,
            start_helper,
            stop_helper,
            load_config,
            save_config,
            read_config_file,
            write_config_file,
            diagnostics
        ])
        .build(context)
        .expect("error while building Convenient Window");

    app.run(|app_handle, event| {
        if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
            stop_managed_helper(app_handle);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_repository_is_the_only_external_target() {
        assert_eq!(
            REPOSITORY_URL,
            "https://github.com/ximizhou/convenient_window_free"
        );
    }

    #[test]
    fn smoke_exit_delay_is_bounded_and_opt_in() {
        assert_eq!(
            parse_smoke_exit_delay(Some("2500")),
            Some(std::time::Duration::from_millis(2500))
        );
        assert_eq!(parse_smoke_exit_delay(None), None);
        assert_eq!(parse_smoke_exit_delay(Some("999")), None);
        assert_eq!(parse_smoke_exit_delay(Some("120001")), None);
        assert_eq!(parse_smoke_exit_delay(Some("not-a-number")), None);
    }

    #[test]
    fn desktop_settings_are_not_written_to_the_helper_config_file() {
        let root = std::env::temp_dir().join(format!(
            "convenient-window-desktop-paths-{}",
            uuid::Uuid::new_v4()
        ));
        let paths = DesktopPaths::from_roots(root.clone(), root.join("payload")).unwrap();

        assert_eq!(paths.settings_path, root.join("desktop-settings.json"));
        assert_ne!(
            paths.settings_path,
            paths.helper_data_dir.join("config.json")
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn desktop_settings_migrate_once_from_the_legacy_shared_file() {
        let root = std::env::temp_dir().join(format!(
            "convenient-window-desktop-migration-{}",
            uuid::Uuid::new_v4()
        ));
        let helper_data = root.join("helper-data");
        std::fs::create_dir_all(&helper_data).unwrap();
        storage::write_json_with_backup(
            &helper_data.join("config.json"),
            &serde_json::json!({ "schemaVersion": 7, "enabled": true }),
        )
        .unwrap();

        let paths = DesktopPaths::from_roots(root.clone(), root.join("payload")).unwrap();
        assert_eq!(
            storage::read_json_with_backup(&paths.settings_path).unwrap(),
            Some(serde_json::json!({ "schemaVersion": 7, "enabled": true }))
        );

        storage::write_json_with_backup(
            &paths.settings_path,
            &serde_json::json!({ "schemaVersion": 7, "enabled": false }),
        )
        .unwrap();
        let paths = DesktopPaths::from_roots(root.clone(), root.join("payload")).unwrap();
        assert_eq!(
            storage::read_json_with_backup(&paths.settings_path).unwrap(),
            Some(serde_json::json!({ "schemaVersion": 7, "enabled": false }))
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_data_directory_must_be_absolute() {
        assert!(parse_explicit_data_dir(None).unwrap().is_none());
        assert!(parse_explicit_data_dir(Some("relative".into())).is_err());
        let absolute = std::env::temp_dir().join("convenient-window-explicit-data");
        assert_eq!(
            parse_explicit_data_dir(Some(absolute.clone().into_os_string())).unwrap(),
            Some(absolute)
        );
    }
}
