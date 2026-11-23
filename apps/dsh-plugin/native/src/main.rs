#![windows_subsystem = "windows"]
#[path = "../../../desktop/src-tauri/src/adjustment_hud.rs"]
mod adjustment_hud;
#[path = "../../../desktop/src-tauri/src/command_broker.rs"]
mod command_broker;
#[path = "../../../desktop/src-tauri/src/startup_manager.rs"]
mod startup_manager;
#[path = "../../../desktop/src-tauri/src/windows_elevation.rs"]
mod windows_elevation;
#[allow(dead_code)]
#[path = "../../../desktop/src-tauri/src/windows_lifecycle.rs"]
mod windows_lifecycle;
#[allow(dead_code)]
#[path = "../../../../shared/windows_process.rs"]
mod windows_process;
#[allow(dead_code)]
mod supervisor {
    include!("../../../desktop/src-tauri/src/supervisor.rs");
    impl HelperProcess {
        pub fn owned_pid(&self) -> Option<u32> {
            self.child.as_ref().map(|p| p.id())
        }
    }
}
#[allow(dead_code)]
mod login {
    include!(concat!(env!("OUT_DIR"), "/dsh_autostart.rs"));
}
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
struct Paths {
    helper_data_dir: PathBuf,
    payload: PathBuf,
}
struct DesktopState {
    paths: Paths,
    helper: Mutex<supervisor::HelperProcess>,
    shutdown_started: AtomicBool,
    picked: Mutex<Option<(PathBuf, Instant)>>,
    language: Mutex<String>,
}
fn explicit_data_dir() -> Result<Option<PathBuf>, String> {
    let p = std::env::var_os("CW_DSH_DATA")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Missing DSH profile")?;
    Ok(Some(p))
}
fn native_dir() -> Result<PathBuf, String> {
    std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or("Missing native payload".into())
}
fn data() -> Result<PathBuf, String> {
    explicit_data_dir()?.ok_or("Missing profile".into())
}
// Use the direct package link. pnpm's versioned store can change on every upgrade.
fn installed_profile() -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let exe = executable.canonicalize()?;
    executable
        .ancestors()
        .filter(|p| p.file_name().is_some_and(|n| n == "node_modules"))
        .filter_map(Path::parent)
        .find(|p| {
            p.join("cordis.yml").is_file()
                && package_launcher(p).canonicalize().is_ok_and(|v| v == exe)
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("missing-profile"))
}
fn package_launcher(profile: &Path) -> PathBuf {
    profile
        .join("node_modules")
        .join(env!("CW_DSH_PACKAGE_NAME"))
        .join("lib/cw-dsh-native.exe")
}
fn login_executable() -> io::Result<PathBuf> {
    match installed_profile() {
        Ok(p) => Ok(package_launcher(&p)),
        Err(_) => std::env::current_exe(),
    }
}
fn msg(app: &AppHandle, title: &str, text: String) -> bool {
    app.dialog()
        .message(text)
        .title(title)
        .buttons(MessageDialogButtons::OkCancel)
        .blocking_show()
}
fn english(s: &DesktopState) -> bool {
    s.language.lock().map(|x| *x == "en-US").unwrap_or(true)
}
#[tauri::command]
fn hud_language(state: tauri::State<'_, DesktopState>) -> String {
    state
        .language
        .lock()
        .map(|x| x.clone())
        .unwrap_or("en-US".into())
}
fn run_request(app: &AppHandle, op: &str, v: &Value) -> Result<Value, String> {
    let s = app.state::<DesktopState>();
    let en = english(&s);
    let label = if en {
        "Convenient Window · DSH"
    } else {
        "便捷窗口 · DSH"
    };
    let payload = &s.paths.payload;
    let dir = &s.paths.helper_data_dir;
    match op {
        "start" | "elevation" => {
            let mut helper = s.helper.lock().map_err(|_| "busy")?;
            let r = if op == "start" {
                helper.start(payload, dir)
            } else {
                helper.switch_mode(payload, dir, v["enabled"].as_bool().ok_or("invalid-state")?)
            }?;
            Ok(
                json!({"pid":helper.owned_pid(),"elevated":r.elevated,"alreadyRunning":r.already_running,"warning":r.warning}),
            )
        }
        "stop" => {
            s.helper.lock().map_err(|_| "busy")?.stop(dir)?;
            Ok(json!(true))
        }
        "status" => {
            let mut helper = s.helper.lock().map_err(|_| "busy")?;
            let running = helper.running();
            let title = app
                .get_webview_window("adjustment-hud")
                .and_then(|w| w.title().ok());
            Ok(
                json!({"running":running,"lastExitCode":helper.last_exit_code(),"hudTitle":title,"loginTargetValid":std::env::var_os("CW_DSH_EXECUTABLE").map(PathBuf::from).is_some_and(|p|valid_dsh(&p)),"installedProfile":installed_profile().is_ok()}),
            )
        }
        "language" => {
            let l = if v == "en-US" { "en-US" } else { "zh-CN" };
            *s.language.lock().map_err(|_| "busy")? = l.into();
            let _ = app.emit_to("adjustment-hud", "dsh-hud-language", l);
            Ok(Value::Null)
        }
        "confirm-commands" => {
            let list = v.as_array().ok_or("invalid-command")?;
            if list.len() > 48 {
                return Err("invalid-command".into());
            }
            let cmds = list
                .iter()
                .map(|x| {
                    x.as_str()
                        .filter(|t| !t.is_empty() && t.len() <= 4096)
                        .ok_or("invalid-command")
                })
                .collect::<Result<Vec<_>, _>>()?;
            let note = if en {
                "Allow the following commands for your configured gestures or hot corners?"
            } else {
                "允许以下命令在你设置的手势或触发区触发时运行吗？"
            };
            Ok(json!(msg(
                app,
                label,
                format!("{note}\n\n{}", cmds.join("\n\n"))
            )))
        }
        "pick" => {
            let file = app
                .dialog()
                .file()
                .set_title(if en {
                    "Choose a startup app"
                } else {
                    "选择启动程序"
                })
                .add_filter("Windows EXE", &["exe"])
                .blocking_pick_file()
                .and_then(|p| p.into_path().ok());
            *s.picked.lock().map_err(|_| "busy")? = file.clone().map(|p| (p, Instant::now()));
            Ok(json!(file))
        }
        "startup" => {
            let exe = supervisor::validate_payload(payload)?;
            let action = v["action"].as_str().ok_or("invalid-command")?;
            if action == "add" {
                let selected = s.picked.lock().map_err(|_| "busy")?.take();
                let path = PathBuf::from(v["executablePath"].as_str().ok_or("invalid-command")?);
                if !selected
                    .is_some_and(|(p, t)| p == path && t.elapsed() < Duration::from_secs(600))
                {
                    return Err("pick-again".into());
                }
                let prompt = if en {
                    "Add this program to Windows startup?"
                } else {
                    "将这个程序加入 Windows 登录启动项吗？"
                };
                if !msg(
                    app,
                    label,
                    format!(
                        "{prompt}\n\n{}\n{}",
                        v["name"].as_str().unwrap_or(""),
                        path.display()
                    ),
                ) {
                    return Ok(
                        json!({"version":1,"ok":false,"items":[],"warnings":[],"error":{"code":"cancelled"}}),
                    );
                }
            } else if action == "setEnabled" {
                let list = startup_manager::request(exe.clone(), json!({"action":"list"}))?;
                let item = list["items"]
                    .as_array()
                    .and_then(|l| {
                        l.iter().find(|x| {
                            x["id"] == v["id"]
                                && x["revision"] == v["revision"]
                                && x["editable"] == true
                        })
                    })
                    .ok_or("entry-changed")?;
                let prompt = match (en, v["enabled"].as_bool().ok_or("invalid-state")?) {
                    (true, true) => "Enable this Windows startup entry?",
                    (true, false) => "Disable this Windows startup entry?",
                    (false, true) => "允许这个程序在登录 Windows 时启动吗？",
                    (false, false) => "关闭这个程序的 Windows 登录自启吗？",
                };
                if !msg(
                    app,
                    label,
                    format!(
                        "{prompt}\n\n{}\n{}",
                        item["name"].as_str().unwrap_or(""),
                        item["command"].as_str().unwrap_or("")
                    ),
                ) {
                    return Ok(
                        json!({"version":1,"ok":false,"items":[],"warnings":[],"error":{"code":"cancelled"}}),
                    );
                }
            } else if action != "list" {
                return Err("invalid-command".into());
            }
            startup_manager::request(exe, v.clone())
        }
        "login" => Ok(
            json!({"enabled":login::enabled_for_current_executable().map_err(|e|e.to_string())?,"admin":login::admin_request_state().map_err(|e|e.to_string())?.enabled}),
        ),
        "set-login" | "set-login-admin" => {
            let on = v.as_bool().ok_or("invalid-state")?;
            let desired = match (op, on, en) {
                ("set-login", true, true) => "Start DSH at Windows sign-in?",
                ("set-login", false, true) => "Turn off DSH startup at Windows sign-in?",
                (_, true, true) => {
                    "Start DSH at sign-in and request administrator permission for its helper?"
                }
                (_, false, true) => "Stop requesting administrator permission at sign-in?",
                ("set-login", true, false) => "登录 Windows 时启动 DSH 吗？",
                ("set-login", false, false) => "关闭 DSH 插件的 Windows 登录自启吗？",
                (_, true, false) => "登录 Windows 时启动 DSH 并为便捷窗口助手请求管理员权限吗？",
                (_, false, false) => "关闭登录时的管理员权限请求吗？",
            };
            if !msg(app, label, format!("{desired}\n\nConvenient Window (DSH)")) {
                return run_request(app, "login", &Value::Null);
            }
            if on {
                let profile = installed_profile().map_err(|e| e.to_string())?;
                if data()?.canonicalize().ok()
                    != profile.join("convenient-window").canonicalize().ok()
                {
                    return Err("missing-profile".into());
                }
                let path =
                    PathBuf::from(std::env::var_os("CW_DSH_EXECUTABLE").ok_or("missing-dsh")?);
                if !valid_dsh(&path) {
                    return Err("missing-dsh".into());
                }
                fs::create_dir_all(data()?).map_err(|e| e.to_string())?;
                fs::write(
                    data()?.join("login-target.json"),
                    serde_json::to_vec(&path).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
            }
            if op == "set-login-admin" {
                login::set_admin_request(on)
            } else if on {
                login::enable()
            } else {
                login::disable()
            }
            .map_err(|e| e.to_string())?;
            run_request(app, "login", &Value::Null)
        }
        _ => Err("invalid-command".into()),
    }
}
fn valid_dsh(p: &Path) -> bool {
    p.is_absolute()
        && p.is_file()
        && p.file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("DeepSeek Harness.exe"))
        && p.parent()
            .is_some_and(|p| p.join("resources/runtime/cli/bin/dsh.cmd").is_file())
}
fn login_launch() -> Result<(), String> {
    if !login::enabled_for_current_executable().map_err(|e| e.to_string())? {
        return Ok(());
    }
    let profile = installed_profile().map_err(|e| e.to_string())?;
    let p = profile.join("convenient-window");
    let app: PathBuf =
        serde_json::from_slice(&fs::read(p.join("login-target.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if !valid_dsh(&app) {
        return Err("missing-dsh".into());
    }
    let home = profile
        .parent()
        .and_then(Path::parent)
        .ok_or("missing-profile")?;
    std::process::Command::new(&app)
        .env_remove("ELECTRON_RUN_AS_NODE")
        .env("DSH_HOME", home)
        .env("CW_DSH_LOGIN_DIR", p)
        .env(
            "CW_DSH_LOGIN_ADMIN",
            if std::env::args().any(|a| a == "--request-admin") {
                "1"
            } else {
                "0"
            },
        )
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn service(app: AppHandle) {
    let (tx, rx) = std::sync::mpsc::sync_channel(16);
    let close = app.clone();
    std::thread::spawn(move || {
        let input = io::stdin();
        for line in input.lock().lines() {
            let Ok(line) = line else { break };
            if line.len() > 1024 * 1024 || tx.send(line).is_err() {
                break;
            }
        }
        close.exit(0);
    });
    for line in rx {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            break;
        };
        let result = run_request(&app, v["op"].as_str().unwrap_or(""), &v["data"]);
        let response = match result {
            Ok(data) => json!({"id":v["id"],"ok":true,"data":data}),
            Err(e) => json!({"id":v["id"],"ok":false,"error":e}),
        };
        let mut out = io::stdout().lock();
        if writeln!(out, "{response}")
            .and_then(|_| out.flush())
            .is_err()
        {
            break;
        }
    }
    app.exit(0);
}
fn main() {
    if windows_process::current_elevated().unwrap_or(true) {
        std::process::exit(78);
    }
    if std::env::args().any(|a| a == "--autostart") {
        let _ = login_launch();
        return;
    }
    if !std::env::args().any(|a| a == "--dsh-host") {
        return;
    }
    let Ok(dir) = data() else { return };
    let Ok(package) = native_dir() else { return };
    let runtime = dir.join("runtime");
    if fs::create_dir_all(&runtime).is_err() {
        return;
    }
    let built = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(DesktopState {
            paths: Paths {
                helper_data_dir: runtime,
                payload: package.join("payload"),
            },
            helper: Mutex::new(supervisor::HelperProcess::default()),
            shutdown_started: AtomicBool::new(false),
            picked: Mutex::new(None),
            language: Mutex::new(std::env::var("CW_DSH_LANG").unwrap_or_else(|_| "zh-CN".into())),
        })
        .setup(|app| {
            adjustment_hud::start(app.handle())?;
            let handle = app.handle().clone();
            std::thread::spawn(move || service(handle));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            hud_language,
            adjustment_hud::adjustment_hud_ready,
            adjustment_hud::adjustment_hud_present
        ])
        .build(tauri::generate_context!());
    if let Ok(app) = built {
        app.run(|app, ev| {
            if matches!(
                ev,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                let s = app.state::<DesktopState>();
                if !s.shutdown_started.swap(true, Ordering::AcqRel) {
                    if let Ok(mut h) = s.helper.lock() {
                        let _ = h.stop(&s.paths.helper_data_dir);
                    };
                }
            }
        });
    }
}
