use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn initialize() -> Result<()> {
    let executable = std::env::current_exe().context("resolve helper executable path")?;
    let args = std::env::args_os().collect::<Vec<_>>();
    let directory = resolve_data_dir(&args, &executable)?;
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("create helper data directory {}", directory.display()))?;
    let _ = DATA_DIR.set(directory);
    Ok(())
}

pub fn data_file(name: &str) -> Option<PathBuf> {
    DATA_DIR
        .get()
        .cloned()
        .or_else(|| {
            std::env::current_exe()
                .ok()?
                .parent()
                .map(Path::to_path_buf)
        })
        .map(|directory| directory.join(name))
}

fn resolve_data_dir(args: &[OsString], executable: &Path) -> Result<PathBuf> {
    let mut index = 1;
    while index < args.len() {
        if args[index] == "--data-dir" {
            let Some(value) = args.get(index + 1) else {
                bail!("--data-dir requires an absolute directory path");
            };
            let directory = PathBuf::from(value);
            if !directory.is_absolute() {
                bail!("--data-dir must be absolute: {}", directory.display());
            }
            return Ok(directory);
        }
        index += 1;
    }

    executable
        .parent()
        .map(Path::to_path_buf)
        .context("helper executable directory is unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_data_directory_is_separate_from_versioned_binary() {
        let data_directory = native_path("ConvenientWindow", "data");
        let executable = native_path("helper", "0.3.0").join(executable_name());
        let args = vec![
            OsString::from(executable_name()),
            OsString::from("--data-dir"),
            data_directory.clone().into_os_string(),
        ];

        assert_eq!(
            resolve_data_dir(&args, &executable).unwrap(),
            data_directory
        );
    }

    #[test]
    fn development_build_falls_back_to_executable_directory() {
        let executable = native_path("project", "helper").join(executable_name());
        assert_eq!(
            resolve_data_dir(&[OsString::from(executable_name())], &executable).unwrap(),
            executable.parent().unwrap().to_path_buf()
        );
    }

    fn executable_name() -> &'static str {
        if cfg!(windows) {
            "helper.exe"
        } else {
            "helper"
        }
    }

    fn native_path(first: &str, second: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\AppData").join(first).join(second)
        } else {
            PathBuf::from("/tmp").join(first).join(second)
        }
    }

    #[test]
    fn relative_data_directory_is_rejected() {
        let args = vec![
            OsString::from("helper.exe"),
            OsString::from("--data-dir"),
            OsString::from("data"),
        ];

        assert!(resolve_data_dir(&args, Path::new(r"C:\helper.exe")).is_err());
    }
}
