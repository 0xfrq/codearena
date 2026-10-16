//! Student and shared JSON endpoints. Identity and team always come from the session cookie.

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, StudentAuth};
use crate::chat::{self, Room};
use crate::engine::{self, PatchFields, ReportFields};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::views;

type ApiResult = AppResult<Json<Value>>;

fn ok(v: Value) -> ApiResult {
    let mut v = v;
    if let Value::Object(m) = &mut v {
        m.insert("ok".into(), Value::Bool(true));
    }
    Ok(Json(v))
}

fn team_of(s: &StudentAuth) -> AppResult<i64> {
    s.team_id.ok_or_else(|| AppError::forbidden("Anda belum tergabung dalam kelompok terkonfirmasi."))
}

#[derive(Deserialize)]
pub struct JoinBody {
    nim: String,
}

pub async fn join(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<JoinBody>) -> AppResult<Response> {
    let existing = auth::student_from_headers(&state, &headers).await?;
    let outcome = auth::join(&state, &b.nim, existing.as_ref()).await?;
    let redirect = if outcome.has_team { "/team" } else { "/wait" };
    let body = Json(json!({ "ok": true, "nim": outcome.nim, "redirect": redirect }));
    Ok(match outcome.token {
        Some(token) => ([auth::set_cookie_header(auth::STUDENT_COOKIE, &token, auth::student_cookie_max_age())], body).into_response(),
        None => body.into_response(),
    })
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    auth::logout(&state, &headers).await?;
    Ok((
        [auth::clear_cookie_header(auth::STUDENT_COOKIE), auth::clear_cookie_header(auth::ADMIN_COOKIE)],
        Json(json!({ "ok": true, "redirect": "/" })),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct SnapshotQuery {
    #[serde(default)]
    view: String,
}

/// Same snapshot the WebSocket sends — useful for debugging and automated checks.
pub async fn snapshot(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<SnapshotQuery>) -> ApiResult {
    let view = if q.view.is_empty() { "arena" } else { q.view.as_str() };
    let viewer = auth::viewer_preferring(&state, &headers, auth::Prefer::parse(view)).await?.ok_or(AppError::Unauthorized)?;
    ok(json!({ "v": state.events.version(), "data": views::snapshot_for(&state, &viewer, view).await? }))
}

#[derive(Deserialize)]
pub struct ChatSendBody {
    room: String,
    body: String,
    /// "student" | "instructor": which login to act as when the browser holds both.
    #[serde(default, rename = "as")]
    as_role: String,
}

pub async fn chat_send(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<ChatSendBody>) -> ApiResult {
    let viewer = auth::viewer_preferring(&state, &headers, auth::Prefer::parse(&b.as_role)).await?.ok_or(AppError::Unauthorized)?;
    let room = Room::parse(&b.room).ok_or_else(|| AppError::bad("Ruang chat tidak dikenal."))?;
    let id = chat::send(&state, &viewer, &room, &b.body).await?;
    ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct ChatHistoryQuery {
    room: String,
    #[serde(default, rename = "as")]
    as_role: String,
}

pub async fn chat_history(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<ChatHistoryQuery>) -> ApiResult {
    let viewer = auth::viewer_preferring(&state, &headers, auth::Prefer::parse(&q.as_role)).await?.ok_or(AppError::Unauthorized)?;
    let room = Room::parse(&q.room).ok_or_else(|| AppError::bad("Ruang chat tidak dikenal."))?;
    let messages = chat::history(&state, &viewer, &room).await?;
    ok(json!({ "room": room.key(), "messages": messages, "instructor": viewer.is_instructor() }))
}

#[derive(Deserialize)]
pub struct StatusBody {
    side: String,
    status: String,
}

pub async fn team_status(State(state): State<AppState>, s: StudentAuth, Json(b): Json<StatusBody>) -> ApiResult {
    engine::set_explicit_status(&state, team_of(&s)?, &s.nim, &b.side, &b.status).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct PrepBody {
    code: String,
    #[serde(default)]
    notes: String,
}

pub async fn prep_save(State(state): State<AppState>, s: StudentAuth, Json(b): Json<PrepBody>) -> ApiResult {
    let saves = engine::save_prep(&state, team_of(&s)?, &s.nim, &b.code, &b.notes).await?;
    ok(json!({ "saves": saves }))
}

pub async fn prep_ready(State(state): State<AppState>, s: StudentAuth) -> ApiResult {
    engine::mark_prep_ready(&state, team_of(&s)?, &s.nim).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct ReportSaveBody {
    #[serde(default)]
    id: Option<i64>,
    #[serde(flatten)]
    fields: ReportFields,
}

pub async fn report_save(State(state): State<AppState>, s: StudentAuth, Json(b): Json<ReportSaveBody>) -> ApiResult {
    let id = engine::save_report(&state, team_of(&s)?, &s.nim, b.id.filter(|i| *i > 0), &b.fields).await?;
    ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct IdBody {
    id: i64,
}

pub async fn report_submit(State(state): State<AppState>, s: StudentAuth, Json(b): Json<IdBody>) -> ApiResult {
    engine::submit_report(&state, team_of(&s)?, &s.nim, b.id).await?;
    ok(json!({}))
}

pub async fn report_delete(State(state): State<AppState>, s: StudentAuth, Json(b): Json<IdBody>) -> ApiResult {
    engine::delete_report_draft(&state, team_of(&s)?, b.id).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct ReviseBody {
    id: i64,
    #[serde(default)]
    revision_reason: String,
    #[serde(flatten)]
    fields: ReportFields,
}

pub async fn report_revise(State(state): State<AppState>, s: StudentAuth, Json(b): Json<ReviseBody>) -> ApiResult {
    engine::revise_report(&state, team_of(&s)?, &s.nim, b.id, &b.fields, &b.revision_reason).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct DefenseReportBody {
    report_id: i64,
    state: String,
    #[serde(default)]
    analysis: String,
}

pub async fn defense_report(State(state): State<AppState>, s: StudentAuth, Json(b): Json<DefenseReportBody>) -> ApiResult {
    engine::update_report_defense(&state, team_of(&s)?, &s.nim, b.report_id, &b.state, &b.analysis).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct RebuttalBody {
    report_id: i64,
    text: String,
}

pub async fn rebuttal(State(state): State<AppState>, s: StudentAuth, Json(b): Json<RebuttalBody>) -> ApiResult {
    engine::submit_rebuttal(&state, team_of(&s)?, &s.nim, b.report_id, &b.text).await?;
    ok(json!({}))
}

pub async fn patch_save(State(state): State<AppState>, s: StudentAuth, Json(b): Json<PatchFields>) -> ApiResult {
    let version = engine::save_patch(&state, team_of(&s)?, &s.nim, &b).await?;
    ok(json!({ "version": version }))
}

pub async fn patch_submit(State(state): State<AppState>, s: StudentAuth) -> ApiResult {
    let version = engine::submit_patch(&state, team_of(&s)?, &s.nim).await?;
    ok(json!({ "version": version }))
}
