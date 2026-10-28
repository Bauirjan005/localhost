use std::path::PathBuf;
use localhost::config::ConfigParser;
use localhost::http::{RequestParser, StatusCode};
use localhost::router::{RouteTarget, Router};

fn setup_test_config() -> localhost::config::Config {
    let conf_str = r#"
        server {
            listen 8080
            host 127.0.0.1
            server_name localhost

            location / {
                accepted_methods GET
                root ./www
                index index.html
                autoindex off
            }

            location /test {
                accepted_methods GET POST
                root ./www/test_root
                index default.html
                autoindex on
            }

            location /test/deep {
                accepted_methods GET
                root ./www/deep_root
            }

            location /old-page {
                redirect 301 /new-page
            }

            location /upload {
                accepted_methods POST
                upload_dir ./www/uploads
            }

            location /cgi-bin {
                accepted_methods GET POST
                root ./www/cgi-bin
                cgi .py python3
                cgi .sh /bin/bash
            }
        }
    "#;

    ConfigParser::parse_string(conf_str).expect("Valid config")
}

#[test]
fn test_longest_prefix_matching() {
    let config = setup_test_config();
    let server = &config.servers[0];

    // Case 1: Route / matches root
    let mut parser = RequestParser::new(1024);
    let req1 = parser.feed(b"GET /about.html HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap().unwrap();
    let route1 = Router::find_longest_prefix_route(server, &req1.path).unwrap();
    assert_eq!(route1.path, "/");

    // Case 2: Route /test/file.html matches /test
    let mut parser = RequestParser::new(1024);
    let req2 = parser.feed(b"GET /test/file.html HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap().unwrap();
    let route2 = Router::find_longest_prefix_route(server, &req2.path).unwrap();
    assert_eq!(route2.path, "/test");

    // Case 3: Route /test/deep/item.html matches /test/deep (longest prefix)
    let mut parser = RequestParser::new(1024);
    let req3 = parser.feed(b"GET /test/deep/item.html HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap().unwrap();
    let route3 = Router::find_longest_prefix_route(server, &req3.path).unwrap();
    assert_eq!(route3.path, "/test/deep");
}

#[test]
fn test_route_redirection() {
    let config = setup_test_config();
    let server = &config.servers[0];

    let mut parser = RequestParser::new(1024);
    let req = parser.feed(b"GET /old-page HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap().unwrap();

    let matched = Router::match_route(server, &req).expect("Route should match");
    match matched.target {
        RouteTarget::Redirect { status, location } => {
            assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
            assert_eq!(location, "/new-page");
        }
        _ => panic!("Expected Redirect target"),
    }
}

#[test]
fn test_route_method_not_allowed() {
    let config = setup_test_config();
    let server = &config.servers[0];

    // Location / only allows GET
    let mut parser = RequestParser::new(1024);
    let req = parser.feed(b"POST /about.html HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n").unwrap().unwrap();

    let err = Router::match_route(server, &req).expect_err("Should reject POST on /");
    assert_eq!(err.0, StatusCode::METHOD_NOT_ALLOWED);
}

#[test]
fn test_route_upload_target() {
    let config = setup_test_config();
    let server = &config.servers[0];

    let mut parser = RequestParser::new(1024);
    let req = parser.feed(b"POST /upload HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n").unwrap().unwrap();

    let matched = Router::match_route(server, &req).unwrap();
    match matched.target {
        RouteTarget::Upload { upload_dir } => {
            assert_eq!(upload_dir, PathBuf::from("./www/uploads"));
        }
        _ => panic!("Expected Upload target"),
    }
}

#[test]
fn test_route_cgi_mapping() {
    let config = setup_test_config();
    let server = &config.servers[0];

    let mut parser = RequestParser::new(1024);
    let req = parser.feed(b"GET /cgi-bin/hello.py/extra/path HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap().unwrap();

    let matched = Router::match_route(server, &req).unwrap();
    match matched.target {
        RouteTarget::Cgi { interpreter, path_info, .. } => {
            assert_eq!(interpreter, "python3");
            assert_eq!(path_info, "/extra/path");
        }
        _ => panic!("Expected CGI target"),
    }
}
