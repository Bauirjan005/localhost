use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use localhost::config::ConfigParser;
use localhost::server::Server;

#[test]
fn test_e2e_server_lifecycle_and_methods() {
    let conf_str = r#"
        server {
            host 127.0.0.1
            listen 18080
            server_name localhost
            client_max_body_size 5M

            location / {
                accepted_methods GET POST DELETE
                root ./www
                index index.html
                autoindex on
            }

            location /upload {
                accepted_methods GET POST DELETE
                upload_dir ./www/uploads
                root ./www/uploads
                autoindex on
            }

            location /redirect-me {
                redirect 301 /
            }
        }
    "#;

    let config = ConfigParser::parse_string(conf_str).expect("Valid config");
    let mut server = Server::new(config).expect("Server init");
    let shutdown_handle = server.shutdown_handle();

    // Start server in background thread
    let server_thread = thread::spawn(move || {
        let _ = server.start();
    });

    // Give server 200ms to bind
    thread::sleep(Duration::from_millis(200));

    // Test 1: GET / (homepage)
    {
        let mut stream = TcpStream::connect("127.0.0.1:18080").expect("Connect to server");
        stream.write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "Expected 200 OK, got: {}", response);
        assert!(response.contains("Content-Type: text/html"), "Expected text/html content-type");
        assert!(response.contains("Localhost"), "Expected HTML body");
    }

    // Test 2: GET /does_not_exist (404)
    {
        let mut stream = TcpStream::connect("127.0.0.1:18080").expect("Connect to server");
        stream.write_all(b"GET /non_existent_page_12345.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 404 Not Found\r\n"), "Expected 404 Not Found, got: {}", response);
    }

    // Test 3: Redirection (301)
    {
        let mut stream = TcpStream::connect("127.0.0.1:18080").expect("Connect to server");
        stream.write_all(b"GET /redirect-me HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 301 Moved Permanently\r\n"));
        assert!(response.contains("Location: /"));
    }

    // Test 4: POST upload raw file
    {
        let mut stream = TcpStream::connect("127.0.0.1:18080").expect("Connect to server");
        let payload = b"Hello from E2E integration test!";
        let req = format!(
            "POST /upload?file=e2e_test.txt HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            payload.len(),
            std::str::from_utf8(payload).unwrap()
        );
        stream.write_all(req.as_bytes()).unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 201 Created\r\n"), "Expected 201 Created, got: {}", response);
    }

    // Test 5: GET the uploaded file
    {
        let mut stream = TcpStream::connect("127.0.0.1:18080").expect("Connect to server");
        stream.write_all(b"GET /upload/e2e_test.txt HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("Hello from E2E integration test!"));
    }

    // Test 6: DELETE the uploaded file
    {
        let mut stream = TcpStream::connect("127.0.0.1:18080").expect("Connect to server");
        stream.write_all(b"DELETE /upload/e2e_test.txt HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    }

    // Test 7: GET deleted file returns 404
    {
        let mut stream = TcpStream::connect("127.0.0.1:18080").expect("Connect to server");
        stream.write_all(b"GET /upload/e2e_test.txt HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 404 Not Found\r\n"));
    }

    // Test 8: Sessions & Cookies
    {
        let mut stream = TcpStream::connect("127.0.0.1:18080").expect("Connect to server");
        stream.write_all(b"GET /session HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("Set-Cookie: session_id="));
    }

    // Clean shutdown
    shutdown_handle.store(false, Ordering::Relaxed);
    let _ = server_thread.join();
}
