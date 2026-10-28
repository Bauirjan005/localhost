use std::path::{Path, PathBuf};
use crate::config::model::{RouteConfig, ServerConfig};
use crate::http::method::Method;
use crate::http::request::Request;
use crate::http::status::StatusCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteTarget {
    StaticFile(PathBuf),
    DirectoryListing {
        dir_path: PathBuf,
        url_path: String,
    },
    Cgi {
        script_path: PathBuf,
        interpreter: String,
        path_info: String,
    },
    Redirect {
        status: StatusCode,
        location: String,
    },
    Upload {
        upload_dir: PathBuf,
    },
}

#[derive(Debug, Clone)]
pub struct MatchedRoute<'a> {
    pub route: &'a RouteConfig,
    pub target: RouteTarget,
    pub client_max_body_size: usize,
}

pub struct Router;

impl Router {
    pub fn match_route<'a>(
        server: &'a ServerConfig,
        req: &Request,
    ) -> Result<MatchedRoute<'a>, (StatusCode, String)> {
        let route = Self::find_longest_prefix_route(server, &req.path)
            .ok_or_else(|| (StatusCode::NOT_FOUND, "No matching route found".to_string()))?;

        // 1. Check redirection
        if let Some((status, ref target)) = route.redirect {
            return Ok(MatchedRoute {
                route,
                target: RouteTarget::Redirect {
                    status,
                    location: target.clone(),
                },
                client_max_body_size: route.client_max_body_size.unwrap_or(server.client_max_body_size),
            });
        }

        // 2. Check accepted HTTP methods
        if !route.accepted_methods.contains(&req.method) {
            let allowed_methods = route
                .accepted_methods
                .iter()
                .map(|m| m.as_str())
                .collect::<Vec<&str>>()
                .join(", ");
            return Err((
                StatusCode::METHOD_NOT_ALLOWED,
                format!("Method not allowed. Allowed methods: {}", allowed_methods),
            ));
        }

        let max_body_size = route
            .client_max_body_size
            .unwrap_or(server.client_max_body_size);

        // 3. Check if this is an upload endpoint (POST with configured upload_dir)
        if req.method == Method::Post && route.upload_dir.is_some() {
            let u_dir = PathBuf::from(route.upload_dir.as_ref().unwrap());
            return Ok(MatchedRoute {
                route,
                target: RouteTarget::Upload { upload_dir: u_dir },
                client_max_body_size: max_body_size,
            });
        }

        // 4. Resolve filesystem path base
        let base_root = route
            .root
            .as_deref()
            .or(server.root.as_deref())
            .unwrap_or("./www");

        // 5. Check if URL matches a CGI script with optional PATH_INFO
        for (ext, interpreter) in &route.cgi_extensions {
            if let Some(pos) = req.path.find(ext) {
                let end_idx = pos + ext.len();
                if end_idx == req.path.len() || req.path.as_bytes().get(end_idx) == Some(&b'/') {
                    let script_url_path = &req.path[..end_idx];
                    let path_info = &req.path[end_idx..];
                    let script_fs_path = resolve_filesystem_path(base_root, &route.path, script_url_path);
                    return Ok(MatchedRoute {
                        route,
                        target: RouteTarget::Cgi {
                            script_path: script_fs_path,
                            interpreter: interpreter.clone(),
                            path_info: path_info.to_string(),
                        },
                        client_max_body_size: max_body_size,
                    });
                }
            }
        }

        let resolved_path = resolve_filesystem_path(base_root, &route.path, &req.path);

        // 6. Check directory vs file
        if resolved_path.is_dir() {
            // Check for default index file
            if let Some(ref default_file) = route.default_file.as_deref().or(server.default_file.as_deref()) {
                let index_path = resolved_path.join(default_file);
                if index_path.is_file() {
                    // Check if the default file itself is a CGI script
                    if let Some((ext, interpreter)) = find_cgi_handler(route, &index_path) {
                        let path_info = extract_path_info(&req.path, &route.path, &ext);
                        return Ok(MatchedRoute {
                            route,
                            target: RouteTarget::Cgi {
                                script_path: index_path,
                                interpreter,
                                path_info,
                            },
                            client_max_body_size: max_body_size,
                        });
                    }
                    return Ok(MatchedRoute {
                        route,
                        target: RouteTarget::StaticFile(index_path),
                        client_max_body_size: max_body_size,
                    });
                }
            }

            // No default index file found. Check autoindex
            if route.autoindex {
                return Ok(MatchedRoute {
                    route,
                    target: RouteTarget::DirectoryListing {
                        dir_path: resolved_path,
                        url_path: req.path.clone(),
                    },
                    client_max_body_size: max_body_size,
                });
            } else {
                return Err((
                    StatusCode::FORBIDDEN,
                    "Directory listing is forbidden and no index file was found".to_string(),
                ));
            }
        }

        // 7. If it's a file or not existing yet (e.g. for DELETE or upload or 404)
        if req.method == Method::Delete {
            return Ok(MatchedRoute {
                route,
                target: RouteTarget::StaticFile(resolved_path),
                client_max_body_size: max_body_size,
            });
        }

        if resolved_path.exists() {
            Ok(MatchedRoute {
                route,
                target: RouteTarget::StaticFile(resolved_path),
                client_max_body_size: max_body_size,
            })
        } else {
            Err((
                StatusCode::NOT_FOUND,
                format!("File not found: {}", req.path),
            ))
        }
    }

    /// Finds the location block with the longest prefix match for `path`
    pub fn find_longest_prefix_route<'a>(
        server: &'a ServerConfig,
        path: &str,
    ) -> Option<&'a RouteConfig> {
        let mut best_match: Option<&'a RouteConfig> = None;
        let mut best_len = 0;

        for route in &server.routes {
            let r_path = &route.path;
            if path_matches_prefix(path, r_path) {
                if best_match.is_none() || r_path.len() > best_len {
                    best_match = Some(route);
                    best_len = r_path.len();
                }
            }
        }

        // If no explicit route matched and server has no routes, create fallback
        best_match
    }
}

fn path_matches_prefix(path: &str, prefix: &str) -> bool {
    if prefix == "/" {
        return true;
    }
    if path == prefix {
        return true;
    }
    if path.starts_with(prefix) {
        let next_char = path.as_bytes().get(prefix.len());
        if next_char == Some(&b'/') || prefix.ends_with('/') {
            return true;
        }
    }
    false
}

pub fn resolve_filesystem_path(root: &str, route_prefix: &str, req_path: &str) -> PathBuf {
    let base = PathBuf::from(root);
    let trimmed_route = route_prefix.trim_end_matches('/');
    let remainder = if trimmed_route.is_empty() {
        req_path.trim_start_matches('/')
    } else if req_path.starts_with(trimmed_route) {
        req_path[trimmed_route.len()..].trim_start_matches('/')
    } else {
        req_path.trim_start_matches('/')
    };

    if remainder.is_empty() {
        base
    } else {
        base.join(remainder)
    }
}

fn find_cgi_handler(route: &RouteConfig, file_path: &Path) -> Option<(String, String)> {
    let ext = file_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{}", s))?;

    if let Some(interpreter) = route.cgi_extensions.get(&ext) {
        return Some((ext, interpreter.clone()));
    }
    None
}

fn extract_path_info(req_path: &str, _route_path: &str, ext: &str) -> String {
    if let Some(pos) = req_path.find(ext) {
        let end_of_ext = pos + ext.len();
        if end_of_ext < req_path.len() {
            return req_path[end_of_ext..].to_string();
        }
    }
    String::new()
}
