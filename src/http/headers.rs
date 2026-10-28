use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers {
    // Key in lowercase -> (original_key_display, value)
    entries: HashMap<String, (String, String)>,
}

impl Headers {
    pub fn new() -> Self {
        Headers {
            entries: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let k = key.into();
        let lower = k.to_ascii_lowercase();
        let val = value.into();
        self.entries.insert(lower, (k, val));
    }

    pub fn append_or_insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let k = key.into();
        let lower = k.to_ascii_lowercase();
        let val = value.into();
        if let Some((_, existing)) = self.entries.get_mut(&lower) {
            existing.push_str(", ");
            existing.push_str(&val);
        } else {
            self.entries.insert(lower, (k, val));
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .get(&key.to_ascii_lowercase())
            .map(|(_, v)| v.as_str())
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(&key.to_ascii_lowercase())
    }

    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.entries
            .remove(&key.to_ascii_lowercase())
            .map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.values().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Parses the `Cookie` header into a map of cookie names to values.
    /// Format: `name1=value1; name2=value2`
    pub fn parse_cookies(&self) -> HashMap<String, String> {
        let mut cookies = HashMap::new();
        if let Some(cookie_str) = self.get("cookie") {
            for pair in cookie_str.split(';') {
                let trimmed = pair.trim();
                if let Some(eq_idx) = trimmed.find('=') {
                    let key = trimmed[..eq_idx].trim().to_string();
                    let val = trimmed[eq_idx + 1..].trim().to_string();
                    if !key.is_empty() {
                        cookies.insert(key, val);
                    }
                }
            }
        }
        cookies
    }
}
