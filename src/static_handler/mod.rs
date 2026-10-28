use std::fs;
use std::path::{Path, PathBuf};
use crate::config::model::ServerConfig;
use crate::http::mime::get_mime_type;
use crate::http::request::Request;
use crate::http::response::Response;
use crate::http::status::StatusCode;

pub struct StaticHandler;

impl StaticHandler {
    pub fn serve_file(file_path: &Path) -> Result<Response, (StatusCode, String)> {
        if !file_path.is_file() {
            return Err((
                StatusCode::NOT_FOUND,
                format!("File not found: {}", file_path.display()),
            ));
        }

        let bytes = fs::read(file_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                (StatusCode::FORBIDDEN, "Permission denied".to_string())
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read file: {}", e),
                )
            }
        })?;

        let mime = get_mime_type(file_path);
        let mut res = Response::new(StatusCode::OK);
        res.set_header("Content-Type", mime);
        res.set_body(bytes);
        Ok(res)
    }

    pub fn delete_file(file_path: &Path) -> Result<Response, (StatusCode, String)> {
        if !file_path.exists() {
            return Err((
                StatusCode::NOT_FOUND,
                format!("File not found: {}", file_path.display()),
            ));
        }

        if file_path.is_dir() {
            return Err((
                StatusCode::FORBIDDEN,
                "Cannot delete directory via DELETE method".to_string(),
            ));
        }

        fs::remove_file(file_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                (
                    StatusCode::FORBIDDEN,
                    "Permission denied to delete file".to_string(),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to delete file: {}", e),
                )
            }
        })?;

        let body = format!(
            "<!DOCTYPE html><html><body><h1>File Deleted</h1><p>Successfully deleted {}</p></body></html>",
            file_path.display()
        );
        let res = Response::html(StatusCode::OK, body);
        Ok(res)
    }

    pub fn generate_directory_listing(
        dir_path: &Path,
        url_path: &str,
    ) -> Result<Response, (StatusCode, String)> {
        let entries = fs::read_dir(dir_path).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read directory: {}", e),
            )
        })?;

        let mut items = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let metadata = entry.metadata().ok();
            let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
            items.push((name, is_dir, size));
        }

        // Sort: directories first, then alphabetical
        items.sort_by(|a, b| {
            if a.1 == b.1 {
                a.0.to_lowercase().cmp(&b.0.to_lowercase())
            } else if a.1 {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });

        let clean_url = if url_path.ends_with('/') {
            url_path.to_string()
        } else {
            format!("{}/", url_path)
        };

        let parent_link = if clean_url != "/" {
            r#"<tr><td><span class="icon">&#128193;</span> <a href="../">..</a></td><td>-</td><td>[Parent Directory]</td></tr>"#
        } else {
            ""
        };

        let mut table_rows = String::new();
        for (name, is_dir, size) in items {
            let icon = if is_dir { "&#128193;" } else { "&#128196;" };
            let href_suffix = if is_dir { format!("{}/", name) } else { name.clone() };
            let size_str = if is_dir {
                "-".to_string()
            } else {
                format_size(size)
            };
            let type_str = if is_dir { "Directory" } else { "File" };

            table_rows.push_str(&format!(
                r#"<tr><td><span class="icon">{icon}</span> <a href="{clean_url}{href_suffix}">{name}</a></td><td>{size_str}</td><td>{type_str}</td></tr>
"#
            ));
        }

        let html = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Index of {clean_url}</title>
    <style>
        :root {{
            --bg: #0f172a;
            --card: rgba(30, 41, 59, 0.7);
            --border: rgba(255, 255, 255, 0.1);
            --primary: #38bdf8;
            --text: #f8fafc;
            --muted: #94a3b8;
        }}
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            background: var(--bg);
            color: var(--text);
            padding: 2.5rem 1.5rem;
        }}
        .container {{
            max-width: 900px;
            margin: 0 auto;
            background: var(--card);
            backdrop-filter: blur(12px);
            border: 1px solid var(--border);
            border-radius: 1rem;
            padding: 2rem;
            box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.5);
        }}
        h1 {{
            font-size: 1.5rem;
            font-weight: 600;
            margin-bottom: 1.5rem;
            color: var(--primary);
            border-bottom: 1px solid var(--border);
            padding-bottom: 0.75rem;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
            text-align: left;
        }}
        th, td {{
            padding: 0.75rem 1rem;
            border-bottom: 1px solid rgba(255, 255, 255, 0.05);
        }}
        th {{
            color: var(--muted);
            font-size: 0.85rem;
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }}
        td {{
            font-size: 0.95rem;
        }}
        tr:hover td {{
            background: rgba(255, 255, 255, 0.03);
        }}
        a {{
            color: #38bdf8;
            text-decoration: none;
            transition: color 0.15s;
        }}
        a:hover {{
            color: #7dd3fc;
            text-decoration: underline;
        }}
        .icon {{
            margin-right: 0.5rem;
        }}
        .footer {{
            margin-top: 1.5rem;
            font-size: 0.8rem;
            color: var(--muted);
            text-align: right;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Index of {clean_url}</h1>
        <table>
            <thead>
                <tr><th>Name</th><th>Size</th><th>Type</th></tr>
            </thead>
            <tbody>
                {parent_link}
                {table_rows}
            </tbody>
        </table>
        <div class="footer">Localhost HTTP/1.1 Server &bull; Directory Listing</div>
    </div>
</body>
</html>"#
        );

        Ok(Response::html(StatusCode::OK, html))
    }

    pub fn handle_upload(
        upload_dir: &Path,
        req: &Request,
    ) -> Result<Response, (StatusCode, String)> {
        if !upload_dir.exists() {
            fs::create_dir_all(upload_dir).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to create upload directory: {}", e),
                )
            })?;
        }

        let content_type = req.get_header("content-type").unwrap_or("");

        if content_type.starts_with("multipart/form-data") {
            Self::handle_multipart_upload(upload_dir, content_type, req)
        } else {
            Self::handle_raw_upload(upload_dir, req)
        }
    }

    fn handle_multipart_upload(
        upload_dir: &Path,
        content_type: &str,
        req: &Request,
    ) -> Result<Response, (StatusCode, String)> {
        let boundary = content_type
            .split(';')
            .find_map(|part| {
                let part = part.trim();
                if part.starts_with("boundary=") {
                    Some(part["boundary=".len()..].trim_matches('"'))
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "Missing boundary in multipart/form-data".to_string(),
                )
            })?;

        let delimiter = format!("--{}", boundary).into_bytes();
        let body = &req.body;

        let mut saved_files = Vec::new();
        let mut pos = 0;

        while let Some(start_idx) = find_subsequence(&body[pos..], &delimiter) {
            let part_start = pos + start_idx + delimiter.len();
            pos = part_start;

            if pos + 2 <= body.len() && &body[pos..pos + 2] == b"--" {
                // End delimiter reached
                break;
            }

            // Skip CRLF after delimiter
            if pos + 2 <= body.len() && &body[pos..pos + 2] == b"\r\n" {
                pos += 2;
            }

            // Find headers end \r\n\r\n
            if let Some(header_end_offset) = find_subsequence(&body[pos..], b"\r\n\r\n") {
                let header_bytes = &body[pos..pos + header_end_offset];
                let headers_str = String::from_utf8_lossy(header_bytes);
                let data_start = pos + header_end_offset + 4;

                // Extract filename
                let mut filename = None;
                for line in headers_str.lines() {
                    if line.to_ascii_lowercase().starts_with("content-disposition:") {
                        for segment in line.split(';') {
                            let seg = segment.trim();
                            if seg.starts_with("filename=") {
                                let raw_fn = seg["filename=".len()..].trim_matches('"');
                                let clean_fn = Path::new(raw_fn)
                                    .file_name()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "uploaded_file".to_string());
                                filename = Some(clean_fn);
                            }
                        }
                    }
                }

                // Find next boundary
                let data_end = if let Some(next_b_offset) = find_subsequence(&body[data_start..], &delimiter) {
                    let mut end = data_start + next_b_offset;
                    // Trim trailing \r\n before delimiter
                    if end >= 2 && &body[end - 2..end] == b"\r\n" {
                        end -= 2;
                    }
                    pos = data_start + next_b_offset;
                    end
                } else {
                    pos = body.len();
                    body.len()
                };

                if let Some(fname) = filename {
                    if !fname.is_empty() {
                        let file_content = &body[data_start..data_end];
                        let dest_path = upload_dir.join(&fname);
                        fs::write(&dest_path, file_content).map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("Failed to write uploaded file '{}': {}", fname, e),
                            )
                        })?;
                        saved_files.push((fname, file_content.len()));
                    }
                }
            } else {
                break;
            }
        }

        if saved_files.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "No files found in multipart form upload".to_string(),
            ));
        }

        let mut files_html = String::new();
        for (f, sz) in &saved_files {
            files_html.push_str(&format!(
                "<li><strong>{}</strong> ({} bytes)</li>",
                f, sz
            ));
        }

        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head><title>Upload Successful</title>
<style>body {{ font-family: sans-serif; background: #0f172a; color: #f8fafc; padding: 2rem; }} a {{ color: #38bdf8; }}</style>
</head>
<body>
    <h1>Upload Successful!</h1>
    <p>Saved files:</p>
    <ul>{files_html}</ul>
    <p><a href="javascript:history.back()">&larr; Go Back</a></p>
</body>
</html>"#
        );

        let mut res = Response::html(StatusCode::CREATED, html);
        res.set_header("Location", format!("/upload/{}", saved_files[0].0));
        Ok(res)
    }

    fn handle_raw_upload(
        upload_dir: &Path,
        req: &Request,
    ) -> Result<Response, (StatusCode, String)> {
        let filename = req
            .query_params
            .get("file")
            .cloned()
            .or_else(|| {
                req.get_header("x-filename")
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| {
                format!(
                    "upload_{}.bin",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                )
            });

        // Strip path traversal characters
        let clean_name = Path::new(&filename)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "file.bin".to_string());

        let target_path = upload_dir.join(&clean_name);
        fs::write(&target_path, &req.body).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to write uploaded file: {}", e),
            )
        })?;

        let html = format!(
            r#"<!DOCTYPE html><html><body><h1>File Uploaded</h1><p>Saved {} ({} bytes)</p></body></html>"#,
            clean_name,
            req.body.len()
        );
        let mut res = Response::html(StatusCode::CREATED, html);
        res.set_header("Location", format!("/upload/{}", clean_name));
        Ok(res)
    }

    pub fn render_error_page(
        server: &ServerConfig,
        status: StatusCode,
        message: Option<&str>,
    ) -> Response {
        // Check if custom error page is configured
        if let Some(custom_path) = server.error_pages.get(&status) {
            let path = PathBuf::from(custom_path);
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(&path) {
                    let res = Response::html(status, content);
                    return res;
                }
            }
        }

        // Fallback to rich default error page
        Response::default_error(status, message)
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    for i in 0..=haystack.len() - needle.len() {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
