use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use crate::config::model::ServerConfig;
use crate::http::request::Request;
use crate::http::response::Response;
use crate::http::status::StatusCode;

pub struct CgiExecutor;

impl CgiExecutor {
    pub fn execute(
        script_path: &Path,
        interpreter: &str,
        path_info: &str,
        server: &ServerConfig,
        port: u16,
        client_ip: &str,
        req: &Request,
    ) -> Result<Response, (StatusCode, String)> {
        if !script_path.exists() {
            return Err((
                StatusCode::NOT_FOUND,
                format!("CGI script not found: {}", script_path.display()),
            ));
        }

        let script_dir = script_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let script_file_name = script_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let mut cmd = Command::new(interpreter);
        cmd.arg(script_file_name);
        cmd.current_dir(&script_dir);

        // Standard CGI environment variables
        cmd.env("GATEWAY_INTERFACE", "CGI/1.1");
        cmd.env("SERVER_PROTOCOL", "HTTP/1.1");
        cmd.env("SERVER_SOFTWARE", "localhost/1.0");
        cmd.env("REQUEST_METHOD", req.method.as_str());
        cmd.env("QUERY_STRING", req.query_string.as_deref().unwrap_or(""));
        cmd.env("CONTENT_LENGTH", req.body.len().to_string());
        cmd.env(
            "CONTENT_TYPE",
            req.get_header("content-type").unwrap_or(""),
        );
        cmd.env("PATH_INFO", path_info);
        cmd.env(
            "PATH_TRANSLATED",
            script_path.to_str().unwrap_or(""),
        );
        cmd.env("SCRIPT_NAME", script_file_name);
        cmd.env("SCRIPT_FILENAME", script_path.to_str().unwrap_or(""));
        cmd.env("SERVER_NAME", server.server_names.first().map(|s| s.as_str()).unwrap_or(&server.host));
        cmd.env("SERVER_PORT", port.to_string());
        cmd.env("REMOTE_ADDR", client_ip);

        // Forward HTTP headers as HTTP_* environment variables
        for (k, v) in req.headers.iter() {
            let env_key = format!("HTTP_{}", k.to_ascii_uppercase().replace('-', "_"));
            cmd.env(env_key, v);
        }

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to spawn CGI process '{}': {}", interpreter, e),
            )
        })?;

        // Write request body to stdin and close stdin (EOF)
        if let Some(mut stdin) = child.stdin.take() {
            if !req.body.is_empty() {
                let _ = stdin.write_all(&req.body);
            }
            // stdin is dropped here, closing the pipe and sending EOF
        }

        // Wait with timeout (e.g. 5 seconds) to prevent infinite loops
        let timeout = Duration::from_secs(5);
        let start = Instant::now();

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let output = child.wait_with_output().map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Failed to read CGI output: {}", e),
                        )
                    })?;

                    if !status.success() {
                        let stderr_msg = String::from_utf8_lossy(&output.stderr);
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("CGI exited with error status {}: {}", status, stderr_msg),
                        ));
                    }

                    return parse_cgi_output(&output.stdout);
                }
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        return Err((
                            StatusCode::GATEWAY_TIMEOUT,
                            "CGI process timed out after 5 seconds".to_string(),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Error waiting for CGI process: {}", e),
                    ));
                }
            }
        }
    }
}

pub fn parse_cgi_output(output: &[u8]) -> Result<Response, (StatusCode, String)> {
    // Find double CRLF or double LF separating headers from body
    let mut header_end = None;
    let mut delim_len = 0;

    for i in 0..output.len().saturating_sub(3) {
        if &output[i..i + 4] == b"\r\n\r\n" {
            header_end = Some(i);
            delim_len = 4;
            break;
        }
    }

    if header_end.is_none() {
        for i in 0..output.len().saturating_sub(1) {
            if &output[i..i + 2] == b"\n\n" {
                header_end = Some(i);
                delim_len = 2;
                break;
            }
        }
    }

    let (headers_bytes, body_bytes) = match header_end {
        Some(idx) => (&output[..idx], &output[idx + delim_len..]),
        None => {
            // Output might be raw HTML without headers
            (&[][..], output)
        }
    };

    let mut response_status = StatusCode::OK;
    let mut custom_headers = Vec::new();
    let mut cookies = Vec::new();

    let headers_str = String::from_utf8_lossy(headers_bytes);
    for line in headers_str.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim();
            let value = trimmed[colon_pos + 1..].trim();

            if key.eq_ignore_ascii_case("status") {
                // e.g. "Status: 404 Not Found" or "Status: 200"
                if let Some(code_str) = value.split_whitespace().next() {
                    if let Ok(code) = code_str.parse::<u16>() {
                        response_status = StatusCode(code);
                    }
                }
            } else if key.eq_ignore_ascii_case("set-cookie") {
                cookies.push(value.to_string());
            } else {
                custom_headers.push((key.to_string(), value.to_string()));
            }
        }
    }

    let mut res = Response::new(response_status);
    for (k, v) in custom_headers {
        res.set_header(k, v);
    }
    for cookie in cookies {
        res.set_cookie(cookie);
    }

    if !res.headers.contains_key("content-type") {
        res.set_header("Content-Type", "text/html; charset=utf-8");
    }

    res.set_body(body_bytes.to_vec());
    Ok(res)
}
