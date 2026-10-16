use chrono::{Local, TimeZone};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Current unix time in milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 32 random bytes, hex encoded (64 chars). Used for login tokens.
pub fn random_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn sha256_hex(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    hex::encode(h.finalize())
}

/// Constant-time string comparison (for password hashes).
pub fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Local wall-clock time "HH:MM:SS" for display in server-rendered HTML.
pub fn fmt_time(ms: i64) -> String {
    match Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(t) => t.format("%H:%M:%S").to_string(),
        _ => "-".into(),
    }
}

pub fn fmt_opt_time(ms: Option<i64>) -> String {
    ms.map(fmt_time).unwrap_or_else(|| "-".into())
}

pub fn fmt_datetime(ms: i64) -> String {
    match Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(t) => t.format("%Y-%m-%d %H:%M:%S").to_string(),
        _ => "-".into(),
    }
}

/// "mm:ss" (or "h:mm:ss") for a duration in milliseconds.
pub fn fmt_duration(ms: i64) -> String {
    let s = (ms.max(0) + 999) / 1000;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m:02}:{sec:02}")
    }
}

/// Serialize a value for embedding inside <script type="application/json">.
pub fn json_for_script(v: &serde_json::Value) -> String {
    serde_json::to_string(v)
        .unwrap_or_else(|_| "null".into())
        .replace("</", "<\\/")
        .replace("<!--", "<\\!--")
}

/// Trim and cap a free-text field at `max` characters.
pub fn clean_text(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.chars().count() > max {
        t.chars().take(max).collect()
    } else {
        t.to_string()
    }
}

/// Count `def test_...` functions in a Python test source (static count — tests are NOT executed).
pub fn count_python_tests(src: &str) -> usize {
    src.lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("def test") || t.starts_with("async def test")
        })
        .count()
}

/// A NIM is a short string of ASCII digits/letters.
pub fn normalize_nim(raw: &str) -> Option<String> {
    let n = raw.trim();
    if n.is_empty() || n.len() > 32 || !n.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(n.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_format() {
        assert_eq!(fmt_duration(0), "00:00");
        assert_eq!(fmt_duration(61_000), "01:01");
        assert_eq!(fmt_duration(3_600_000), "1:00:00");
    }

    #[test]
    fn script_json_escapes_closing_tag() {
        let v = serde_json::json!({"x": "</script><script>alert(1)</script>"});
        assert!(!json_for_script(&v).contains("</script>"));
    }

    #[test]
    fn counts_test_functions() {
        let src = "class T:\n    def test_a(self):\n        pass\n    def helper(self):\n        pass\n    def test_b(self): pass\n";
        assert_eq!(count_python_tests(src), 2);
    }

    #[test]
    fn nim_validation() {
        assert_eq!(normalize_nim(" 25032014005 ").as_deref(), Some("25032014005"));
        assert!(normalize_nim("").is_none());
        assert!(normalize_nim("2503'; DROP").is_none());
    }
}
