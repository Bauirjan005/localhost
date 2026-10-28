use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::model::Config;
use crate::http::method::Method;
use crate::http::request::Request;
use crate::http::response::Response;
use crate::http::status::StatusCode;
use crate::router::{RouteTarget, Router};
use crate::session::SessionStore;
use crate::static_handler::StaticHandler;
use crate::cgi::CgiExecutor;

use super::client::{Client, ClientActivity, ClientIoError};
use super::listener::Listener;
use super::poller::{AsSocketHandle, DefaultPoller, PollEvent, PollInterest, Poller};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const SESSION_TIMEOUT: Duration = Duration::from_secs(3600); // 1 hour

pub struct Server {
    config: Config,
    listeners: HashMap<usize, Listener>,
    clients: HashMap<usize, Client>,
    poller: DefaultPoller,
    next_client_token: usize,
    session_store: SessionStore,
    running: Arc<AtomicBool>,
}

impl Server {
    pub fn new(config: Config) -> io::Result<Self> {
        let poller = DefaultPoller::new()?;

        Ok(Server {
            config,
            listeners: HashMap::new(),
            clients: HashMap::new(),
            poller,
            next_client_token: 10000,
            session_store: SessionStore::new(SESSION_TIMEOUT),
            running: Arc::new(AtomicBool::new(true)),
        })
    }

    pub fn shutdown_handle(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    pub fn start(&mut self) -> io::Result<()> {
        // 1. Identify all unique (host, port) combinations
        let mut host_ports = Vec::new();
        for server in &self.config.servers {
            for &port in &server.ports {
                if !host_ports.contains(&(server.host.clone(), port)) {
                    host_ports.push((server.host.clone(), port));
                }
            }
        }

        // 2. Bind listeners for all unique ports
        let mut listener_token = 1;
        for (host, port) in host_ports {
            let listener = Listener::bind(listener_token, &host, port).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to bind listener on {}:{}: {} (port may already be in use by another process)",
                        host, port, e
                    ),
                )
            })?;
            self.poller.register(
                listener_token,
                listener.listener.as_socket_handle(),
                PollInterest::READABLE,
            )?;
            println!(" [Localhost] Listening on http://{}:{}", host, port);
            self.listeners.insert(listener_token, listener);
            listener_token += 1;
        }

        println!(" [Localhost] Server running in single-threaded event loop. Press Ctrl+C to stop.");

        let mut events = Vec::with_capacity(1024);
        let mut last_timeout_check = Instant::now();

        // Main event loop: single thread, non-blocking multiplexed I/O
        while self.running.load(Ordering::Relaxed) {
            // Poll for ready I/O events
            let _ = self.poller.poll(&mut events, 100);

            // Process ready events
            for i in 0..events.len() {
                let ev = events[i];
                self.handle_event(ev);
            }

            // Periodic timeout checking every 500ms
            if last_timeout_check.elapsed() > Duration::from_millis(500) {
                self.check_timeouts();
                last_timeout_check = Instant::now();
            }
        }

        println!(" [Localhost] Server shutting down gracefully...");
        Ok(())
    }

    fn handle_event(&mut self, ev: PollEvent) {
        // Case 1: Listener socket ready for accept
        if self.listeners.contains_key(&ev.token) {
            self.handle_listener_event(ev.token);
            return;
        }

        // Case 2: Client socket ready for read/write
        if !self.clients.contains_key(&ev.token) {
            return;
        }

        if ev.error || (ev.hangup && !ev.readable) {
            self.remove_client(ev.token);
            return;
        }

        if ev.readable {
            self.handle_client_read(ev.token);
        }

        if ev.writable && self.clients.contains_key(&ev.token) {
            self.handle_client_write(ev.token);
        }
    }

    fn handle_listener_event(&mut self, listener_token: usize) {
        let (host, port) = {
            let listener = match self.listeners.get(&listener_token) {
                Some(l) => l,
                None => return,
            };
            (listener.host.clone(), listener.port)
        };

        let new_conns = match self.listeners.get(&listener_token).unwrap().accept_all() {
            Ok(conns) => conns,
            Err(_) => return,
        };

        for (stream, peer_addr) in new_conns {
            let client_token = self.next_client_token;
            self.next_client_token += 1;

            let max_body_size = self
                .config
                .find_server(&host, port, None)
                .map(|s| s.client_max_body_size)
                .unwrap_or(10 * 1024 * 1024);

            let handle = stream.as_socket_handle();
            match Client::new(
                client_token,
                stream,
                peer_addr,
                host.clone(),
                port,
                max_body_size,
            ) {
                Ok(client) => {
                    if self
                        .poller
                        .register(client_token, handle, PollInterest::READABLE)
                        .is_ok()
                    {
                        self.clients.insert(client_token, client);
                    }
                }
                Err(_) => {
                    // Failed to initialize client, ignore
                }
            }
        }
    }

    fn handle_client_read(&mut self, token: usize) {
        let activity = {
            let client = match self.clients.get_mut(&token) {
                Some(c) => c,
                None => return,
            };

            match client.on_readable() {
                Ok(act) => act,
                Err(ClientIoError::WouldBlock) => return,
                Err(ClientIoError::Disconnected) => {
                    self.remove_client(token);
                    return;
                }
                Err(ClientIoError::ProtocolError(status, msg)) => {
                    let server = self
                        .config
                        .find_server(&client.server_host, client.server_port, None)
                        .cloned()
                        .unwrap_or_default();
                    let mut res = StaticHandler::render_error_page(&server, status, Some(&msg));
                    res.set_connection_close();
                    client.queue_response(res);

                    let handle = client.stream.as_socket_handle();
                    let _ = self.poller.reregister(token, handle, PollInterest::WRITABLE);
                    return;
                }
                Err(ClientIoError::Io(_)) => {
                    self.remove_client(token);
                    return;
                }
            }
        };

        match activity {
            ClientActivity::ReadyToHandle(request) => {
                let response = self.dispatch_request(token, &request);
                if let Some(client) = self.clients.get_mut(&token) {
                    client.queue_response(response);
                    let handle = client.stream.as_socket_handle();
                    let _ = self.poller.reregister(token, handle, PollInterest::WRITABLE);
                }
            }
            ClientActivity::Reading => {
                // Still reading
            }
            _ => {}
        }
    }

    fn handle_client_write(&mut self, token: usize) {
        let activity = {
            let client = match self.clients.get_mut(&token) {
                Some(c) => c,
                None => return,
            };

            match client.on_writable() {
                Ok(act) => act,
                Err(ClientIoError::WouldBlock) => return,
                Err(_) => {
                    self.remove_client(token);
                    return;
                }
            }
        };

        match activity {
            ClientActivity::WriteComplete { keep_alive: true } => {
                if let Some(client) = self.clients.get_mut(&token) {
                    let handle = client.stream.as_socket_handle();
                    let _ = self.poller.reregister(token, handle, PollInterest::READABLE);
                }
            }
            ClientActivity::WriteComplete { keep_alive: false } => {
                self.remove_client(token);
            }
            ClientActivity::Writing => {
                // Still writing
            }
            _ => {}
        }
    }

    fn dispatch_request(&mut self, token: usize, req: &Request) -> Response {
        let (server_host, server_port, peer_ip) = {
            let client = &self.clients[&token];
            (client.server_host.clone(), client.server_port, client.peer_addr.ip().to_string())
        };

        let server = match self.config.find_server(&server_host, server_port, req.host()) {
            Some(s) => s.clone(),
            None => {
                return Response::default_error(StatusCode::NOT_FOUND, Some("Server not found"));
            }
        };

        // Check if client updated body size limit based on route
        if let Some(client) = self.clients.get_mut(&token) {
            client.parser.update_max_body_size(server.client_max_body_size);
        }

        // 1. Session Demo Endpoints
        if req.path == "/session" || req.path == "/session/" {
            return self.handle_session_dashboard(req);
        }
        if req.path == "/session/login" && req.method == Method::Post {
            return self.handle_session_login(req);
        }
        if req.path == "/session/logout" {
            return self.handle_session_logout(req);
        }

        // 2. Route matching
        match Router::match_route(&server, req) {
            Ok(matched) => match matched.target {
                RouteTarget::Redirect { status, location } => {
                    Response::redirect(status, &location)
                }
                RouteTarget::StaticFile(ref path) => {
                    if req.method == Method::Delete {
                        match StaticHandler::delete_file(path) {
                            Ok(res) => res,
                            Err((status, msg)) => {
                                StaticHandler::render_error_page(&server, status, Some(&msg))
                            }
                        }
                    } else {
                        match StaticHandler::serve_file(path) {
                            Ok(res) => res,
                            Err((status, msg)) => {
                                StaticHandler::render_error_page(&server, status, Some(&msg))
                            }
                        }
                    }
                }
                RouteTarget::DirectoryListing { ref dir_path, ref url_path } => {
                    match StaticHandler::generate_directory_listing(dir_path, url_path) {
                        Ok(res) => res,
                        Err((status, msg)) => {
                            StaticHandler::render_error_page(&server, status, Some(&msg))
                        }
                    }
                }
                RouteTarget::Cgi { ref script_path, ref interpreter, ref path_info } => {
                    match CgiExecutor::execute(
                        script_path,
                        interpreter,
                        path_info,
                        &server,
                        server_port,
                        &peer_ip,
                        req,
                    ) {
                        Ok(res) => res,
                        Err((status, msg)) => {
                            StaticHandler::render_error_page(&server, status, Some(&msg))
                        }
                    }
                }
                RouteTarget::Upload { ref upload_dir } => {
                    match StaticHandler::handle_upload(upload_dir, req) {
                        Ok(res) => res,
                        Err((status, msg)) => {
                            StaticHandler::render_error_page(&server, status, Some(&msg))
                        }
                    }
                }
            },
            Err((status, msg)) => {
                StaticHandler::render_error_page(&server, status, Some(&msg))
            }
        }
    }

    fn handle_session_dashboard(&mut self, req: &Request) -> Response {
        let cookie_id = req.cookies.get("session_id").map(|s| s.as_str());
        let (session_id, is_new) = self.session_store.get_or_create(cookie_id);

        let visits = {
            let session = self.session_store.get_mut(&session_id).unwrap();
            let count: u64 = session
                .get("visit_count")
                .and_then(|c| c.parse().ok())
                .unwrap_or(0)
                + 1;
            session.set("visit_count", count.to_string());
            count
        };

        let user = self
            .session_store
            .get(&session_id)
            .and_then(|s| s.get("username"))
            .unwrap_or("Guest");

        let active_sessions = self.session_store.count();

        let html = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Cookie & Session Management - Localhost</title>
    <link rel="stylesheet" href="/style.css">
    <style>
        .session-box {{
            background: rgba(30, 41, 59, 0.7);
            border: 1px solid rgba(255, 255, 255, 0.1);
            border-radius: 1rem;
            padding: 2rem;
            margin-bottom: 2rem;
        }}
        .stat-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 1rem;
            margin: 1.5rem 0;
        }}
        .stat-card {{
            background: rgba(15, 23, 42, 0.6);
            border: 1px solid rgba(255, 255, 255, 0.05);
            padding: 1.25rem;
            border-radius: 0.75rem;
            text-align: center;
        }}
        .stat-val {{
            font-size: 1.8rem;
            font-weight: 700;
            color: #38bdf8;
            margin-top: 0.5rem;
        }}
        .form-row {{
            display: flex;
            gap: 0.75rem;
            margin-top: 1rem;
        }}
        input[type="text"] {{
            flex: 1;
            padding: 0.75rem 1rem;
            background: rgba(15, 23, 42, 0.8);
            border: 1px solid rgba(255, 255, 255, 0.15);
            border-radius: 0.5rem;
            color: #fff;
            font-size: 1rem;
        }}
    </style>
</head>
<body>
    <div class="container">
        <header>
            <div class="logo">Localhost &bull; Cookie & Session Engine</div>
            <nav><a href="/">Home</a> &bull; <a href="/upload">Uploads</a> &bull; <a href="/cgi-bin/test.py">Python CGI</a></nav>
        </header>

        <div class="session-box">
            <h2>Current User Session</h2>
            <p>Demonstrating stateful HTTP session tracking and cookie manipulation.</p>

            <div class="stat-grid">
                <div class="stat-card">
                    <div>Logged in as</div>
                    <div class="stat-val">{user}</div>
                </div>
                <div class="stat-card">
                    <div>Page Visits</div>
                    <div class="stat-val">{visits}</div>
                </div>
                <div class="stat-card">
                    <div>Active Sessions</div>
                    <div class="stat-val">{active_sessions}</div>
                </div>
            </div>

            <p style="font-family: monospace; font-size: 0.85rem; color: #94a3b8; word-break: break-all;">
                <strong>Session ID:</strong> {session_id} {status_tag}
            </p>

            <h3 style="margin-top: 2rem;">Switch User</h3>
            <form action="/session/login" method="POST" class="form-row">
                <input type="text" name="username" placeholder="Enter username..." required>
                <button type="submit" class="btn">Log In</button>
            </form>

            <div style="margin-top: 1.5rem;">
                <a href="/session/logout" class="btn btn-secondary">Clear Session &amp; Logout</a>
                <a href="/session" class="btn">Refresh Page (Increment Visit)</a>
            </div>
        </div>
    </div>
</body>
</html>"#,
            user = user,
            visits = visits,
            active_sessions = active_sessions,
            session_id = session_id,
            status_tag = if is_new { "(Newly Created)" } else { "(Restored from Cookie)" }
        );

        let mut res = Response::html(StatusCode::OK, html);
        res.set_cookie(format!(
            "session_id={}; Path=/; HttpOnly; SameSite=Lax",
            session_id
        ));
        res
    }

    fn handle_session_login(&mut self, req: &Request) -> Response {
        let cookie_id = req.cookies.get("session_id").map(|s| s.as_str());
        let (session_id, _) = self.session_store.get_or_create(cookie_id);

        let username = req
            .query_params
            .get("username")
            .cloned()
            .or_else(|| {
                // Parse application/x-www-form-urlencoded body
                let body_str = String::from_utf8_lossy(&req.body);
                for pair in body_str.split('&') {
                    if let Some(pos) = pair.find('=') {
                        let k = &pair[..pos];
                        let v = &pair[pos + 1..];
                        if k == "username" {
                            return Some(
                                crate::http::request::percent_decode(&v.replace('+', " "))
                                    .unwrap_or_else(|_| v.to_string()),
                            );
                        }
                    }
                }
                None
            })
            .unwrap_or_else(|| "Anonymous".to_string());

        if let Some(session) = self.session_store.get_mut(&session_id) {
            session.set("username", username);
        }

        let mut res = Response::redirect(StatusCode::SEE_OTHER, "/session");
        res.set_cookie(format!(
            "session_id={}; Path=/; HttpOnly; SameSite=Lax",
            session_id
        ));
        res
    }

    fn handle_session_logout(&mut self, req: &Request) -> Response {
        if let Some(id) = req.cookies.get("session_id") {
            self.session_store.destroy(id);
        }

        let mut res = Response::redirect(StatusCode::SEE_OTHER, "/session");
        // Expire cookie
        res.set_cookie("session_id=; Path=/; Max-Age=0; HttpOnly");
        res
    }

    fn check_timeouts(&mut self) {
        let now = Instant::now();
        let mut timed_out_tokens = Vec::new();

        for (&token, client) in &self.clients {
            if now.duration_since(client.last_active) > REQUEST_TIMEOUT {
                timed_out_tokens.push(token);
            }
        }

        for token in timed_out_tokens {
            if let Some(mut client) = self.clients.remove(&token) {
                let _ = self.poller.deregister(client.stream.as_socket_handle());
                // Send 408 Request Timeout before closing
                let server = self
                    .config
                    .find_server(&client.server_host, client.server_port, None)
                    .cloned()
                    .unwrap_or_default();
                let mut res = StaticHandler::render_error_page(
                    &server,
                    StatusCode::REQUEST_TIMEOUT,
                    Some("Request timed out due to client inactivity"),
                );
                res.set_connection_close();
                let bytes = res.to_bytes();
                use std::io::Write;
                let _ = client.stream.write_all(&bytes);
            }
        }
    }

    fn remove_client(&mut self, token: usize) {
        if let Some(client) = self.clients.remove(&token) {
            let _ = self.poller.deregister(client.stream.as_socket_handle());
        }
    }
}
