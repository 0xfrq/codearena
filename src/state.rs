use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use sqlx::SqlitePool;

use crate::events::EventBus;
use crate::util::now_ms;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    /// Plain password from CODEARENA_ADMIN_PASSWORD (overrides the stored hash when set).
    pub admin_password_env: Option<String>,
    /// Run the auto-advance timer (disabled in some tests).
    pub run_timer: bool,
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir = PathBuf::from(std::env::var("CODEARENA_DATA").unwrap_or_else(|_| "data".into()));
        let db_path = std::env::var("CODEARENA_DB")
            .map(PathBuf::from)
            .unwrap_or_else(|_| data_dir.join("codearena.db"));
        Config {
            bind: std::env::var("CODEARENA_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            data_dir,
            db_path,
            admin_password_env: std::env::var("CODEARENA_ADMIN_PASSWORD").ok().filter(|s| !s.is_empty()),
            run_timer: true,
        }
    }
}

/// In-memory presence: number of open WebSocket connections per student NIM.
/// Presence is *connection* status only; it never drives gameplay status.
#[derive(Default)]
pub struct Presence {
    inner: Mutex<HashMap<String, usize>>,
}

impl Presence {
    /// Returns true when the student just came online (0 -> 1 connections).
    pub fn connect(&self, nim: &str) -> bool {
        let mut m = self.inner.lock().unwrap();
        let c = m.entry(nim.to_string()).or_insert(0);
        *c += 1;
        *c == 1
    }

    /// Returns true when the student just went offline (1 -> 0 connections).
    pub fn disconnect(&self, nim: &str) -> bool {
        let mut m = self.inner.lock().unwrap();
        if let Some(c) = m.get_mut(nim) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                m.remove(nim);
                return true;
            }
        }
        false
    }

    pub fn is_online(&self, nim: &str) -> bool {
        self.inner.lock().unwrap().contains_key(nim)
    }

    pub fn online(&self) -> Vec<String> {
        self.inner.lock().unwrap().keys().cloned().collect()
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

/// Sliding-window rate limiter keyed by sender.
#[derive(Default)]
pub struct RateLimiter {
    inner: Mutex<HashMap<String, VecDeque<i64>>>,
}

impl RateLimiter {
    /// Allow at most `max` events per `window_ms` and enforce a minimum gap of `min_gap_ms`.
    pub fn check(&self, key: &str, max: usize, window_ms: i64, min_gap_ms: i64) -> bool {
        let now = now_ms();
        let mut m = self.inner.lock().unwrap();
        let q = m.entry(key.to_string()).or_default();
        while q.front().is_some_and(|t| now - *t > window_ms) {
            q.pop_front();
        }
        if q.len() >= max || q.back().is_some_and(|t| now - *t < min_gap_ms) {
            return false;
        }
        q.push_back(now);
        true
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub events: EventBus,
    pub presence: Arc<Presence>,
    /// Serialises all writes: SQLite allows a single writer, and taking this lock before
    /// `BEGIN` avoids SQLITE_BUSY upgrades and makes compare-and-set transitions race-free.
    pub write: Arc<tokio::sync::Mutex<()>>,
    pub chat_limiter: Arc<RateLimiter>,
    pub login_limiter: Arc<RateLimiter>,
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(db: SqlitePool, config: Config) -> Self {
        AppState {
            db,
            events: EventBus::new(),
            presence: Arc::new(Presence::default()),
            write: Arc::new(tokio::sync::Mutex::new(())),
            chat_limiter: Arc::new(RateLimiter::default()),
            login_limiter: Arc::new(RateLimiter::default()),
            config: Arc::new(config),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_counts_connections() {
        let p = Presence::default();
        assert!(p.connect("1"));
        assert!(!p.connect("1"));
        assert!(!p.disconnect("1"));
        assert!(p.is_online("1"));
        assert!(p.disconnect("1"));
        assert!(!p.is_online("1"));
    }

    #[test]
    fn rate_limiter_blocks_bursts() {
        let r = RateLimiter::default();
        assert!(r.check("a", 2, 10_000, 0));
        assert!(r.check("a", 2, 10_000, 0));
        assert!(!r.check("a", 2, 10_000, 0));
        assert!(r.check("b", 2, 10_000, 0));
    }
}
