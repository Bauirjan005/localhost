use localhost::http::{Method, RequestParser, StatusCode};

#[test]
fn test_parse_simple_get_request() {
    let mut parser = RequestParser::new(1024 * 1024);
    let raw = b"GET /index.html?search=rust&page=1 HTTP/1.1\r\nHost: localhost:8080\r\nUser-Agent: curl/7.88.1\r\nAccept: */*\r\n\r\n";

    let result = parser.feed(raw).expect("Feed should succeed");
    let req = result.expect("Request should be complete");

    assert_eq!(req.method, Method::Get);
    assert_eq!(req.path, "/index.html");
    assert_eq!(req.version, "HTTP/1.1");
    assert_eq!(req.get_header("Host"), Some("localhost:8080"));
    assert_eq!(req.get_header("host"), Some("localhost:8080"));
    assert_eq!(req.get_header("user-agent"), Some("curl/7.88.1"));
    assert_eq!(req.query_string.as_deref(), Some("search=rust&page=1"));
    assert_eq!(req.query_params.get("search").map(|s| s.as_str()), Some("rust"));
    assert_eq!(req.query_params.get("page").map(|s| s.as_str()), Some("1"));
    assert!(req.body.is_empty());
}

#[test]
fn test_parse_post_request_with_body() {
    let mut parser = RequestParser::new(1024 * 1024);
    let raw = b"POST /api/upload HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 19\r\n\r\n{\"status\":\"active\"}";

    let result = parser.feed(raw).expect("Feed should succeed");
    let req = result.expect("Request should be complete");

    assert_eq!(req.method, Method::Post);
    assert_eq!(req.path, "/api/upload");
    assert_eq!(req.content_length(), Some(19));
    assert_eq!(req.body, b"{\"status\":\"active\"}");
}

#[test]
fn test_parse_delete_request() {
    let mut parser = RequestParser::new(1024 * 1024);
    let raw = b"DELETE /files/old_doc.txt HTTP/1.1\r\nHost: localhost\r\n\r\n";

    let result = parser.feed(raw).expect("Feed should succeed");
    let req = result.expect("Request should be complete");

    assert_eq!(req.method, Method::Delete);
    assert_eq!(req.path, "/files/old_doc.txt");
    assert!(req.body.is_empty());
}

#[test]
fn test_incremental_feed_streaming() {
    let mut parser = RequestParser::new(1024 * 1024);
    let part1 = b"POST /submit HTTP/1.1\r\nHost: 127.0.0.1\r";
    let part2 = b"\nContent-Length: 11\r\n\r\nHel";
    let part3 = b"lo World";

    assert_eq!(parser.feed(part1).unwrap(), None);
    assert_eq!(parser.feed(part2).unwrap(), None);
    let req = parser.feed(part3).unwrap().expect("Should complete after 11 bytes");

    assert_eq!(req.body, b"Hello World");
}

#[test]
fn test_url_percent_decoding_and_normalization() {
    let mut parser = RequestParser::new(1024 * 1024);
    let raw = b"GET /hello%20world/test/../final%2Fdoc.pdf HTTP/1.1\r\nHost: localhost\r\n\r\n";

    let req = parser.feed(raw).unwrap().unwrap();
    assert_eq!(req.path, "/hello world/final/doc.pdf");
}

#[test]
fn test_missing_host_header_in_http_1_1() {
    let mut parser = RequestParser::new(1024 * 1024);
    let raw = b"GET / HTTP/1.1\r\nUser-Agent: test\r\n\r\n";

    let err = parser.feed(raw).unwrap_err();
    assert_eq!(err.0, StatusCode::BAD_REQUEST);
}

#[test]
fn test_malformed_request_line() {
    let mut parser = RequestParser::new(1024 * 1024);
    let raw = b"NOT_A_VALID_REQUEST\r\nHost: localhost\r\n\r\n";

    let err = parser.feed(raw).unwrap_err();
    assert_eq!(err.0, StatusCode::BAD_REQUEST);
}

#[test]
fn test_payload_too_large_fixed() {
    let mut parser = RequestParser::new(50); // limit 50 bytes
    let raw = b"POST /upload HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100\r\n\r\n";

    let err = parser.feed(raw).unwrap_err();
    assert_eq!(err.0, StatusCode::PAYLOAD_TOO_LARGE);
}

#[test]
fn test_cookie_parsing() {
    let mut parser = RequestParser::new(1024 * 1024);
    let raw = b"GET /profile HTTP/1.1\r\nHost: localhost\r\nCookie: session_id=abc123xyz; theme=dark; logged_in=true\r\n\r\n";

    let req = parser.feed(raw).unwrap().unwrap();
    assert_eq!(req.cookies.get("session_id").map(|s| s.as_str()), Some("abc123xyz"));
    assert_eq!(req.cookies.get("theme").map(|s| s.as_str()), Some("dark"));
    assert_eq!(req.cookies.get("logged_in").map(|s| s.as_str()), Some("true"));
}
