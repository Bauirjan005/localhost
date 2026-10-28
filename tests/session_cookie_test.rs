use std::time::Duration;
use localhost::http::{Response, StatusCode};
use localhost::session::SessionStore;

#[test]
fn test_session_lifecycle() {
    let mut store = SessionStore::new(Duration::from_secs(60));

    // 1. Initial request without cookie creates session
    let (sess_id_1, is_new_1) = store.get_or_create(None);
    assert!(is_new_1);
    assert_eq!(sess_id_1.len(), 32);

    // 2. Set session data
    {
        let session = store.get_mut(&sess_id_1).unwrap();
        session.set("username", "alice");
        session.set("role", "admin");
    }

    // 3. Second request with cookie restores session
    let (sess_id_2, is_new_2) = store.get_or_create(Some(&sess_id_1));
    assert!(!is_new_2);
    assert_eq!(sess_id_1, sess_id_2);

    let session = store.get(&sess_id_2).unwrap();
    assert_eq!(session.get("username"), Some("alice"));
    assert_eq!(session.get("role"), Some("admin"));

    // 4. Destroy session
    let destroyed = store.destroy(&sess_id_1);
    assert!(destroyed);
    assert!(store.get(&sess_id_1).is_none());
}

#[test]
fn test_response_cookie_generation() {
    let mut res = Response::new(StatusCode::OK);
    res.set_cookie("session_id=12345; Path=/; HttpOnly");
    res.set_cookie("theme=dark; Path=/");

    let bytes = res.to_bytes();
    let as_str = String::from_utf8_lossy(&bytes);

    assert!(as_str.contains("Set-Cookie: session_id=12345; Path=/; HttpOnly\r\n"));
    assert!(as_str.contains("Set-Cookie: theme=dark; Path=/\r\n"));
}
