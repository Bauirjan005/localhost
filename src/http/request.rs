use std::collections::HashMap;
use super::headers::Headers;
use super::method::Method;
use super::status::StatusCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    pub raw_uri: String,
    pub path: String,
    pub query_string: Option<String>,
    pub query_params: HashMap<String, String>,
    pub version: String,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub cookies: HashMap<String, String>,
}

impl Request {
    pub fn get_header(&self, key: &str) -> Option<&str> {
        self.headers.get(key)
    }

    pub fn host(&self) -> Option<&str> {
        self.headers.get("host")
    }

    pub fn content_length(&self) -> Option<usize> {
        self.headers
            .get("content-length")
            .and_then(|val| val.trim().parse::<usize>().ok())
    }

    pub fn is_chunked(&self) -> bool {
        self.headers
            .get("transfer-encoding")
            .map(|val| val.to_ascii_lowercase().contains("chunked"))
            .unwrap_or(false)
    }

    pub fn is_keep_alive(&self) -> bool {
        if let Some(conn) = self.headers.get("connection") {
            let lower = conn.to_ascii_lowercase();
            if lower.contains("close") {
                return false;
            }
            if lower.contains("keep-alive") {
                return true;
            }
        }
        // HTTP/1.1 defaults to keep-alive
        self.version == "HTTP/1.1"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseState {
    RequestLine,
    Headers,
    BodyFixed { expected_len: usize },
    BodyChunkSize,
    BodyChunkData { chunk_len: usize },
    BodyChunkTrailer,
    Complete,
}

pub struct RequestParser {
    pub state: ParseState,
    buffer: Vec<u8>,
    max_body_size: usize,

    // Extracted so far
    method: Option<Method>,
    raw_uri: Option<String>,
    path: Option<String>,
    query_string: Option<String>,
    query_params: HashMap<String, String>,
    version: Option<String>,
    headers: Headers,
    body: Vec<u8>,
}

impl RequestParser {
    pub fn new(max_body_size: usize) -> Self {
        RequestParser {
            state: ParseState::RequestLine,
            buffer: Vec::with_capacity(4096),
            max_body_size,
            method: None,
            raw_uri: None,
            path: None,
            query_string: None,
            query_params: HashMap::new(),
            version: None,
            headers: Headers::new(),
            body: Vec::new(),
        }
    }

    pub fn update_max_body_size(&mut self, new_limit: usize) {
        self.max_body_size = new_limit;
    }

    /// Feed new data received from socket.
    /// Returns:
    /// - Ok(Some(Request)) when a full request is complete.
    /// - Ok(None) when more data is needed.
    /// - Err((StatusCode, error_msg)) when parsing fails or payload is too large.
    pub fn feed(&mut self, data: &[u8]) -> Result<Option<Request>, (StatusCode, String)> {
        self.buffer.extend_from_slice(data);

        loop {
            match self.state {
                ParseState::RequestLine => {
                    if let Some(line_end) = find_crlf(&self.buffer) {
                        let line_bytes = &self.buffer[..line_end];
                        let line = match std::str::from_utf8(line_bytes) {
                            Ok(s) => s.trim_end(),
                            Err(_) => {
                                return Err((StatusCode::BAD_REQUEST, "Invalid UTF-8 in request line".into()))
                            }
                        };

                        if line.is_empty() {
                            // Skip leading empty lines per RFC 7230 §3.5
                            self.buffer.drain(..line_end + 2);
                            continue;
                        }

                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() != 3 {
                            return Err((
                                StatusCode::BAD_REQUEST,
                                format!("Malformed request line: {}", line),
                            ));
                        }

                        let method = match parts[0].parse::<Method>() {
                            Ok(m) => m,
                            Err(_) => {
                                return Err((
                                    StatusCode::METHOD_NOT_ALLOWED,
                                    format!("Unsupported method: {}", parts[0]),
                                ))
                            }
                        };

                        let raw_uri = parts[1].to_string();
                        let (path, query_str, query_params) = parse_uri(&raw_uri)?;
                        let version = parts[2].to_string();

                        if version != "HTTP/1.1" && version != "HTTP/1.0" {
                            return Err((
                                StatusCode::HTTP_VERSION_NOT_SUPPORTED,
                                format!("Unsupported HTTP version: {}", version),
                            ));
                        }

                        self.method = Some(method);
                        self.raw_uri = Some(raw_uri);
                        self.path = Some(path);
                        self.query_string = query_str;
                        self.query_params = query_params;
                        self.version = Some(version);

                        self.buffer.drain(..line_end + 2);
                        self.state = ParseState::Headers;
                    } else {
                        // Need more data for request line
                        if self.buffer.len() > 8192 {
                            return Err((
                                StatusCode::URI_TOO_LONG,
                                "Request line exceeded limit".into(),
                            ));
                        }
                        return Ok(None);
                    }
                }

                ParseState::Headers => {
                    let mut found_header = false;
                    while let Some(line_end) = find_crlf(&self.buffer) {
                        found_header = true;
                        let line_bytes = &self.buffer[..line_end];
                        let line = match std::str::from_utf8(line_bytes) {
                            Ok(s) => s.trim_end(),
                            Err(_) => {
                                return Err((StatusCode::BAD_REQUEST, "Invalid UTF-8 in headers".into()))
                            }
                        };

                        if line.is_empty() {
                            // End of headers!
                            self.buffer.drain(..line_end + 2);

                            // Validate Host header for HTTP/1.1
                            if self.version.as_deref() == Some("HTTP/1.1")
                                && !self.headers.contains_key("host")
                            {
                                return Err((
                                    StatusCode::BAD_REQUEST,
                                    "HTTP/1.1 requires Host header".into(),
                                ));
                            }

                            // Determine next state
                            if let Some(te) = self.headers.get("transfer-encoding") {
                                if te.to_ascii_lowercase().contains("chunked") {
                                    self.state = ParseState::BodyChunkSize;
                                    break;
                                }
                            }

                            if let Some(cl_str) = self.headers.get("content-length") {
                                let cl = match cl_str.trim().parse::<usize>() {
                                    Ok(n) => n,
                                    Err(_) => {
                                        return Err((
                                            StatusCode::BAD_REQUEST,
                                            "Invalid Content-Length".into(),
                                        ))
                                    }
                                };

                                if cl > self.max_body_size {
                                    return Err((
                                        StatusCode::PAYLOAD_TOO_LARGE,
                                        format!("Body size {} exceeds limit {}", cl, self.max_body_size),
                                    ));
                                }

                                if cl == 0 {
                                    self.state = ParseState::Complete;
                                } else {
                                    self.state = ParseState::BodyFixed { expected_len: cl };
                                }
                                break;
                            }

                            // No body expected
                            self.state = ParseState::Complete;
                            break;
                        }

                        // Parse header key: value
                        if let Some(colon_pos) = line.find(':') {
                            let key = line[..colon_pos].trim();
                            let value = line[colon_pos + 1..].trim();
                            if key.is_empty() {
                                return Err((
                                    StatusCode::BAD_REQUEST,
                                    "Empty header name".into(),
                                ));
                            }
                            self.headers.append_or_insert(key, value);
                        } else {
                            return Err((
                                StatusCode::BAD_REQUEST,
                                format!("Malformed header: {}", line),
                            ));
                        }

                        self.buffer.drain(..line_end + 2);
                    }

                    if !found_header {
                        if self.buffer.len() > 32768 {
                            return Err((
                                StatusCode::BAD_REQUEST,
                                "Headers section exceeded maximum size".into(),
                            ));
                        }
                        return Ok(None);
                    }
                }

                ParseState::BodyFixed { expected_len } => {
                    let needed = expected_len - self.body.len();
                    let take = std::cmp::min(needed, self.buffer.len());
                    self.body.extend_from_slice(&self.buffer[..take]);
                    self.buffer.drain(..take);

                    if self.body.len() > self.max_body_size {
                        return Err((
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "Body exceeded maximum body size".into(),
                        ));
                    }

                    if self.body.len() == expected_len {
                        self.state = ParseState::Complete;
                    } else {
                        return Ok(None);
                    }
                }

                ParseState::BodyChunkSize => {
                    if let Some(line_end) = find_crlf(&self.buffer) {
                        let line_bytes = &self.buffer[..line_end];
                        let line = match std::str::from_utf8(line_bytes) {
                            Ok(s) => s.trim(),
                            Err(_) => {
                                return Err((
                                    StatusCode::BAD_REQUEST,
                                    "Invalid UTF-8 in chunk size".into(),
                                ))
                            }
                        };

                        // Chunk extension may be present: e.g. "1a; name=val"
                        let size_part = line.split(';').next().unwrap_or("").trim();
                        let chunk_len = match usize::from_str_radix(size_part, 16) {
                            Ok(n) => n,
                            Err(_) => {
                                return Err((
                                    StatusCode::BAD_REQUEST,
                                    format!("Invalid chunk size hex: {}", size_part),
                                ))
                            }
                        };

                        self.buffer.drain(..line_end + 2);

                        if chunk_len == 0 {
                            // Final chunk!
                            self.state = ParseState::BodyChunkTrailer;
                        } else {
                            if self.body.len() + chunk_len > self.max_body_size {
                                return Err((
                                    StatusCode::PAYLOAD_TOO_LARGE,
                                    "Chunked body exceeds max body size".into(),
                                ));
                            }
                            self.state = ParseState::BodyChunkData { chunk_len };
                        }
                    } else {
                        return Ok(None);
                    }
                }

                ParseState::BodyChunkData { chunk_len } => {
                    // We need chunk_len bytes + \r\n (2 bytes)
                    if self.buffer.len() >= chunk_len + 2 {
                        let chunk_data = &self.buffer[..chunk_len];
                        self.body.extend_from_slice(chunk_data);

                        // Ensure terminating CRLF
                        if &self.buffer[chunk_len..chunk_len + 2] != b"\r\n" {
                            return Err((
                                StatusCode::BAD_REQUEST,
                                "Malformed chunk: missing CRLF after data".into(),
                            ));
                        }

                        self.buffer.drain(..chunk_len + 2);
                        self.state = ParseState::BodyChunkSize;
                    } else {
                        return Ok(None);
                    }
                }

                ParseState::BodyChunkTrailer => {
                    // Read trailers until empty line \r\n
                    if let Some(line_end) = find_crlf(&self.buffer) {
                        let line_bytes = &self.buffer[..line_end];
                        if line_bytes.is_empty() {
                            self.buffer.drain(..line_end + 2);
                            self.state = ParseState::Complete;
                        } else {
                            // Optional trailer line, discard or parse
                            self.buffer.drain(..line_end + 2);
                        }
                    } else {
                        return Ok(None);
                    }
                }

                ParseState::Complete => {
                    let cookies = self.headers.parse_cookies();
                    let req = Request {
                        method: self.method.take().unwrap(),
                        raw_uri: self.raw_uri.take().unwrap(),
                        path: self.path.take().unwrap(),
                        query_string: self.query_string.take(),
                        query_params: std::mem::take(&mut self.query_params),
                        version: self.version.take().unwrap(),
                        headers: std::mem::take(&mut self.headers),
                        body: std::mem::take(&mut self.body),
                        cookies,
                    };
                    return Ok(Some(req));
                }
            }
        }
    }

    /// Reset parser state for HTTP keep-alive connection on same socket.
    pub fn reset_for_keep_alive(&mut self) {
        self.state = ParseState::RequestLine;
        self.method = None;
        self.raw_uri = None;
        self.path = None;
        self.query_string = None;
        self.query_params.clear();
        self.version = None;
        self.headers = Headers::new();
        self.body.clear();
        // Keep self.buffer in case pipelined bytes arrived!
    }
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    for i in 0..buf.len().saturating_sub(1) {
        if buf[i] == b'\r' && buf[i + 1] == b'\n' {
            return Some(i);
        }
    }
    None
}

/// Parses URI into (normalized_decoded_path, query_string, query_params)
pub fn parse_uri(
    uri: &str,
) -> Result<(String, Option<String>, HashMap<String, String>), (StatusCode, String)> {
    let (path_part, query_part) = match uri.find('?') {
        Some(pos) => (&uri[..pos], Some(&uri[pos + 1..])),
        None => (uri, None),
    };

    let decoded_path = percent_decode(path_part)?;
    let normalized = normalize_path(&decoded_path);

    let mut query_params = HashMap::new();
    let query_string = if let Some(q) = query_part {
        for pair in q.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (k, v) = match pair.find('=') {
                Some(idx) => (&pair[..idx], &pair[idx + 1..]),
                None => (pair, ""),
            };
            let decoded_k = percent_decode(k).unwrap_or_else(|_| k.to_string());
            let decoded_v = percent_decode(&v.replace('+', " ")).unwrap_or_else(|_| v.to_string());
            query_params.insert(decoded_k, decoded_v);
        }
        Some(q.to_string())
    } else {
        None
    };

    Ok((normalized, query_string, query_params))
}

pub fn percent_decode(input: &str) -> Result<String, (StatusCode, String)> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut chars = input.bytes();

    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars
                .next()
                .ok_or((StatusCode::BAD_REQUEST, "Invalid hex in URL encoding".into()))?;
            let h2 = chars
                .next()
                .ok_or((StatusCode::BAD_REQUEST, "Invalid hex in URL encoding".into()))?;
            let hex_bytes = [h1, h2];
            let hex_str = std::str::from_utf8(&hex_bytes)
                .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid hex byte in URL".into()))?;
            let val = u8::from_str_radix(hex_str, 16)
                .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid hex value in URL".into()))?;
            bytes.push(val);
        } else {
            bytes.push(b);
        }
    }

    String::from_utf8(bytes).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid UTF-8 after percent decoding".into(),
        )
    })
}

/// Normalizes path to prevent directory traversal (`/../`, `/./`, double slashes)
pub fn normalize_path(path: &str) -> String {
    let mut segments = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                segments.pop();
            }
            s => segments.push(s),
        }
    }

    if segments.is_empty() {
        return "/".to_string();
    }

    let mut res = String::new();
    for seg in segments {
        res.push('/');
        res.push_str(seg);
    }

    // Preserve trailing slash if original had it and it's not root
    if path.ends_with('/') && !res.ends_with('/') {
        res.push('/');
    }

    res
}
