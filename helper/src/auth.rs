use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const TOKEN_FILE_NAME: &str = "auth-token";

pub fn load_or_create_token() -> Result<String> {
    let path = token_path().context("helper executable directory is unavailable")?;
    load_or_create_token_at(&path)
}

fn load_or_create_token_at(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(token) => {
            let token = token.trim();
            if is_valid_token(token) {
                return Ok(token.to_string());
            }
            crate::logging::write_line(format!(
                "auth: replacing invalid authentication token in {}",
                path.display()
            ));
            create_token(path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => create_token(path),
        Err(error) => {
            Err(error).with_context(|| format!("read authentication token from {}", path.display()))
        }
    }
}

fn create_token(path: &Path) -> Result<String> {
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    crate::storage::write_bytes_with_backup(path, token.as_bytes())
        .with_context(|| format!("write authentication token to {}", path.display()))?;
    Ok(token)
}

fn token_path() -> Option<PathBuf> {
    crate::paths::data_file(TOKEN_FILE_NAME)
}

fn is_valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn token_format_requires_256_bits_of_hex_data() {
        assert!(is_valid_token(&"a".repeat(64)));
        assert!(!is_valid_token(&"a".repeat(63)));
        assert!(!is_valid_token(&format!("{}g", "a".repeat(63))));
    }

    #[test]
    fn invalid_token_is_replaced_atomically() {
        let directory =
            std::env::temp_dir().join(format!("magic-corners-auth-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(TOKEN_FILE_NAME);
        fs::write(&path, "partial-token").unwrap();

        let token = load_or_create_token_at(&path).unwrap();

        assert!(is_valid_token(&token));
        assert_eq!(fs::read_to_string(&path).unwrap(), token);
        fs::remove_dir_all(directory).unwrap();
    }
}
