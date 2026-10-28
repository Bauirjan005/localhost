use std::collections::HashMap;
use crate::http::method::Method;
use crate::http::status::StatusCode;

pub const DEFAULT_MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10MB
pub const ABSOLUTE_MAX_BODY_SIZE: usize = 500 * 1024 * 1024; // 500MB sanity bound

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteConfig {
    pub path: String,
    pub accepted_methods: Vec<Method>,
    pub root: Option<String>,
    pub default_file: Option<String>,
    pub autoindex: bool,
    pub redirect: Option<(StatusCode, String)>,
    pub cgi_extensions: HashMap<String, String>, // extension (e.g. ".py") -> executable (e.g. "python3")
    pub upload_dir: Option<String>,
    pub client_max_body_size: Option<usize>,
}

impl Default for RouteConfig {
    fn default() -> Self {
        RouteConfig {
            path: "/".to_string(),
            accepted_methods: vec![Method::Get, Method::Post, Method::Delete],
            root: None,
            default_file: Some("index.html".to_string()),
            autoindex: false,
            redirect: None,
            cgi_extensions: HashMap::new(),
            upload_dir: None,
            client_max_body_size: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub host: String,
    pub ports: Vec<u16>,
    pub server_names: Vec<String>,
    pub client_max_body_size: usize,
    pub error_pages: HashMap<StatusCode, String>,
    pub routes: Vec<RouteConfig>,
    pub root: Option<String>,
    pub default_file: Option<String>,
    pub is_default: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            host: "127.0.0.1".to_string(),
            ports: vec![8080],
            server_names: Vec::new(),
            client_max_body_size: DEFAULT_MAX_BODY_SIZE,
            error_pages: HashMap::new(),
            routes: Vec::new(),
            root: Some("./www".to_string()),
            default_file: Some("index.html".to_string()),
            is_default: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub servers: Vec<ServerConfig>,
}

impl Config {
    pub fn new() -> Self {
        Config {
            servers: Vec::new(),
        }
    }

    /// Finds the matching server configuration for a given host:port and Host header
    pub fn find_server(&self, host: &str, port: u16, host_header: Option<&str>) -> Option<&ServerConfig> {
        let matching_servers: Vec<&ServerConfig> = self
            .servers
            .iter()
            .filter(|s| {
                let host_match = s.host == host
                    || s.host == "0.0.0.0"
                    || host == "0.0.0.0"
                    || (s.host == "127.0.0.1" && host == "localhost")
                    || (s.host == "localhost" && host == "127.0.0.1");
                let port_match = s.ports.contains(&port);
                host_match && port_match
            })
            .collect();

        if matching_servers.is_empty() {
            return None;
        }

        // If host_header is present, extract server_name (without port)
        if let Some(h) = host_header {
            let requested_name = h.split(':').next().unwrap_or("").trim().to_ascii_lowercase();
            for s in &matching_servers {
                for name in &s.server_names {
                    if name.to_ascii_lowercase() == requested_name {
                        return Some(s);
                    }
                }
            }
        }

        // If no server_name matched, return the default server for this host:port (the first one)
        Some(matching_servers[0])
    }
}
