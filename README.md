# Localhost &bull; HTTP/1.1 Server in Rust

A fully compliant, high-performance, single-threaded **HTTP/1.1 Web Server** written from scratch in **Rust**. Built for the **01-Edu (Zone01 / Reboot01)** "localhost" curriculum.

---

## Key Features & Compliance

- **Single-Threaded, Event-Driven Architecture:**
  - Operates using **one process and one thread**.
  - All network I/O operations are **100% non-blocking**.
  - Event multiplexing via `libc::epoll` on Linux (`epoll_create1`, `epoll_ctl`, `epoll_wait`) and `WSAPoll` on Windows.
  - Sockets and requests are processed through the event loop without blocking.

- **HTTP/1.1 Protocol Support:**
  - Full request-line, header, and body parsing.
  - **Chunked Transfer Encoding** (decode incoming `Transfer-Encoding: chunked` streams with extensions and trailers).
  - Unchunked `Content-Length` requests with strict body limit enforcement.
  - **Persistent Connections** (`Connection: keep-alive` by default, pipelining support, and clean `close`).
  - Strict header validation (e.g. mandatory `Host` header for HTTP/1.1).

- **Multi-Server & Multi-Port Listening:**
  - Can listen on multiple ports simultaneously (e.g. `8080`, `8081`, `8082`).
  - Supports virtual hosts on the same port via `server_name` matching with automatic fallback to the default server (the first server defined for that host:port).

- **Methods Supported:**
  - `GET`: Serve static files with automatic MIME detection or execute CGIs.
  - `POST`: Handle file uploads (`multipart/form-data` and raw binary) or submit data to CGI scripts.
  - `DELETE`: Remove files safely with permission and existence checks.
  - Returns `405 Method Not Allowed` with RFC-compliant `Allow` header when a method is disallowed on a route.

- **Cookies & Session Management:**
  - Parses `Cookie` headers and issues `Set-Cookie` directives (`HttpOnly`, `SameSite=Lax`).
  - In-memory stateful session store with session timeouts and automated eviction to prevent memory leaks.
  - Interactive session dashboard at `/session`.

- **CGI 1.1 Gateway:**
  - Executes scripts dynamically based on file extensions (e.g., Python `.py` and Shell `.sh`).
  - Passes the script path as the first argument (`argv[1]`).
  - Sets the working directory to the script's directory for relative path resolution.
  - Sets standard CGI environment variables (`REQUEST_METHOD`, `QUERY_STRING`, `CONTENT_LENGTH`, `CONTENT_TYPE`, `PATH_INFO`, `GATEWAY_INTERFACE`, `REMOTE_ADDR`, `HTTP_*` headers).
  - Streams request body to script `stdin` terminated with `EOF`.
  - Process timeout protection (5 seconds) to prevent infinite loops from hanging the server.

- **Directory Listing (Autoindex):**
  - When `autoindex on` is enabled on a route and no default index file exists, dynamically generates a responsive HTML directory browser.
  - If `autoindex off` and no index file exists, safely returns `403 Forbidden`.

- **Custom & Rich Default Error Pages:**
  - Richly styled default error pages for at least `[400, 403, 404, 405, 413, 500]`.
  - Configurable custom error pages per status code in the configuration file.

- **Zero Framework Crates:**
  - No `tokio`, `hyper`, `actix`, `warp`, or `nix`.
  - Only uses `libc` for system call bindings as permitted by 01-Edu guidelines.

---

## Project Structure

```
├── Cargo.toml                    # Package manifest (dependencies: libc)
├── config/
│   ├── default.conf              # Primary server configuration
│   ├── multi_server.conf         # Multi-host virtual server demonstration
│   ├── invalid_conflicting_port.conf
│   └── invalid_body_size.conf
├── src/
│   ├── main.rs                   # CLI entrypoint and server runner
│   ├── lib.rs                    # Module root
│   ├── config/                   # Config parsing, models, and validation
│   ├── http/                     # Request parser, Response builder, Status, MIME, Headers
│   ├── router/                   # Longest-prefix route matcher and redirector
│   ├── session/                  # In-memory session store & cookie manager
│   ├── cgi/                      # CGI 1.1 process execution and timeout handler
│   ├── static_handler/           # File serving, autoindex, upload, and delete handlers
│   └── server/                   # Event loop, listeners, clients, and poller abstraction
│       └── poller/
│           ├── epoll.rs          # Linux epoll implementation via libc
│           └── windows.rs        # Windows WSAPoll implementation
├── tests/
│   ├── request_parsing_test.rs   # Unit tests: HTTP request parsing
│   ├── config_validation_test.rs # Unit tests: Config validator (ports, paths, sizes)
│   ├── route_matching_test.rs    # Unit tests: Route matching logic
│   ├── status_code_test.rs       # Unit tests: Status code generation
│   ├── chunked_transfer_test.rs  # Unit tests: Chunked transfer-encoding
│   ├── session_cookie_test.rs    # Unit tests: Session and cookie tracking
│   └── e2e_server_test.rs        # End-to-end TCP integration test
├── www/                          # Static demo site, uploads, CGI, and custom error pages
└── scripts/
    └── siege_test.sh             # Benchmark and stress testing script
```

---

## Configuration File Syntax

Configuration files use an intuitive Nginx-like syntax:

```nginx
server {
    host 127.0.0.1
    listen 8080 8081
    server_name localhost example.com
    client_max_body_size 10M

    error_page 404 ./www/errors/404.html
    error_page 500 ./www/errors/500.html

    location / {
        accepted_methods GET POST DELETE
        root ./www
        index index.html
        autoindex on
    }

    location /upload {
        accepted_methods GET POST DELETE
        upload_dir ./www/uploads
        root ./www/uploads
        autoindex on
    }

    location /redirect-me {
        redirect 301 /
    }

    location /cgi-bin {
        accepted_methods GET POST
        root ./www/cgi-bin
        cgi .py python3
        cgi .sh /bin/bash
    }
}
```

---

## Running the Server

### 1. Build and Run
```bash
# Run with default configuration (config/default.conf - listens on http://127.0.0.1:8081 and 8082)
cargo run

# Run with multi-virtual host configuration (config/multi_server.conf)
cargo run -- config/multi_server.conf
```

### 2. Run Test Suite
```bash
# Execute all 36 unit and end-to-end integration tests
cargo test
```

### 3. Verify Linux Target Compilation
```bash
# Check strict Linux epoll libc compilation
cargo check --target x86_64-unknown-linux-gnu
```

### 4. Stress Testing & Availability Benchmark
```powershell
# On Windows PowerShell:
.\scripts\siege_test.ps1 -Port 8081 -Requests 300 -Concurrent 30

# On Linux / Git Bash with siege:
./scripts/siege_test.sh 127.0.0.1 8081 10s 50
```
Availability target: **&ge; 99.5%**.
