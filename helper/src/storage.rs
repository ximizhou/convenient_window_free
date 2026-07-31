use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn read_json_with_backup<T: DeserializeOwned>(path: &Path) -> Result<Option<(T, bool)>> {
    match read_json_file(path) {
        Ok(Some(value)) => Ok(Some((value, false))),
        Ok(None) => read_backup(path),
        Err(primary_error) => match read_backup(path) {
            Ok(Some(value)) => Ok(Some(value)),
            Ok(None) => Err(primary_error),
            Err(backup_error) => Err(primary_error.context(format!(
                "backup {} also failed: {backup_error:#}",
                backup_path(path).display()
            ))),
        },
    }
}

pub fn write_json_with_backup<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).context("serialize json")?;
    write_bytes_with_backup(path, &bytes)
}

fn read_backup<T: DeserializeOwned>(path: &Path) -> Result<Option<(T, bool)>> {
    read_json_file(&backup_path(path)).map(|value| value.map(|value| (value, true)))
}

fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    serde_json::from_slice(&bytes)
        .with_context(|| format!("parse {}", path.display()))
        .map(Some)
}

pub fn write_bytes_with_backup(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("data file has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;

    let temporary = temporary_path(path);
    let result = (|| {
        let mut file =
            File::create(&temporary).with_context(|| format!("create {}", temporary.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("write {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("flush {}", temporary.display()))?;
        drop(file);

        replace_file(path, &temporary, &backup_path(path))
    })();

    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(windows)]
fn replace_file(target: &Path, replacement: &Path, backup: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        ReplaceFileW, REPLACEFILE_IGNORE_MERGE_ERRORS, REPLACEFILE_WRITE_THROUGH,
    };

    if !target.exists() {
        return fs::rename(replacement, target)
            .with_context(|| format!("install {}", target.display()));
    }

    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replacement_wide = replacement
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let backup_wide = backup
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

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
    .with_context(|| format!("atomically replace {}", target.display()))
}

#[cfg(not(windows))]
fn replace_file(target: &Path, replacement: &Path, backup: &Path) -> Result<()> {
    if target.exists() {
        let _ = fs::remove_file(backup);
        fs::rename(target, backup).with_context(|| format!("backup {}", target.display()))?;
    }
    if let Err(error) = fs::rename(replacement, target) {
        if backup.exists() {
            let _ = fs::rename(backup, target);
        }
        return Err(error).with_context(|| format!("install {}", target.display()));
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
        .unwrap_or_else(|| OsString::from("data"));
    filename.push(suffix);
    path.with_file_name(filename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct Fixture {
        value: u32,
    }

    fn test_dir() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("magic-corners-storage-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn atomic_write_keeps_previous_valid_value_as_backup() {
        let directory = test_dir();
        let path = directory.join("config.json");
        write_json_with_backup(&path, &Fixture { value: 1 }).unwrap();
        write_json_with_backup(&path, &Fixture { value: 2 }).unwrap();

        assert_eq!(
            read_json_file::<Fixture>(&path).unwrap(),
            Some(Fixture { value: 2 })
        );
        assert_eq!(
            read_json_file::<Fixture>(&backup_path(&path)).unwrap(),
            Some(Fixture { value: 1 })
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_primary_recovers_from_backup() {
        let directory = test_dir();
        let path = directory.join("config.json");
        write_json_with_backup(&path, &Fixture { value: 1 }).unwrap();
        write_json_with_backup(&path, &Fixture { value: 2 }).unwrap();
        fs::write(&path, b"not-json").unwrap();

        let (value, recovered) = read_json_with_backup::<Fixture>(&path).unwrap().unwrap();
        assert_eq!(value, Fixture { value: 1 });
        assert!(recovered);
        fs::remove_dir_all(directory).unwrap();
    }
}
