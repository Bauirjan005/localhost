use std::collections::HashMap;
use std::fs;
use std::path::Path;
use super::model::{Config, RouteConfig, ServerConfig};
use crate::http::method::Method;
use crate::http::status::StatusCode;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    OpenBrace,
    CloseBrace,
    Semicolon,
}

pub struct ConfigParser;

impl ConfigParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Config, String> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Failed to read config file '{}': {}", path.as_ref().display(), e))?;
        Self::parse_string(&content)
    }

    pub fn parse_string(content: &str) -> Result<Config, String> {
        let tokens = tokenize(content)?;
        let mut index = 0;
        let mut servers = Vec::new();

        while index < tokens.len() {
            match &tokens[index] {
                Token::Word(w) if w == "server" => {
                    index += 1;
                    while index < tokens.len() && tokens[index] == Token::Semicolon {
                        index += 1;
                    }
                    if index >= tokens.len() || tokens[index] != Token::OpenBrace {
                        return Err("Expected '{' after 'server'".to_string());
                    }
                    index += 1;
                    let server = parse_server_block(&tokens, &mut index)?;
                    servers.push(server);
                }
                Token::Semicolon => {
                    index += 1;
                }
                other => {
                    return Err(format!("Unexpected token at top level: {:?}", other));
                }
            }
        }

        if servers.is_empty() {
            return Err("Configuration must define at least one 'server' block".to_string());
        }

        // Mark the first server for each unique host:port combination as default
        let mut seen_host_ports = std::collections::HashSet::new();
        for server in &mut servers {
            let mut is_first = false;
            for &port in &server.ports {
                let key = format!("{}:{}", server.host, port);
                if seen_host_ports.insert(key) {
                    is_first = true;
                }
            }
            server.is_default = is_first;
        }

        Ok(Config { servers })
    }
}

fn tokenize(content: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut current_word = String::new();
    let mut in_comment = false;

    for ch in content.chars() {
        if in_comment {
            if ch == '\n' || ch == '\r' {
                in_comment = false;
                if !tokens.last().map_or(false, |t| *t == Token::Semicolon) {
                    tokens.push(Token::Semicolon);
                }
            }
            continue;
        }

        if ch == '#' {
            if !current_word.is_empty() {
                tokens.push(Token::Word(std::mem::take(&mut current_word)));
            }
            in_comment = true;
            continue;
        }

        if ch == '\n' || ch == '\r' {
            if !current_word.is_empty() {
                tokens.push(Token::Word(std::mem::take(&mut current_word)));
            }
            if !tokens.last().map_or(false, |t| *t == Token::Semicolon) {
                tokens.push(Token::Semicolon);
            }
            continue;
        }

        if ch.is_whitespace() {
            if !current_word.is_empty() {
                tokens.push(Token::Word(std::mem::take(&mut current_word)));
            }
            continue;
        }

        if ch == '{' {
            if !current_word.is_empty() {
                tokens.push(Token::Word(std::mem::take(&mut current_word)));
            }
            tokens.push(Token::OpenBrace);
            continue;
        }

        if ch == '}' {
            if !current_word.is_empty() {
                tokens.push(Token::Word(std::mem::take(&mut current_word)));
            }
            tokens.push(Token::CloseBrace);
            continue;
        }

        if ch == ';' {
            if !current_word.is_empty() {
                tokens.push(Token::Word(std::mem::take(&mut current_word)));
            }
            if !tokens.last().map_or(false, |t| *t == Token::Semicolon) {
                tokens.push(Token::Semicolon);
            }
            continue;
        }

        current_word.push(ch);
    }

    if !current_word.is_empty() {
        tokens.push(Token::Word(current_word));
    }

    Ok(tokens)
}

fn parse_server_block(tokens: &[Token], index: &mut usize) -> Result<ServerConfig, String> {
    let mut server = ServerConfig::default();
    server.ports.clear(); // We want ports explicitly from config if provided
    let mut ports_defined = false;

    while *index < tokens.len() {
        match &tokens[*index] {
            Token::CloseBrace => {
                *index += 1;
                if !ports_defined {
                    server.ports.push(8080);
                }
                return Ok(server);
            }
            Token::Semicolon => {
                *index += 1;
            }
            Token::Word(directive) => {
                let dir = directive.clone();
                *index += 1;
                match dir.as_str() {
                    "listen" | "port" => {
                        ports_defined = true;
                        let args = consume_directive_args(tokens, index);
                        if args.is_empty() {
                            return Err("Directive 'listen' requires at least one port number".to_string());
                        }
                        for arg in args {
                            let p: u16 = arg
                                .parse()
                                .map_err(|_| format!("Invalid port number: '{}'", arg))?;
                            if p == 0 {
                                return Err("Port number cannot be 0".to_string());
                            }
                            if !server.ports.contains(&p) {
                                server.ports.push(p);
                            }
                        }
                    }
                    "host" | "server_address" => {
                        let args = consume_directive_args(tokens, index);
                        if args.is_empty() {
                            return Err("Directive 'host' requires an IP or hostname".to_string());
                        }
                        server.host = args[0].clone();
                    }
                    "server_name" => {
                        let args = consume_directive_args(tokens, index);
                        for name in args {
                            server.server_names.push(name);
                        }
                    }
                    "client_max_body_size" => {
                        let args = consume_directive_args(tokens, index);
                        if args.is_empty() {
                            return Err("Directive 'client_max_body_size' requires a size argument".to_string());
                        }
                        server.client_max_body_size = parse_body_size(&args[0])?;
                    }
                    "error_page" => {
                        let args = consume_directive_args(tokens, index);
                        if args.len() < 2 {
                            return Err("Directive 'error_page' requires a status code and file path".to_string());
                        }
                        let code: u16 = args[0]
                            .parse()
                            .map_err(|_| format!("Invalid status code in error_page: '{}'", args[0]))?;
                        let page_path = args[1].clone();
                        server.error_pages.insert(StatusCode(code), page_path);
                    }
                    "root" => {
                        let args = consume_directive_args(tokens, index);
                        if args.is_empty() {
                            return Err("Directive 'root' requires a directory path".to_string());
                        }
                        server.root = Some(args[0].clone());
                    }
                    "index" => {
                        let args = consume_directive_args(tokens, index);
                        if args.is_empty() {
                            return Err("Directive 'index' requires a file name".to_string());
                        }
                        server.default_file = Some(args[0].clone());
                    }
                    "location" => {
                        while *index < tokens.len() && tokens[*index] == Token::Semicolon {
                            *index += 1;
                        }
                        if *index >= tokens.len() {
                            return Err("Expected path after 'location'".to_string());
                        }
                        let loc_path = match &tokens[*index] {
                            Token::Word(p) => p.clone(),
                            _ => return Err("Expected location path".to_string()),
                        };
                        *index += 1;

                        while *index < tokens.len() && tokens[*index] == Token::Semicolon {
                            *index += 1;
                        }
                        if *index >= tokens.len() || tokens[*index] != Token::OpenBrace {
                            return Err(format!("Expected '{{' after location '{}'", loc_path));
                        }
                        *index += 1;

                        let route = parse_location_block(&loc_path, tokens, index, &server)?;
                        server.routes.push(route);
                    }
                    unknown => {
                        return Err(format!("Unknown server directive: '{}'", unknown));
                    }
                }
            }
            other => {
                return Err(format!("Unexpected token inside server block: {:?}", other));
            }
        }
    }

    Err("Unclosed 'server' block: missing '}'".to_string())
}

fn parse_location_block(
    loc_path: &str,
    tokens: &[Token],
    index: &mut usize,
    server: &ServerConfig,
) -> Result<RouteConfig, String> {
    let mut route = RouteConfig {
        path: loc_path.to_string(),
        accepted_methods: vec![Method::Get, Method::Post, Method::Delete],
        root: server.root.clone(),
        default_file: server.default_file.clone(),
        autoindex: false,
        redirect: None,
        cgi_extensions: HashMap::new(),
        upload_dir: None,
        client_max_body_size: None,
    };
    let mut methods_specified = false;

    while *index < tokens.len() {
        match &tokens[*index] {
            Token::CloseBrace => {
                *index += 1;
                return Ok(route);
            }
            Token::Semicolon => {
                *index += 1;
            }
            Token::Word(directive) => {
                let dir = directive.clone();
                *index += 1;
                match dir.as_str() {
                    "accepted_methods" | "methods" | "limit_except" => {
                        if !methods_specified {
                            route.accepted_methods.clear();
                            methods_specified = true;
                        }
                        let args = consume_directive_args(tokens, index);
                        for arg in args {
                            let m: Method = arg.parse()?;
                            if !route.accepted_methods.contains(&m) {
                                route.accepted_methods.push(m);
                            }
                        }
                    }
                    "root" => {
                        let args = consume_directive_args(tokens, index);
                        if args.is_empty() {
                            return Err(format!("Directive 'root' in location '{}' requires a path", loc_path));
                        }
                        route.root = Some(args[0].clone());
                    }
                    "index" | "default_file" => {
                        let args = consume_directive_args(tokens, index);
                        if args.is_empty() {
                            return Err(format!("Directive 'index' in location '{}' requires a file", loc_path));
                        }
                        route.default_file = Some(args[0].clone());
                    }
                    "autoindex" | "directory_listing" => {
                        let args = consume_directive_args(tokens, index);
                        if args.is_empty() {
                            return Err(format!("Directive 'autoindex' in location '{}' requires 'on' or 'off'", loc_path));
                        }
                        match args[0].to_ascii_lowercase().as_str() {
                            "on" | "true" | "1" | "yes" => route.autoindex = true,
                            "off" | "false" | "0" | "no" => route.autoindex = false,
                            other => return Err(format!("Invalid autoindex value '{}': use 'on' or 'off'", other)),
                        }
                    }
                    "redirect" | "return" => {
                        let args = consume_directive_args(tokens, index);
                        if args.len() < 2 {
                            return Err(format!("Directive 'redirect' in location '{}' requires a status code and URL", loc_path));
                        }
                        let code: u16 = args[0]
                            .parse()
                            .map_err(|_| format!("Invalid status code for redirect: '{}'", args[0]))?;
                        let target = args[1].clone();
                        route.redirect = Some((StatusCode(code), target));
                    }
                    "cgi" | "cgi_pass" => {
                        let args = consume_directive_args(tokens, index);
                        if args.len() < 2 {
                            return Err(format!("Directive 'cgi' in location '{}' requires extension and executable (e.g. 'cgi .py python3')", loc_path));
                        }
                        let mut ext = args[0].clone();
                        if !ext.starts_with('.') {
                            ext.insert(0, '.');
                        }
                        let executable = args[1].clone();
                        route.cgi_extensions.insert(ext, executable);
                    }
                    "upload_dir" | "upload_store" => {
                        let args = consume_directive_args(tokens, index);
                        if args.is_empty() {
                            return Err(format!("Directive 'upload_dir' in location '{}' requires a path", loc_path));
                        }
                        route.upload_dir = Some(args[0].clone());
                    }
                    "client_max_body_size" => {
                        let args = consume_directive_args(tokens, index);
                        if args.is_empty() {
                            return Err("Directive 'client_max_body_size' requires a size argument".to_string());
                        }
                        route.client_max_body_size = Some(parse_body_size(&args[0])?);
                    }
                    unknown => {
                        return Err(format!("Unknown location directive: '{}' in location '{}'", unknown, loc_path));
                    }
                }
            }
            other => {
                return Err(format!("Unexpected token inside location block: {:?}", other));
            }
        }
    }

    Err(format!("Unclosed 'location' block for '{}': missing '}}'", loc_path))
}

fn consume_directive_args(tokens: &[Token], index: &mut usize) -> Vec<String> {
    let mut args = Vec::new();
    while *index < tokens.len() {
        match &tokens[*index] {
            Token::Word(w) => {
                args.push(w.clone());
                *index += 1;
            }
            Token::Semicolon => {
                *index += 1;
                break;
            }
            Token::OpenBrace | Token::CloseBrace => {
                break;
            }
        }
    }
    args
}

pub fn parse_body_size(s: &str) -> Result<usize, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("Body size string is empty".to_string());
    }

    let (num_part, unit) = match trimmed.chars().last() {
        Some('k') | Some('K') => (&trimmed[..trimmed.len() - 1], 1024),
        Some('m') | Some('M') => (&trimmed[..trimmed.len() - 1], 1024 * 1024),
        Some('g') | Some('G') => (&trimmed[..trimmed.len() - 1], 1024 * 1024 * 1024),
        _ => (trimmed, 1),
    };

    let base: usize = num_part
        .trim()
        .parse()
        .map_err(|_| format!("Invalid number in body size: '{}'", s))?;

    let total = base
        .checked_mul(unit)
        .ok_or_else(|| format!("Body size overflow: '{}'", s))?;

    Ok(total)
}
