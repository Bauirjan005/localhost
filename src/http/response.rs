use std::time::SystemTime;
use super::headers::Headers;
use super::status::StatusCode;

#[derive(Debug, Clone)]
pub struct Response {
    pub status: StatusCode,
    pub version: String,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub cookies_to_set: Vec<String>,
}

impl Response {
    pub fn new(status: StatusCode) -> Self {
        let mut headers = Headers::new();
        headers.insert("Server", "localhost/1.0 (Rust)");
        headers.insert("Date", http_date());
        headers.insert("Connection", "keep-alive");

        Response {
            status,
            version: "HTTP/1.1".to_string(),
            headers,
            body: Vec::new(),
            cookies_to_set: Vec::new(),
        }
    }

    pub fn set_header(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.headers.insert(key, value);
        self
    }

    pub fn set_cookie(&mut self, cookie_directive: impl Into<String>) -> &mut Self {
        self.cookies_to_set.push(cookie_directive.into());
        self
    }

    pub fn set_connection_close(&mut self) -> &mut Self {
        self.headers.insert("Connection", "close");
        self
    }

    pub fn set_body(&mut self, body: impl Into<Vec<u8>>) -> &mut Self {
        self.body = body.into();
        self.headers.insert("Content-Length", self.body.len().to_string());
        self
    }

    pub fn html(status: StatusCode, body: impl Into<String>) -> Self {
        let mut res = Response::new(status);
        let bytes = body.into().into_bytes();
        res.set_header("Content-Type", "text/html; charset=utf-8");
        res.set_body(bytes);
        res
    }

    pub fn text(status: StatusCode, body: impl Into<String>) -> Self {
        let mut res = Response::new(status);
        let bytes = body.into().into_bytes();
        res.set_header("Content-Type", "text/plain; charset=utf-8");
        res.set_body(bytes);
        res
    }

    pub fn json(status: StatusCode, body: impl Into<String>) -> Self {
        let mut res = Response::new(status);
        let bytes = body.into().into_bytes();
        res.set_header("Content-Type", "application/json; charset=utf-8");
        res.set_body(bytes);
        res
    }

    pub fn redirect(status: StatusCode, location: &str) -> Self {
        let mut res = Response::new(status);
        res.set_header("Location", location);
        let body = format!(
            "<!DOCTYPE html><html><head><title>Redirecting</title></head><body>Redirecting to <a href=\"{}\">{}</a></body></html>",
            location, location
        );
        res.set_header("Content-Type", "text/html; charset=utf-8");
        res.set_body(body.into_bytes());
        res
    }

    pub fn default_error(status: StatusCode, message: Option<&str>) -> Self {
        let reason = status.reason_phrase();
        let code = status.as_u16();
        let detail = message.unwrap_or(reason);

        let html = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{code} {reason} - Localhost</title>
    <style>
        :root {{
            --bg: #0b0f19;
            --card-bg: rgba(23, 32, 54, 0.75);
            --border: rgba(255, 255, 255, 0.1);
            --primary: #38bdf8;
            --accent: #f43f5e;
            --text-main: #f8fafc;
            --text-muted: #94a3b8;
        }}
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            background: var(--bg);
            background-image: radial-gradient(circle at 50% 20%, rgba(56, 189, 248, 0.12) 0%, transparent 60%);
            color: var(--text-main);
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 1.5rem;
        }}
        .card {{
            background: var(--card-bg);
            backdrop-filter: blur(16px);
            border: 1px solid var(--border);
            border-radius: 1.5rem;
            padding: 3rem 2.5rem;
            max-width: 520px;
            width: 100%;
            text-align: center;
            box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.6);
        }}
        .status-badge {{
            display: inline-flex;
            align-items: center;
            gap: 0.5rem;
            font-size: 4rem;
            font-weight: 800;
            letter-spacing: -2px;
            background: linear-gradient(135deg, #38bdf8 0%, #818cf8 100%);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            margin-bottom: 0.5rem;
        }}
        h1 {{
            font-size: 1.5rem;
            font-weight: 600;
            margin-bottom: 1rem;
            color: #e2e8f0;
        }}
        p {{
            color: var(--text-muted);
            font-size: 1rem;
            line-height: 1.6;
            margin-bottom: 2rem;
            word-break: break-word;
        }}
        .btn {{
            display: inline-block;
            background: linear-gradient(135deg, #0284c7 0%, #2563eb 100%);
            color: #ffffff;
            text-decoration: none;
            padding: 0.75rem 1.75rem;
            border-radius: 0.75rem;
            font-weight: 600;
            font-size: 0.95rem;
            transition: transform 0.2s, box-shadow 0.2s;
        }}
        .btn:hover {{
            transform: translateY(-2px);
            box-shadow: 0 10px 20px -5px rgba(2, 132, 199, 0.5);
        }}
        .footer {{
            margin-top: 2.5rem;
            font-size: 0.8rem;
            color: #64748b;
            border-top: 1px solid rgba(255, 255, 255, 0.06);
            padding-top: 1.5rem;
        }}
    </style>
</head>
<body>
    <div class="card">
        <div class="status-badge">{code}</div>
        <h1>{reason}</h1>
        <p>{detail}</p>
        <a href="/" class="btn">Return Home</a>
        <div class="footer">Localhost HTTP/1.1 Server &bull; Written in Rust</div>
    </div>
</body>
</html>"#
        );

        let mut res = Response::new(status);
        res.set_header("Content-Type", "text/html; charset=utf-8");
        res.set_body(html.into_bytes());
        res
    }

    /// Serializes status line, headers, and body into a byte vector for writing to socket
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256 + self.body.len());

        // Status Line: HTTP/1.1 200 OK\r\n
        out.extend_from_slice(
            format!(
                "{} {} {}\r\n",
                self.version,
                self.status.as_u16(),
                self.status.reason_phrase()
            )
            .as_bytes(),
        );

        // Headers
        for (k, v) in self.headers.iter() {
            out.extend_from_slice(format!("{}: {}\r\n", k, v).as_bytes());
        }

        // Set-Cookie headers
        for cookie in &self.cookies_to_set {
            out.extend_from_slice(format!("Set-Cookie: {}\r\n", cookie).as_bytes());
        }

        // End of headers
        out.extend_from_slice(b"\r\n");

        // Body
        out.extend_from_slice(&self.body);

        out
    }
}

pub fn http_date() -> String {
    // Generates RFC 1123 GMT date: e.g. "Wed, 21 Oct 2015 07:28:00 GMT"
    let now = SystemTime::now();
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Days since unix epoch
    let mut days = (secs / 86400) as i64;
    let day_secs = (secs % 86400) as u32;

    let hour = day_secs / 3600;
    let minute = (day_secs % 3600) / 60;
    let second = day_secs % 60;

    // Day of week: 1970-01-01 was Thursday (index 4)
    let wday_idx = ((days + 4) % 7 + 7) % 7;
    let wdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let wday = wdays[wday_idx as usize];

    // Compute year, month, day
    let mut year = 1970;
    loop {
        let leap = is_leap_year(year);
        let days_in_year = if leap { 366 } else { 365 };
        if days >= days_in_year {
            days -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }

    let leap = is_leap_year(year);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    let mut month_idx = 0;
    for (idx, &mdays) in month_days.iter().enumerate() {
        if days >= mdays {
            days -= mdays;
        } else {
            month_idx = idx;
            break;
        }
    }

    let month = months[month_idx];
    let day = days + 1;

    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        wday, day, month, year, hour, minute, second
    )
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
