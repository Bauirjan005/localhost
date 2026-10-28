use localhost::http::{RequestParser, StatusCode};

#[test]
fn test_chunked_body_parsing() {
    let mut parser = RequestParser::new(1024 * 1024);
    let raw = b"POST /chunked HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n6\r\npedia \r\ne\r\nin \r\n\r\nchunks.\r\n0\r\n\r\n";

    let result = parser.feed(raw).expect("Feed should succeed");
    let req = result.expect("Request should be complete");

    assert_eq!(req.body, b"Wikipedia in \r\n\r\nchunks.");
}

#[test]
fn test_chunked_body_with_chunk_extension() {
    let mut parser = RequestParser::new(1024 * 1024);
    let raw = b"POST /test HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n5;foo=bar\r\nHello\r\n6;ext=1\r\n World\r\n0\r\n\r\n";

    let req = parser.feed(raw).unwrap().unwrap();
    assert_eq!(req.body, b"Hello World");
}

#[test]
fn test_empty_chunked_body() {
    let mut parser = RequestParser::new(1024 * 1024);
    let raw = b"POST /empty HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n";

    let req = parser.feed(raw).unwrap().unwrap();
    assert!(req.body.is_empty());
}

#[test]
fn test_chunked_body_exceeding_max_limit() {
    let mut parser = RequestParser::new(10); // limit 10 bytes
    let raw = b"POST /too-big HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n8\r\n12345678\r\n8\r\n12345678\r\n0\r\n\r\n";

    let err = parser.feed(raw).unwrap_err();
    assert_eq!(err.0, StatusCode::PAYLOAD_TOO_LARGE);
}

#[test]
fn test_malformed_chunk_missing_crlf() {
    let mut parser = RequestParser::new(1024);
    let raw = b"POST /bad HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWikiXX"; // missing \r\n

    let err = parser.feed(raw).unwrap_err();
    assert_eq!(err.0, StatusCode::BAD_REQUEST);
}
