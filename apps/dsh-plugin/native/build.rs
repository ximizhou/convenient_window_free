// Share the desktop login state machine but register a distinct DSH startup entry.
// Substitutions are checked so a desktop ownership/migration change requires review.
use std::{env, fs, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-changed=ui/hud.html");
    println!("cargo:rerun-if-changed=ui/hud.js");
    let source = "../../desktop/src-tauri/src/windows_autostart.rs";
    println!("cargo:rerun-if-changed={source}");
    let mut code = fs::read_to_string(source).expect("desktop autostart source");
    for (old, new) in [
        (
            "pub const STARTUP_NAME: &str = \"Convenient Window\";",
            "pub const STARTUP_NAME: &str = \"Convenient Window (DSH)\";",
        ),
        (
            "const LEGACY_NAMES: [&str; 2] = [\"便捷窗口\", \"convenient-window\"];",
            "const LEGACY_NAMES: [&str; 0] = []; ",
        ),
    ] {
        assert_eq!(code.matches(old).count(), 1, "review DSH startup identity");
        code = code.replace(old, new);
    }
    let (runtime, _) = code
        .split_once("#[cfg(test)]")
        .expect("review desktop autostart test boundary");
    assert_eq!(
        runtime.matches("std::env::current_exe()?").count(),
        6,
        "review login ownership checks"
    );
    let code = runtime.replace("std::env::current_exe()?", "crate::login_executable()?");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("dsh_autostart.rs"),
        code,
    )
    .unwrap();
    tauri_build::build();
}
