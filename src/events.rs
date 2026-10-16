//! Event bus: every state change is committed to SQLite first, then published here.
//! Events carry absolute state (not deltas), so a replayed or duplicated event can never
//! double-count a score. Each event has a monotonically increasing version `v`; clients ignore
//! events whose version is not newer than the snapshot they hold.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast;

use crate::util::now_ms;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Audience {
    /// Everyone connected to the current session (students + instructors).
    All,
    Instructors,
    Students,
    /// Members of one team only.
    Team(i64),
    /// Members of one team plus instructors.
    TeamAndInstructors(i64),
    /// A single student.
    Nim(String),
}

/// Who is looking: resolved from the login cookie on the server, never from the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Viewer {
    Instructor,
    Student { nim: String, team_id: Option<i64> },
}

impl Viewer {
    pub fn is_instructor(&self) -> bool {
        matches!(self, Viewer::Instructor)
    }
}

impl Audience {
    pub fn includes(&self, viewer: &Viewer) -> bool {
        match (self, viewer) {
            (Audience::All, _) => true,
            (Audience::Instructors, Viewer::Instructor) => true,
            (Audience::Students, Viewer::Student { .. }) => true,
            (Audience::Team(t), Viewer::Student { team_id: Some(mine), .. }) => t == mine,
            (Audience::TeamAndInstructors(_), Viewer::Instructor) => true,
            (Audience::TeamAndInstructors(t), Viewer::Student { team_id: Some(mine), .. }) => t == mine,
            (Audience::Nim(n), Viewer::Student { nim, .. }) => n == nim,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Event {
    pub v: u64,
    pub kind: String,
    pub audience: Audience,
    pub data: Value,
}

impl Event {
    /// Structural events invalidate the client's whole view; the WS layer answers them with a
    /// fresh, viewer-specific snapshot instead of forwarding the raw event.
    pub fn is_structural(&self) -> bool {
        self.kind == "structure"
    }
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Arc<Event>>,
    version: Arc<AtomicU64>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        // Start from wall-clock micro-time so versions keep increasing across restarts.
        let start = (now_ms() as u64) * 1000;
        EventBus { tx, version: Arc::new(AtomicU64::new(start)) }
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    pub fn emit(&self, kind: &str, audience: Audience, data: Value) -> u64 {
        let v = self.version.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.tx.send(Arc::new(Event { v, kind: kind.to_string(), audience, data }));
        v
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Event>> {
        self.tx.subscribe()
    }
}

/// Events collected while a DB transaction is open and published only after commit,
/// so clients never see state that was rolled back.
#[derive(Default)]
pub struct Outbox(Vec<(String, Audience, Value)>);

impl Outbox {
    pub fn push(&mut self, kind: &str, audience: Audience, data: Value) {
        self.0.push((kind.to_string(), audience, data));
    }
    pub fn structure(&mut self, reason: &str) {
        self.push("structure", Audience::All, serde_json::json!({ "reason": reason }));
    }
    pub fn flush(self, bus: &EventBus) {
        for (k, a, d) in self.0 {
            bus.emit(&k, a, d);
        }
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn student(nim: &str, team: Option<i64>) -> Viewer {
        Viewer::Student { nim: nim.into(), team_id: team }
    }

    #[test]
    fn audience_filtering() {
        let i = Viewer::Instructor;
        let a = student("1", Some(10));
        let b = student("2", Some(11));
        let w = student("3", None);
        assert!(Audience::All.includes(&w));
        assert!(Audience::Instructors.includes(&i) && !Audience::Instructors.includes(&a));
        assert!(Audience::Team(10).includes(&a) && !Audience::Team(10).includes(&b));
        assert!(!Audience::Team(10).includes(&i));
        assert!(Audience::TeamAndInstructors(10).includes(&i));
        assert!(!Audience::TeamAndInstructors(10).includes(&w));
        assert!(Audience::Nim("2".into()).includes(&b) && !Audience::Nim("2".into()).includes(&a));
        assert!(!Audience::Students.includes(&i));
    }

    #[test]
    fn versions_strictly_increase() {
        let bus = EventBus::new();
        let a = bus.emit("x", Audience::All, Value::Null);
        let b = bus.emit("x", Audience::All, Value::Null);
        assert!(b > a);
        assert_eq!(bus.version(), b);
    }
}
