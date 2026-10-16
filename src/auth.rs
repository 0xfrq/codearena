//! Identity. Students identify with a roster NIM (NIM is NOT a password — it only proves roster
//! membership; a single active device per NIM is enforced). Instructors use a separate password.
//! Login tokens are random 256-bit values stored only as SHA-256 hashes.

use axum::extract::FromRequestParts;
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue};
use serde_json::json;
use sqlx::SqlitePool;

use crate::db::{self, current_session};
use crate::error::{AppError, AppResult};
use crate::events::{Audience, Viewer};
use crate::state::AppState;
use crate::util::{ct_eq, normalize_nim, now_ms, random_token, sha256_hex};

pub const STUDENT_COOKIE: &str = "ca_student";
pub const ADMIN_COOKIE: &str = "ca_admin";
const STUDENT_TTL_MS: i64 = 24 * 3600 * 1000;
const ADMIN_TTL_MS: i64 = 12 * 3600 * 1000;

pub fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get_all(COOKIE).iter().filter_map(|v| v.to_str().ok()).find_map(|raw| {
        raw.split(';').find_map(|pair| {
            let (k, v) = pair.trim().split_once('=')?;
            (k == name && !v.is_empty()).then(|| v.to_string())
        })
    })
}

pub fn set_cookie_header(name: &str, token: &str, max_age_s: i64) -> (axum::http::HeaderName, HeaderValue) {
    let v = format!("{name}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_s}");
    (SET_COOKIE, HeaderValue::from_str(&v).expect("valid cookie"))
}

pub fn clear_cookie_header(name: &str) -> (axum::http::HeaderName, HeaderValue) {
    set_cookie_header(name, "x", 0)
}

#[derive(Debug, Clone)]
pub struct StudentAuth {
    pub nim: String,
    pub session_id: i64,
    /// Confirmed team only (draft/preview teams are invisible to students).
    pub team_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct InstructorAuth;

pub async fn student_from_headers(state: &AppState, headers: &HeaderMap) -> AppResult<Option<StudentAuth>> {
    let Some(token) = read_cookie(headers, STUDENT_COOKIE) else { return Ok(None) };
    let session = current_session(&state.db).await?;
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT a.nim FROM auth_sessions a JOIN roster r ON r.nim = a.nim AND r.active = 1
         WHERE a.token_hash = ? AND a.role = 'student' AND a.game_session_id = ? AND a.expires_at > ?",
    )
    .bind(sha256_hex(&token))
    .bind(session.id)
    .bind(now_ms())
    .fetch_optional(&state.db)
    .await?;
    let Some((nim,)) = row else { return Ok(None) };
    let team_id = confirmed_team_of(&state.db, session.id, &nim).await?;
    Ok(Some(StudentAuth { nim, session_id: session.id, team_id }))
}

pub async fn confirmed_team_of(pool: &SqlitePool, session_id: i64, nim: &str) -> AppResult<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT tm.team_id FROM team_members tm JOIN teams t ON t.id = tm.team_id
         WHERE tm.session_id = ? AND tm.nim = ? AND t.confirmed = 1",
    )
    .bind(session_id)
    .bind(nim)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn instructor_from_headers(state: &AppState, headers: &HeaderMap) -> AppResult<bool> {
    let Some(token) = read_cookie(headers, ADMIN_COOKIE) else { return Ok(false) };
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM auth_sessions WHERE token_hash = ? AND role = 'instructor' AND expires_at > ?",
    )
    .bind(sha256_hex(&token))
    .bind(now_ms())
    .fetch_optional(&state.db)
    .await?;
    Ok(row.is_some())
}

/// Resolve the viewer for read-only pages and WebSocket connections. Instructor wins if both cookies exist.
pub async fn viewer_from_headers(state: &AppState, headers: &HeaderMap) -> AppResult<Option<Viewer>> {
    if instructor_from_headers(state, headers).await? {
        return Ok(Some(Viewer::Instructor));
    }
    Ok(student_from_headers(state, headers)
        .await?
        .map(|s| Viewer::Student { nim: s.nim, team_id: s.team_id }))
}

/// Which identity a page acts as when one browser holds both an instructor and a student login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefer {
    /// Student pages (team dashboard, waiting room): act as the student.
    Student,
    /// Instructor dashboard: act as the instructor.
    Instructor,
    /// Shared pages (arena): instructor if present, else student.
    Any,
}

impl Prefer {
    pub fn parse(s: &str) -> Prefer {
        match s {
            "student" | "team" | "wait" => Prefer::Student,
            "instructor" | "admin" => Prefer::Instructor,
            _ => Prefer::Any,
        }
    }
}

pub async fn viewer_preferring(state: &AppState, headers: &HeaderMap, prefer: Prefer) -> AppResult<Option<Viewer>> {
    match prefer {
        Prefer::Student => Ok(student_from_headers(state, headers)
            .await?
            .map(|s| Viewer::Student { nim: s.nim, team_id: s.team_id })),
        Prefer::Instructor => Ok(instructor_from_headers(state, headers).await?.then_some(Viewer::Instructor)),
        Prefer::Any => viewer_from_headers(state, headers).await,
    }
}

impl FromRequestParts<AppState> for StudentAuth {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        student_from_headers(state, &parts.headers).await?.ok_or(AppError::Unauthorized)
    }
}

impl FromRequestParts<AppState> for InstructorAuth {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        if instructor_from_headers(state, &parts.headers).await? {
            Ok(InstructorAuth)
        } else {
            Err(AppError::Unauthorized)
        }
    }
}

pub struct JoinOutcome {
    pub token: Option<String>,
    pub nim: String,
    pub has_team: bool,
}

/// Validate a NIM against the roster and register the student in the current session.
pub async fn join(state: &AppState, raw_nim: &str, existing: Option<&StudentAuth>) -> AppResult<JoinOutcome> {
    let nim = normalize_nim(raw_nim).ok_or_else(|| AppError::bad("Format NIM tidak valid."))?;
    if !state.login_limiter.check(&format!("join:{nim}"), 10, 60_000, 0) {
        return Err(AppError::conflict("Terlalu banyak percobaan. Tunggu sebentar."));
    }
    let _w = state.write.lock().await;
    let active: Option<(i64,)> =
        sqlx::query_as("SELECT active FROM roster WHERE nim = ?").bind(&nim).fetch_optional(&state.db).await?;
    match active {
        None => return Err(AppError::forbidden("NIM tidak terdaftar pada roster praktikum.")),
        Some((0,)) => return Err(AppError::forbidden("NIM ini sedang dinonaktifkan oleh instruktur.")),
        _ => {}
    }
    let session = current_session(&state.db).await?;
    let now = now_ms();

    // Same browser re-joining with its own valid cookie: reuse the identity.
    if let Some(e) = existing {
        if e.nim == nim && e.session_id == session.id {
            sqlx::query("UPDATE participants SET last_seen = ? WHERE session_id = ? AND nim = ?")
                .bind(now)
                .bind(session.id)
                .bind(&nim)
                .execute(&state.db)
                .await?;
            return Ok(JoinOutcome { token: None, nim, has_team: e.team_id.is_some() });
        }
    }
    // One NIM = one active participant: refuse a second device while the first is connected.
    if state.presence.is_online(&nim) {
        return Err(AppError::conflict(
            "NIM ini sedang aktif di perangkat lain. Jika ini NIM Anda, minta instruktur melepas perangkat sebelumnya.",
        ));
    }

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO participants (session_id, nim, joined_at, last_seen) VALUES (?, ?, ?, ?)
         ON CONFLICT(session_id, nim) DO UPDATE SET joined_at = COALESCE(participants.joined_at, excluded.joined_at),
         last_seen = excluded.last_seen",
    )
    .bind(session.id)
    .bind(&nim)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    // Only one live login per NIM per session: previous device tokens are revoked.
    sqlx::query("DELETE FROM auth_sessions WHERE role = 'student' AND nim = ?").bind(&nim).execute(&mut *tx).await?;
    let token = random_token();
    sqlx::query(
        "INSERT INTO auth_sessions (token_hash, role, nim, game_session_id, created_at, expires_at) VALUES (?, 'student', ?, ?, ?, ?)",
    )
    .bind(sha256_hex(&token))
    .bind(&nim)
    .bind(session.id)
    .bind(now)
    .bind(now + STUDENT_TTL_MS)
    .execute(&mut *tx)
    .await?;
    db::audit(&mut tx, Some(session.id), &format!("student:{nim}"), "join", "participant", &nim, json!({})).await?;
    tx.commit().await?;
    drop(_w);

    let has_team = confirmed_team_of(&state.db, session.id, &nim).await?.is_some();
    state.events.emit("participant", Audience::All, json!({ "nim": nim, "joined": true, "online": false }));
    Ok(JoinOutcome { token: Some(token), nim, has_team })
}

pub async fn instructor_login(state: &AppState, password: &str) -> AppResult<String> {
    if !state.login_limiter.check("admin-login", 10, 60_000, 300) {
        return Err(AppError::conflict("Terlalu banyak percobaan login. Coba lagi dalam satu menit."));
    }
    let ok = if let Some(env_pw) = &state.config.admin_password_env {
        ct_eq(&sha256_hex(password), &sha256_hex(env_pw))
    } else {
        let salt = db::get_setting(&state.db, "admin_salt").await?.unwrap_or_default();
        let hash = db::get_setting(&state.db, "admin_hash").await?.unwrap_or_default();
        !hash.is_empty() && ct_eq(&sha256_hex(&format!("{salt}:{password}")), &hash)
    };
    if !ok {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        return Err(AppError::forbidden("Password instruktur salah."));
    }
    let token = random_token();
    let now = now_ms();
    let _w = state.write.lock().await;
    sqlx::query(
        "INSERT INTO auth_sessions (token_hash, role, nim, game_session_id, created_at, expires_at) VALUES (?, 'instructor', NULL, NULL, ?, ?)",
    )
    .bind(sha256_hex(&token))
    .bind(now)
    .bind(now + ADMIN_TTL_MS)
    .execute(&state.db)
    .await?;
    sqlx::query("DELETE FROM auth_sessions WHERE expires_at < ?").bind(now).execute(&state.db).await?;
    Ok(token)
}

pub async fn logout(state: &AppState, headers: &HeaderMap) -> AppResult<()> {
    let _w = state.write.lock().await;
    for name in [STUDENT_COOKIE, ADMIN_COOKIE] {
        if let Some(tok) = read_cookie(headers, name) {
            sqlx::query("DELETE FROM auth_sessions WHERE token_hash = ?").bind(sha256_hex(&tok)).execute(&state.db).await?;
        }
    }
    Ok(())
}

pub const fn student_cookie_max_age() -> i64 {
    STUDENT_TTL_MS / 1000
}

pub const fn admin_cookie_max_age() -> i64 {
    ADMIN_TTL_MS / 1000
}
