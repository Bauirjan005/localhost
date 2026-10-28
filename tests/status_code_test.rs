use localhost::config::ConfigParser;
use localhost::http::{RequestParser, Response, StatusCode};
use localhost::router::Router;
use localhost::static_handler::StaticHandler;

#[test]
fn test_status_400_for_malformed_syntax() {
    let mut parser = RequestParser::new(1024);
    let raw = b"GET / HTTP/1.1\r\nBad-Header\r\n\r\n";

    let err = parser.feed(raw).unwrap_err();
    assert_eq!(err.0, StatusCode::BAD_REQUEST);

    let res = Response::default_error(err.0, Some(&err.1));
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    let bytes = res.to_bytes();
    assert!(bytes.starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
}

#[test]
fn test_status_400_for_missing_host() {
    let mut parser = RequestParser::new(1024);
    let raw = b"GET / HTTP/1.1\r\nUser-Agent: test\r\n\r\n";

    let err = parser.feed(raw).unwrap_err();
    assert_eq!(err.0, StatusCode::BAD_REQUEST);
}

#[test]
fn test_status_404_for_missing_file() {
    let conf_str = r#"
        server {
            listen 8080
            host 127.0.0.1
            location / {
                accepted_methods GET
                root ./non_existent_folder_xyz123
            }
        }
    "#;
    let config = ConfigParser::parse_string(conf_str).unwrap();
    let server = &config.servers[0];

    let mut parser = RequestParser::new(1024);
    let req = parser.feed(b"GET /does_not_exist.html HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap().unwrap();

    let err = Router::match_route(server, &req).unwrap_err();
    assert_eq!(err.0, StatusCode::NOT_FOUND);

    let res = StaticHandler::render_error_page(server, err.0, Some(&err.1));
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[test]
fn test_status_403_for_forbidden_directory_listing() {
    let conf_str = r#"
        server {
            listen 8080
            host 127.0.0.1
            location / {
                accepted_methods GET
                root ./src
                autoindex off
            }
        }
    "#;
    let config = ConfigParser::parse_string(conf_str).unwrap();
    let server = &config.servers[0];

    let mut parser = RequestParser::new(1024);
    let req = parser.feed(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap().unwrap();

    let err = Router::match_route(server, &req).unwrap_err();
    assert_eq!(err.0, StatusCode::FORBIDDEN);
}

#[test]
fn test_status_405_for_disallowed_method() {
    let conf_str = r#"
        server {
            listen 8080
            host 127.0.0.1
            location /readonly {
                accepted_methods GET
                root ./www
            }
        }
    "#;
    let config = ConfigParser::parse_string(conf_str).unwrap();
    let server = &config.servers[0];

    let mut parser = RequestParser::new(1024);
    let req = parser.feed(b"DELETE /readonly/file.txt HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap().unwrap();

    let err = Router::match_route(server, &req).unwrap_err();
    assert_eq!(err.0, StatusCode::METHOD_NOT_ALLOWED);
}

#[test]
fn test_status_413_for_payload_too_large() {
    let mut parser = RequestParser::new(100);
    let raw = b"POST /upload HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1000\r\n\r\n";

    let err = parser.feed(raw).unwrap_err();
    assert_eq!(err.0, StatusCode::PAYLOAD_TOO_LARGE);
}

#[test]
fn test_status_505_for_unsupported_version() {
    let mut parser = RequestParser::new(1024);
    let raw = b"GET / HTTP/2.0\r\nHost: localhost\r\n\r\n";

    let err = parser.feed(raw).unwrap_err();
    assert_eq!(err.0, StatusCode::HTTP_VERSION_NOT_SUPPORTED);
}
