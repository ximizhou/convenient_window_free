use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

pub fn write_line(message: impl AsRef<str>) {
    let Some(path) = log_path() else {
        return;
    };

    static LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let Ok(_guard) = LOG_LOCK.get_or_init(|| Mutex::new(())).lock() else {
        return;
    };
    rotate_if_needed(&path);

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(
            file,
            "{} pid={} {}",
            timestamp_ms(),
            std::process::id(),
            message.as_ref()
        );
    }
}

fn rotate_if_needed(path: &std::path::Path) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() < MAX_LOG_BYTES {
        return;
    }

    let rotated = append_suffix(path, ".1");
    let _ = fs::remove_file(&rotated);
    let _ = fs::rename(path, rotated);
}

fn append_suffix(path: &std::path::Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("magic-corners-helper.log"));
    name.push(suffix);
    path.with_file_name(name)
}

fn log_path() -> Option<PathBuf> {
    crate::paths::data_file("magic-corners-helper.log")
}

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
