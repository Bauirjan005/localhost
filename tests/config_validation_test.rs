use localhost::config::{ConfigParser, ConfigValidator, ValidationError};

#[test]
fn test_valid_configuration() {
    let conf_str = r#"
        server {
            listen 8080 8081
            host 127.0.0.1
            server_name example.com
            client_max_body_size 10M

            location / {
                accepted_methods GET POST
                root ./www
                index index.html
                autoindex on
            }
        }
    "#;

    let config = ConfigParser::parse_string(conf_str).expect("Should parse valid config");
    assert_eq!(config.servers.len(), 1);
    assert_eq!(config.servers[0].ports, vec![8080, 8081]);
    assert_eq!(config.servers[0].client_max_body_size, 10 * 1024 * 1024);
    assert!(config.servers[0].is_default);

    ConfigValidator::validate(&config).expect("Validation should succeed");
}

#[test]
fn test_detect_conflicting_server_names_on_same_port() {
    let conf_str = r#"
        server {
            listen 8080
            host 127.0.0.1
            server_name mysite.local
        }
        server {
            listen 8080
            host 127.0.0.1
            server_name mysite.local
        }
    "#;

    let config = ConfigParser::parse_string(conf_str).expect("Parser should parse blocks");
    let errs = ConfigValidator::validate(&config).expect_err("Should fail due to duplicate server_name");

    let has_conflict = errs.iter().any(|e| matches!(e, ValidationError::ConflictingPorts { .. }));
    assert!(has_conflict, "Expected ConflictingPorts error");
}

#[test]
fn test_detect_multiple_unnamed_default_servers_on_same_port() {
    let conf_str = r#"
        server {
            listen 9090
            host 127.0.0.1
        }
        server {
            listen 9090
            host 127.0.0.1
        }
    "#;

    let config = ConfigParser::parse_string(conf_str).unwrap();
    let errs = ConfigValidator::validate(&config).expect_err("Should fail due to multiple default servers");

    let has_conflict = errs.iter().any(|e| matches!(e, ValidationError::ConflictingPorts { .. }));
    assert!(has_conflict);
}

#[test]
fn test_detect_invalid_path_traversal() {
    let conf_str = r#"
        server {
            listen 8085
            host 127.0.0.1
            location /secret {
                root ../../../etc
            }
        }
    "#;

    let config = ConfigParser::parse_string(conf_str).unwrap();
    let errs = ConfigValidator::validate(&config).expect_err("Should fail due to directory traversal");

    let has_invalid_path = errs.iter().any(|e| matches!(e, ValidationError::InvalidPath { .. }));
    assert!(has_invalid_path);
}

#[test]
fn test_detect_out_of_bounds_body_size() {
    let conf_str = r#"
        server {
            listen 8086
            host 127.0.0.1
            client_max_body_size 0
        }
    "#;

    let config = ConfigParser::parse_string(conf_str).unwrap();
    let errs = ConfigValidator::validate(&config).expect_err("Should fail due to zero body size");

    let has_oob = errs.iter().any(|e| matches!(e, ValidationError::OutOfBoundsBodySize { .. }));
    assert!(has_oob);
}

#[test]
fn test_detect_empty_accepted_methods() {
    let conf_str = r#"
        server {
            listen 8087
            host 127.0.0.1
            location /no-methods {
                accepted_methods
            }
        }
    "#;

    let config = ConfigParser::parse_string(conf_str).unwrap();
    let errs = ConfigValidator::validate(&config).expect_err("Should fail due to empty methods");

    let has_empty_methods = errs.iter().any(|e| matches!(e, ValidationError::EmptyMethods { .. }));
    assert!(has_empty_methods);
}

#[test]
fn test_detect_invalid_redirect_status() {
    let conf_str = r#"
        server {
            listen 8088
            host 127.0.0.1
            location /bad-redir {
                redirect 200 /home
            }
        }
    "#;

    let config = ConfigParser::parse_string(conf_str).unwrap();
    let errs = ConfigValidator::validate(&config).expect_err("Should fail due to non-3xx redirect");

    let has_invalid_redir = errs.iter().any(|e| matches!(e, ValidationError::InvalidRedirect { .. }));
    assert!(has_invalid_redir);
}
