use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub created_at: Instant,
    pub last_accessed: Instant,
    pub data: HashMap<String, String>,
}

impl Session {
    pub fn new(id: String) -> Self {
        let now = Instant::now();
        Session {
            id,
            created_at: now,
            last_accessed: now,
            data: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(|s| s.as_str())
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.data.insert(key.into(), value.into());
    }

    pub fn touch(&mut self) {
        self.last_accessed = Instant::now();
    }
}

pub struct SessionStore {
    sessions: HashMap<String, Session>,
    session_timeout: Duration,
    last_prune: Instant,
}

impl SessionStore {
    pub fn new(session_timeout: Duration) -> Self {
        SessionStore {
            sessions: HashMap::new(),
            session_timeout,
            last_prune: Instant::now(),
        }
    }

    /// Retrieves an existing session if valid, or creates a new one with a fresh ID.
    /// Returns (session_id, is_new).
    pub fn get_or_create(&mut self, requested_id: Option<&str>) -> (String, bool) {
        self.maybe_prune();

        if let Some(id) = requested_id {
            if let Some(session) = self.sessions.get_mut(id) {
                if session.last_accessed.elapsed() < self.session_timeout {
                    session.touch();
                    return (id.to_string(), false);
                } else {
                    // Session expired, remove it
                    self.sessions.remove(id);
                }
            }
        }

        // Generate new session ID
        let new_id = generate_session_id();
        let session = Session::new(new_id.clone());
        self.sessions.insert(new_id.clone(), session);
        (new_id, true)
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id).filter(|s| s.last_accessed.elapsed() < self.session_timeout)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.maybe_prune();
        self.sessions.get_mut(id).and_then(|s| {
            if s.last_accessed.elapsed() < self.session_timeout {
                s.touch();
                Some(s)
            } else {
                None
            }
        })
    }

    pub fn destroy(&mut self, id: &str) -> bool {
        self.sessions.remove(id).is_some()
    }

    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    fn maybe_prune(&mut self) {
        if self.last_prune.elapsed() > Duration::from_secs(60) {
            let timeout = self.session_timeout;
            self.sessions.retain(|_, s| s.last_accessed.elapsed() < timeout);
            self.last_prune = Instant::now();
        }
    }
}

fn generate_session_id() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let counter = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = now.as_nanos();

    // Generate a pseudo-random hash without external crates
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in &nanos.to_le_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    for &b in &counter.to_le_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    for &b in &pid.to_le_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }

    format!("{:016x}{:016x}", h, (nanos as u64) ^ (counter << 32))
}
