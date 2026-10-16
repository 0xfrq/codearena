//! Three channels: private team chat (`team:<id>`), cross-team `lobby`, instructor `broadcast`.
//! Sender identity always comes from the server-side login, never from the request body.
//! Chat is completely separate from gameplay status.

use serde::Serialize;
use serde_json::json;

use crate::db::{self, current_session};
use crate::error::{AppError, AppResult};
use crate::events::{Audience, Viewer};
use crate::state::AppState;
use crate::util::{clean_text, now_ms};

pub const MAX_LEN: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Room {
    Lobby,
    Broadcast,
    Team(i64),
}

impl Room {
    pub fn parse(s: &str) -> Option<Room> {
        match s {
            "lobby" => Some(Room::Lobby),
            "broadcast" => Some(Room::Broadcast),
            _ => s.strip_prefix("team:").and_then(|id| id.parse().ok()).map(Room::Team),
        }
    }
    pub fn key(&self) -> String {
        match self {
            Room::Lobby => "lobby".into(),
            Room::Broadcast => "broadcast".into(),
            Room::Team(id) => format!("team:{id}"),
        }
    }
    pub fn audience(&self) -> Audience {
        match self {
            Room::Lobby | Room::Broadcast => Audience::All,
            Room::Team(id) => Audience::TeamAndInstructors(*id),
        }
    }
}

pub fn can_read(viewer: &Viewer, room: &Room) -> bool {
    match (viewer, room) {
        (Viewer::Instructor, _) => true,
        (Viewer::Student { .. }, Room::Lobby | Room::Broadcast) => true,
        (Viewer::Student { team_id: Some(mine), .. }, Room::Team(t)) => mine == t,
        _ => false,
    }
}

pub fn can_post(viewer: &Viewer, room: &Room, lobby_locked: bool) -> bool {
    match (viewer, room) {
        (Viewer::Instructor, _) => true,
        (Viewer::Student { .. }, Room::Broadcast) => false,
        (Viewer::Student { .. }, Room::Lobby) => !lobby_locked,
        (Viewer::Student { team_id: Some(mine), .. }, Room::Team(t)) => mine == t,
        _ => false,
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ChatMessage {
    pub id: i64,
    pub room: String,
    pub sender_kind: String,
    pub sender_nim: Option<String>,
    pub sender_label: String,
    pub body: String,
    pub created_at: i64,
    pub deleted: i64,
}

pub async fn history(state: &AppState, viewer: &Viewer, room: &Room) -> AppResult<Vec<ChatMessage>> {
    if !can_read(viewer, room) {
        return Err(AppError::forbidden("Anda tidak memiliki akses ke ruang chat ini."));
    }
    let session = current_session(&state.db).await?;
    let mut rows: Vec<ChatMessage> = sqlx::query_as(
        "SELECT id, room, sender_kind, sender_nim, sender_label, body, created_at, deleted FROM chat_messages
         WHERE session_id = ? AND room = ? ORDER BY id DESC LIMIT 100",
    )
    .bind(session.id)
    .bind(room.key())
    .fetch_all(&state.db)
    .await?;
    rows.reverse();
    if !viewer.is_instructor() {
        for m in rows.iter_mut().filter(|m| m.deleted == 1) {
            m.body = "[pesan dihapus oleh moderator]".into();
        }
    }
    Ok(rows)
}

pub async fn send(state: &AppState, viewer: &Viewer, room: &Room, body: &str) -> AppResult<i64> {
    let body = clean_text(body, MAX_LEN + 1);
    if body.is_empty() {
        return Err(AppError::bad("Pesan kosong."));
    }
    if body.chars().count() > MAX_LEN {
        return Err(AppError::bad(format!("Pesan maksimal {MAX_LEN} karakter.")));
    }
    let session = current_session(&state.db).await?;
    if !can_post(viewer, room, session.lobby_locked == 1) {
        return Err(AppError::forbidden(match room {
            Room::Broadcast => "Hanya instruktur yang dapat mengirim broadcast.",
            Room::Lobby => "Lobby sedang ditutup oleh instruktur.",
            Room::Team(_) => "Anda bukan anggota tim ini.",
        }));
    }
    let (kind, nim, label) = match viewer {
        Viewer::Instructor => ("instructor", None, "Instruktur".to_string()),
        Viewer::Student { nim, team_id } => {
            if !state.chat_limiter.check(nim, 5, 10_000, 700) {
                return Err(AppError::conflict("Terlalu cepat — tunggu beberapa detik sebelum mengirim lagi."));
            }
            let muted: Option<(Option<i64>,)> =
                sqlx::query_as("SELECT muted_until FROM participants WHERE session_id = ? AND nim = ?")
                    .bind(session.id)
                    .bind(nim)
                    .fetch_optional(&state.db)
                    .await?;
            if let Some((Some(until),)) = muted {
                if until > now_ms() {
                    return Err(AppError::forbidden("Anda sedang di-mute oleh instruktur."));
                }
            }
            let team_label = match team_id {
                Some(t) => {
                    let mut c = state.db.acquire().await?;
                    format!(" · {}", db::team_name(&mut c, *t).await?)
                }
                None => String::new(),
            };
            ("student", Some(nim.clone()), format!("{nim}{team_label}"))
        }
    };
    let now = now_ms();
    let _w = state.write.lock().await;
    let id = sqlx::query(
        "INSERT INTO chat_messages (session_id, room, sender_kind, sender_nim, sender_label, body, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(session.id)
    .bind(room.key())
    .bind(kind)
    .bind(&nim)
    .bind(&label)
    .bind(&body)
    .bind(now)
    .execute(&state.db)
    .await?
    .last_insert_rowid();
    drop(_w);
    state.events.emit(
        "chat",
        room.audience(),
        json!({ "id": id, "room": room.key(), "sender_kind": kind, "sender_nim": nim, "sender_label": label, "body": body, "at": now }),
    );
    Ok(id)
}

pub async fn delete(state: &AppState, message_id: i64) -> AppResult<()> {
    let _w = state.write.lock().await;
    let row: Option<(String, i64)> = sqlx::query_as("SELECT room, session_id FROM chat_messages WHERE id = ?")
        .bind(message_id)
        .fetch_optional(&state.db)
        .await?;
    let (room, session_id) = row.ok_or_else(|| AppError::not_found("Pesan tidak ditemukan."))?;
    sqlx::query("UPDATE chat_messages SET deleted = 1, deleted_by = 'instructor' WHERE id = ?").bind(message_id).execute(&state.db).await?;
    let mut conn = state.db.acquire().await?;
    db::audit(&mut conn, Some(session_id), "instructor", "chat_deleted", "chat_message", message_id, json!({ "room": room })).await?;
    drop(_w);
    let audience = Room::parse(&room).map(|r| r.audience()).unwrap_or(Audience::Instructors);
    state.events.emit("chat_deleted", audience, json!({ "id": message_id, "room": room }));
    Ok(())
}

pub async fn mute(state: &AppState, nim: &str, minutes: i64) -> AppResult<()> {
    let minutes = minutes.clamp(0, 240);
    let _w = state.write.lock().await;
    let session = current_session(&state.db).await?;
    let until = if minutes == 0 { None } else { Some(now_ms() + minutes * 60_000) };
    let n = sqlx::query("UPDATE participants SET muted_until = ? WHERE session_id = ? AND nim = ?")
        .bind(until)
        .bind(session.id)
        .bind(nim)
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::not_found("Peserta tidak ditemukan pada sesi ini."));
    }
    let mut conn = state.db.acquire().await?;
    db::audit(&mut conn, Some(session.id), "instructor", "chat_mute", "participant", nim, json!({ "minutes": minutes })).await?;
    drop(_w);
    state.events.emit("chat_muted", Audience::Nim(nim.to_string()), json!({ "until": until }));
    Ok(())
}

pub async fn set_lobby_locked(state: &AppState, locked: bool) -> AppResult<()> {
    let _w = state.write.lock().await;
    let session = current_session(&state.db).await?;
    sqlx::query("UPDATE game_sessions SET lobby_locked = ? WHERE id = ?").bind(locked as i64).bind(session.id).execute(&state.db).await?;
    drop(_w);
    state.events.emit("lobby_locked", Audience::All, json!({ "locked": locked }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_authorization() {
        let i = Viewer::Instructor;
        let a = Viewer::Student { nim: "1".into(), team_id: Some(5) };
        let w = Viewer::Student { nim: "2".into(), team_id: None };
        assert!(can_read(&a, &Room::Team(5)) && !can_read(&a, &Room::Team(6)));
        assert!(!can_read(&w, &Room::Team(5)));
        assert!(can_read(&i, &Room::Team(5)));
        assert!(!can_post(&a, &Room::Broadcast, false));
        assert!(can_post(&i, &Room::Broadcast, false));
        assert!(can_post(&a, &Room::Lobby, false) && !can_post(&a, &Room::Lobby, true));
        assert_eq!(Room::parse("team:12"), Some(Room::Team(12)));
        assert_eq!(Room::parse("team:x"), None);
    }
}
