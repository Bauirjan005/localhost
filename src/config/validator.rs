use std::collections::{HashMap, HashSet};
use std::path::Path;
use super::model::{Config, RouteConfig, ServerConfig, ABSOLUTE_MAX_BODY_SIZE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    ConflictingPorts {
        host: String,
        port: u16,
        server_name: String,
        details: String,
    },
    InvalidPath {
        context: String,
        path: String,
        reason: String,
    },
    OutOfBoundsBodySize {
        context: String,
        size: usize,
        min: usize,
        max: usize,
    },
    InvalidRedirect {
        location: String,
        status: u16,
        reason: String,
    },
    EmptyMethods {
        location: String,
    },
    DuplicateLocation {
        server_index: usize,
        location: String,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::ConflictingPorts { host, port, server_name, details } => {
                write!(
                    f,
                    "Conflicting port configuration on {}:{} for server_name '{}': {}",
                    host, port, server_name, details
                )
            }
            ValidationError::InvalidPath { context, path, reason } => {
                write!(f, "Invalid path in {}: '{}' ({})", context, path, reason)
            }
            ValidationError::OutOfBoundsBodySize { context, size, min, max } => {
                write!(
                    f,
                    "Out of bounds body size in {}: {} bytes (allowed range: {} to {} bytes)",
                    context, size, min, max
                )
            }
            ValidationError::InvalidRedirect { location, status, reason } => {
                write!(f, "Invalid redirect at '{}': status {} ({})", location, status, reason)
            }
            ValidationError::EmptyMethods { location } => {
                write!(f, "Route '{}' has no accepted HTTP methods", location)
            }
            ValidationError::DuplicateLocation { server_index, location } => {
                write!(
                    f,
                    "Server #{} has duplicate location block for '{}'",
                    server_index, location
                )
            }
        }
    }
}

pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate(config: &Config) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if config.servers.is_empty() {
            errors.push(ValidationError::InvalidPath {
                context: "config".into(),
                path: "".into(),
                reason: "Configuration has no server blocks".into(),
            });
            return Err(errors);
        }

        // 1. Check conflicting ports and server_name collisions
        Self::validate_ports_and_server_names(&config.servers, &mut errors);

        // 2. Validate each server block
        for (idx, server) in config.servers.iter().enumerate() {
            Self::validate_server(idx, server, &mut errors);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_ports_and_server_names(servers: &[ServerConfig], errors: &mut Vec<ValidationError>) {
        // Map: (normalized_host, port) -> list of (server_idx, server_names)
        let mut map: HashMap<(String, u16), Vec<(usize, Vec<String>)>> = HashMap::new();

        for (idx, s) in servers.iter().enumerate() {
            let host_norm = if s.host == "localhost" {
                "127.0.0.1".to_string()
            } else {
                s.host.clone()
            };

            for &port in &s.ports {
                let entry = map.entry((host_norm.clone(), port)).or_default();
                entry.push((idx, s.server_names.clone()));
            }
        }

        for ((host, port), instances) in map {
            if instances.len() > 1 {
                let mut seen_names = HashSet::new();
                let mut empty_names_count = 0;

                for (idx, names) in &instances {
                    if names.is_empty() {
                        empty_names_count += 1;
                        if empty_names_count > 1 {
                            errors.push(ValidationError::ConflictingPorts {
                                host: host.clone(),
                                port,
                                server_name: "(default/unnamed)".to_string(),
                                details: format!(
                                    "Multiple default servers without server_name defined for {}:{}",
                                    host, port
                                ),
                            });
                        }
                    } else {
                        for name in names {
                            let lower = name.to_ascii_lowercase();
                            if !seen_names.insert(lower.clone()) {
                                errors.push(ValidationError::ConflictingPorts {
                                    host: host.clone(),
                                    port,
                                    server_name: name.clone(),
                                    details: format!(
                                        "Duplicate server_name '{}' on server #{} for {}:{}",
                                        name, idx, host, port
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn validate_server(server_index: usize, server: &ServerConfig, errors: &mut Vec<ValidationError>) {
        // Validate ports range
        for &port in &server.ports {
            if port == 0 {
                errors.push(ValidationError::ConflictingPorts {
                    host: server.host.clone(),
                    port,
                    server_name: server.server_names.first().cloned().unwrap_or_default(),
                    details: "Port cannot be 0".into(),
                });
            }
        }

        // Validate client body size
        const MIN_BODY_SIZE: usize = 1;
        if server.client_max_body_size < MIN_BODY_SIZE || server.client_max_body_size > ABSOLUTE_MAX_BODY_SIZE {
            errors.push(ValidationError::OutOfBoundsBodySize {
                context: format!("server #{}", server_index),
                size: server.client_max_body_size,
                min: MIN_BODY_SIZE,
                max: ABSOLUTE_MAX_BODY_SIZE,
            });
        }

        // Validate server root path if present
        if let Some(ref r) = server.root {
            Self::validate_path(&format!("server #{} root", server_index), r, errors);
        }

        // Validate locations
        let mut seen_locations = HashSet::new();
        for route in &server.routes {
            if !seen_locations.insert(route.path.clone()) {
                errors.push(ValidationError::DuplicateLocation {
                    server_index,
                    location: route.path.clone(),
                });
            }

            Self::validate_route(server_index, route, errors);
        }
    }

    fn validate_route(server_index: usize, route: &RouteConfig, errors: &mut Vec<ValidationError>) {
        if route.accepted_methods.is_empty() {
            errors.push(ValidationError::EmptyMethods {
                location: route.path.clone(),
            });
        }

        if let Some(size) = route.client_max_body_size {
            const MIN_BODY_SIZE: usize = 1;
            if size < MIN_BODY_SIZE || size > ABSOLUTE_MAX_BODY_SIZE {
                errors.push(ValidationError::OutOfBoundsBodySize {
                    context: format!("location '{}' in server #{}", route.path, server_index),
                    size,
                    min: MIN_BODY_SIZE,
                    max: ABSOLUTE_MAX_BODY_SIZE,
                });
            }
        }

        if let Some(ref r) = route.root {
            Self::validate_path(
                &format!("location '{}' root in server #{}", route.path, server_index),
                r,
                errors,
            );
        }

        if let Some(ref u) = route.upload_dir {
            Self::validate_path(
                &format!("location '{}' upload_dir in server #{}", route.path, server_index),
                u,
                errors,
            );
        }

        if let Some((status, ref target)) = route.redirect {
            if !status.is_redirection() {
                errors.push(ValidationError::InvalidRedirect {
                    location: route.path.clone(),
                    status: status.as_u16(),
                    reason: "Redirect status code must be 3xx (e.g. 301, 302, 303, 307, 308)".into(),
                });
            }
            if target.is_empty() {
                errors.push(ValidationError::InvalidRedirect {
                    location: route.path.clone(),
                    status: status.as_u16(),
                    reason: "Redirect target URL cannot be empty".into(),
                });
            }
        }
    }

    fn validate_path(context: &str, path_str: &str, errors: &mut Vec<ValidationError>) {
        let trimmed = path_str.trim();
        if trimmed.is_empty() {
            errors.push(ValidationError::InvalidPath {
                context: context.to_string(),
                path: path_str.to_string(),
                reason: "Path cannot be empty".to_string(),
            });
            return;
        }

        if trimmed.contains('\0') {
            errors.push(ValidationError::InvalidPath {
                context: context.to_string(),
                path: path_str.to_string(),
                reason: "Path cannot contain null bytes".to_string(),
            });
            return;
        }

        // Check for suspicious directory traversal beyond root
        let path = Path::new(trimmed);
        let mut depth: i32 = 0;
        for comp in path.components() {
            match comp {
                std::path::Component::ParentDir => {
                    depth -= 1;
                    if depth < 0 && !path.is_absolute() {
                        errors.push(ValidationError::InvalidPath {
                            context: context.to_string(),
                            path: path_str.to_string(),
                            reason: "Path escapes root directory with '..' traversal".to_string(),
                        });
                        return;
                    }
                }
                std::path::Component::Normal(_) => {
                    depth += 1;
                }
                _ => {}
            }
        }
    }
}
