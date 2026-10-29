//! Short-lived, ordinary-user startup management. This does not start the runtime.
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

#[cfg(windows)]
mod windows;

pub const MAX_REQUEST_BYTES: u64 = 16 * 1024;
pub const CAPABILITY: &str = "CW_STARTUP_MANAGER_PROTOCOL_V1";

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum Request {
    List {},
    SetEnabled {
        id: String,
        revision: String,
        enabled: bool,
    },
    Add {
        #[serde(rename = "executablePath")]
        executable_path: String,
        name: Option<String>,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub name: String,
    pub command: String,
    pub source: String,
    pub enabled: Option<bool>,
    pub editable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub revision: String,
}

#[derive(Debug, Serialize)]
pub struct Failure {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub ok: bool,
    pub version: u8,
    pub capability: &'static str,
    pub items: Vec<Item>,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Failure>,
}

impl Response {
    pub fn failure(code: &str, message: impl ToString) -> Self {
        Self {
            ok: false,
            version: 1,
            capability: CAPABILITY,
            items: vec![],
            warnings: vec![],
            error: Some(Failure {
                code: code.into(),
                message: message.to_string(),
            }),
        }
    }
}

pub fn parse_request(bytes: &[u8]) -> Result<Request, Response> {
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        return Err(Response::failure(
            "invalidRequest",
            "Request exceeds 16 KiB",
        ));
    }
    serde_json::from_slice(bytes)
        .map_err(|_| Response::failure("invalidRequest", "Invalid startup request"))
}

/// The caller owns a pipe and closes stdin after exactly one request.
pub fn run_cli() -> anyhow::Result<()> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let mut response = match parse_request(&bytes) {
        Ok(request) => dispatch(request),
        Err(response) => response,
    };
    let mut output = serde_json::to_vec(&response)?;
    while output.len() > 1024 * 1024 && !response.items.is_empty() {
        response.items.pop();
        if !response.warnings.iter().any(|v| v == "itemLimit") {
            response.warnings.push("itemLimit".into());
        }
        output = serde_json::to_vec(&response)?;
    }
    std::io::stdout().lock().write_all(&output)?;
    std::io::stdout().flush()?;
    Ok(())
}

fn dispatch(request: Request) -> Response {
    #[cfg(windows)]
    {
        match crate::windows_process::current_elevated() {
            Ok(false) => windows::execute(request),
            Ok(true) => Response::failure(
                "ordinaryUserRequired",
                "Open the app without administrator privileges",
            ),
            Err(error) => Response::failure("identityUnavailable", error),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = request;
        Response::failure("unsupported", "Startup management is available on Windows")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_have_a_closed_schema_and_bounded_size() {
        assert!(parse_request(br#"{"action":"list"}"#).is_ok());
        assert!(parse_request(br#"{"action":"list","registryPath":"HKLM"}"#).is_err());
        assert!(parse_request(
            br#"{"action":"setEnabled","id":"userRun:a","revision":"x","enabled":false}"#
        )
        .is_ok());
        assert!(
            parse_request(br#"{"action":"setEnabled","id":"userRun:a","enabled":false}"#).is_err()
        );
        assert!(parse_request(&vec![b' '; MAX_REQUEST_BYTES as usize + 1]).is_err());
    }
}
