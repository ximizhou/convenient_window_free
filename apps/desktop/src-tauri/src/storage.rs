use serde_json::Value;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MAX_CONFIG_BYTES: usize = 900 * 1024;

pub fn read_json_with_backup(path: &Path) -> Result<Option<Value>, String> {
    match read_json(path) {
        Ok(Some(value)) => Ok(Some(value)),
        Ok(None) => read_json(&backup_path(path)),
        Err(primary_error) => match read_json(&backup_path(path)) {
            Ok(Some(value)) => Ok(Some(value)),
            Ok(None) => Err(primary_error),
            Err(backup_error) => Err(format!(
                "{primary_error}; backup {} also failed: {backup_error}",
                backup_path(path).display()
            )),
        },
    }
}

pub fn write_json_with_backup(path: &Path, value: &Value) -> Result<(), String> {
    if !value.is_object() {
        return Err("configuration root must be a JSON object".to_string());
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(format!(
            "configuration is {} bytes; limit is {MAX_CONFIG_BYTES}",
            bytes.len()
        ));
    }
    write_bytes_with_backup(path, &bytes)
}

pub fn read_config_file(path: &Path) -> Result<String, String> {
    validate_json_extension(path)?;
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err("selected path is not a regular file".to_string());
    }
    if metadata.len() > MAX_CONFIG_BYTES as u64 {
        return Err(format!(
            "configuration is {} bytes; limit is {MAX_CONFIG_BYTES}",
            metadata.len()
        ));
    }
    let content = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&content)
        .map_err(|error| format!("invalid JSON configuration: {error}"))?;
    if !value.is_object() {
        return Err("configuration root must be a JSON object".to_string());
    }
    Ok(content)
}

pub fn write_config_file(path: &Path, content: &str) -> Result<(), String> {
    validate_json_extension(path)?;
    if content.as_bytes().len() > MAX_CONFIG_BYTES {
        return Err(format!(
            "configuration is {} bytes; limit is {MAX_CONFIG_BYTES}",
            content.len()
        ));
    }
    let value: Value = serde_json::from_str(content)
        .map_err(|error| format!("invalid JSON configuration: {error}"))?;
    if !value.is_object() {
        return Err("configuration root must be a JSON object".to_string());
    }
    write_bytes_with_backup(path, content.as_bytes())
}

fn validate_json_extension(path: &Path) -> Result<(), String> {
    let is_json = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
    if !is_json {
        return Err("only .json configuration files are allowed".to_string());
    }
    if path.file_name().is_none() {
        return Err("configuration path has no file name".to_string());
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<Option<Value>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(format!(
            "{} exceeds the configuration size limit",
            path.display()
        ));
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| format!("cannot parse {}: {error}", path.display()))
}

fn write_bytes_with_backup(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "configuration path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;

    let temporary = temporary_path(path);
    let result = (|| {
        let mut file = File::create(&temporary)
            .map_err(|error| format!("cannot create {}: {error}", temporary.display()))?;
        file.write_all(bytes)
            .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("cannot flush {}: {error}", temporary.display()))?;
        drop(file);
        replace_file(path, &temporary, &backup_path(path))
    })();
    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(windows)]
fn replace_file(target: &Path, replacement: &Path, backup: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        ReplaceFileW, REPLACEFILE_IGNORE_MERGE_ERRORS, REPLACEFILE_WRITE_THROUGH,
    };

    if !target.exists() {
        return fs::rename(replacement, target)
            .map_err(|error| format!("cannot install {}: {error}", target.display()));
    }
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let target_wide = wide(target);
    let replacement_wide = wide(replacement);
    let backup_wide = wide(backup);
    unsafe {
        ReplaceFileW(
            PCWSTR(target_wide.as_ptr()),
            PCWSTR(replacement_wide.as_ptr()),
            PCWSTR(backup_wide.as_ptr()),
            REPLACEFILE_IGNORE_MERGE_ERRORS | REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
    }
    .map_err(|error| format!("cannot atomically replace {}: {error}", target.display()))
}

#[cfg(not(windows))]
fn replace_file(target: &Path, replacement: &Path, backup: &Path) -> Result<(), String> {
    if target.exists() {
        let _ = fs::remove_file(backup);
        fs::rename(target, backup)
            .map_err(|error| format!("cannot back up {}: {error}", target.display()))?;
    }
    if let Err(error) = fs::rename(replacement, target) {
        if backup.exists() {
            let _ = fs::rename(backup, target);
        }
        return Err(format!("cannot install {}: {error}", target.display()));
    }
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    append_suffix(path, ".bak")
}

fn temporary_path(path: &Path) -> PathBuf {
    append_suffix(path, ".writing")
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut filename = path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("configuration"));
    filename.push(suffix);
    path.with_file_name(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "convenient-window-desktop-storage-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn atomic_write_keeps_a_valid_backup() {
        let directory = test_dir();
        let path = directory.join("config.json");
        write_json_with_backup(&path, &serde_json::json!({ "value": 1 })).unwrap();
        write_json_with_backup(&path, &serde_json::json!({ "value": 2 })).unwrap();

        assert_eq!(
            read_json(&path).unwrap(),
            Some(serde_json::json!({ "value": 2 }))
        );
        assert_eq!(
            read_json(&backup_path(&path)).unwrap(),
            Some(serde_json::json!({ "value": 1 }))
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_primary_recovers_from_backup() {
        let directory = test_dir();
        let path = directory.join("config.json");
        write_json_with_backup(&path, &serde_json::json!({ "value": 1 })).unwrap();
        write_json_with_backup(&path, &serde_json::json!({ "value": 2 })).unwrap();
        fs::write(&path, "not-json").unwrap();

        assert_eq!(
            read_json_with_backup(&path).unwrap(),
            Some(serde_json::json!({ "value": 1 }))
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn import_rejects_non_json_paths_and_oversized_files() {
        let directory = test_dir();
        let text_path = directory.join("config.txt");
        fs::write(&text_path, "{}").unwrap();
        assert!(read_config_file(&text_path).unwrap_err().contains(".json"));

        let large_path = directory.join("large.json");
        fs::write(&large_path, vec![b' '; MAX_CONFIG_BYTES + 1]).unwrap();
        assert!(read_config_file(&large_path).unwrap_err().contains("limit"));
        fs::remove_dir_all(directory).unwrap();
    }
}
