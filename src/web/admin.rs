//! Instructor dashboard: pages, fragments and authorised JSON actions.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use super::pages::{page, patch_views, render, report_views, team_names};
use super::vm::*;
use crate::assignment;
use crate::auth::{self, InstructorAuth};
use crate::challenges::{self, REQUIRED_TOPICS};
use crate::chat;
use crate::db;
use crate::engine::{self, Durations, PatchRow, ReportRow, PATCH_COLS, REPORT_COLS};
use crate::error::{AppError, AppResult};
use crate::events::{Audience, Outbox};
use crate::models::Phase;
use crate::scoring;
use crate::state::AppState;
use crate::util::{fmt_datetime, fmt_duration, fmt_opt_time, fmt_time, json_for_script, normalize_nim, now_ms};

type ApiResult = AppResult<Json<Value>>;

fn ok(v: Value) -> ApiResult {
    let mut v = v;
    if let Value::Object(m) = &mut v {
        m.insert("ok".into(), Value::Bool(true));
    }
    Ok(Json(v))
}

// ------------------------------------------------------------------------------------------------
// Pages

#[derive(Template)]
#[template(path = "admin_login.html")]
struct AdminLoginTpl {
    session_name: String,
}

pub async fn login_page(State(state): State<AppState>) -> AppResult<Response> {
    let s = db::current_session(&state.db).await?;
    Ok(page(&AdminLoginTpl { session_name: s.name }))
}

#[derive(Template)]
#[template(path = "admin.html")]
struct AdminTpl {
    session_name: String,
    frags: Vec<(&'static str, &'static str, String)>,
    boot_json: String,
}

pub const ADMIN_FRAGS: &[(&str, &str)] = &[
    ("round", "Ronde & Fase"),
    ("teams", "Waiting Room & Kelompok"),
    ("review", "Review"),
    ("scores", "Skor & Koreksi"),
    ("chat", "Chat & Broadcast"),
    ("roster", "Roster"),
    ("session", "Sesi & Ekspor"),
    ("audit", "Audit Log"),
];

pub async fn dashboard(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    if !auth::instructor_from_headers(&state, &headers).await? {
        return Ok(Redirect::to("/admin/login").into_response());
    }
    let s = db::current_session(&state.db).await?;
    let mut frags = Vec::new();
    for (key, label) in ADMIN_FRAGS {
        frags.push((*key, *label, admin_fragment(&state, key).await?));
    }
    Ok(page(&AdminTpl { session_name: s.name, frags, boot_json: json_for_script(&json!({ "instructor": true })) }))
}

pub async fn fragment_handler(State(state): State<AppState>, _a: InstructorAuth, Path(name): Path<String>) -> AppResult<Html<String>> {
    Ok(Html(admin_fragment(&state, &name).await?))
}

// ------------------------------------------------------------------------------------------------
// Fragments

#[derive(Template)]
#[template(path = "admin_session.html")]
struct SessionFrag {
    id: i64,
    name: String,
    status: String,
    teams_locked: bool,
    created: String,
    archived: Vec<(i64, String, String)>,
    password_from_env: bool,
}

#[derive(Template)]
#[template(path = "admin_roster.html")]
struct RosterFrag {
    rows: Vec<(String, String, bool, bool, bool, String)>,
    active: usize,
}

#[derive(Clone)]
struct TeamCard {
    id: i64,
    name: String,
    confirmed: bool,
    members: Vec<MemberView>,
}

#[derive(Template)]
#[template(path = "admin_teams.html")]
struct TeamsFrag {
    locked: bool,
    drafts: Vec<TeamCard>,
    confirmed: Vec<TeamCard>,
    waiting: Vec<MemberView>,
    not_joined: Vec<String>,
    history: Vec<(String, String, String, String)>,
}

#[derive(Clone)]
struct PairRow {
    attacker_id: i64,
    attacker: String,
    target_id: i64,
    target: String,
    locked: bool,
}

#[derive(Clone)]
struct PhaseRow {
    label: String,
    state: String,
    started: String,
    ended: String,
    planned: String,
    actual: String,
    submissions: i64,
}

#[derive(Template)]
#[template(path = "admin_round.html")]
struct RoundFrag {
    has_round: bool,
    round_id: i64,
    number: i64,
    state: String,
    phase: String,
    phase_label: String,
    next_label: String,
    paused: bool,
    remaining: String,
    timed: bool,
    discussion: bool,
    challenge_label: String,
    pairs: Vec<PairRow>,
    teams: Vec<(i64, String)>,
    timeline: Vec<PhaseRow>,
    can_create: bool,
    confirmed_teams: usize,
    challenges: Vec<(String, String, String)>,
    defaults: Durations,
    open_reports: i64,
    open_patches: i64,
    history: Vec<(i64, String, String, String)>,
}

#[derive(Template)]
#[template(path = "admin_review.html")]
struct ReviewFrag {
    has_round: bool,
    round_number: i64,
    reports: Vec<ReportView>,
    patches: Vec<PatchView>,
    pending_reports: usize,
    pending_patches: usize,
}

#[derive(Template)]
#[template(path = "admin_scores.html")]
struct ScoresFrag {
    rows: Vec<ScoreRowView>,
    adjustments: Vec<(String, String, i64, String, String)>,
    teams: Vec<(i64, String)>,
    round_id: i64,
    finalized_rounds: Vec<(i64, Vec<(String, i64, i64, i64, i64)>)>,
}

#[derive(Template)]
#[template(path = "admin_chat.html")]
struct ChatFrag {
    rooms: Vec<(String, String)>,
    lobby_locked: bool,
    participants: Vec<(String, String)>,
}

#[derive(Template)]
#[template(path = "admin_audit.html")]
struct AuditFrag {
    rows: Vec<(String, String, String, String, String)>,
}

pub async fn admin_fragment(state: &AppState, name: &str) -> AppResult<String> {
    let s = db::current_session(&state.db).await?;
    let names = team_names(&state.db, s.id).await?;
    match name {
        "session" => {
            let archived: Vec<(i64, String, Option<i64>)> =
                sqlx::query_as("SELECT id, name, archived_at FROM game_sessions WHERE status = 'archived' ORDER BY id DESC LIMIT 20")
                    .fetch_all(&state.db)
                    .await?;
            render(&SessionFrag {
                id: s.id,
                name: s.name.clone(),
                status: s.status.clone(),
                teams_locked: s.teams_locked == 1,
                created: fmt_datetime(s.created_at),
                archived: archived.into_iter().map(|(id, n, at)| (id, n, at.map(fmt_datetime).unwrap_or_default())).collect(),
                password_from_env: state.config.admin_password_env.is_some(),
            })
        }
        "roster" => {
            let rows: Vec<(String, String, i64, Option<i64>, Option<String>)> = sqlx::query_as(
                "SELECT r.nim, r.name, r.active, p.joined_at, t.name FROM roster r
                 LEFT JOIN participants p ON p.nim = r.nim AND p.session_id = ?
                 LEFT JOIN team_members tm ON tm.nim = r.nim AND tm.session_id = ?
                 LEFT JOIN teams t ON t.id = tm.team_id ORDER BY r.nim",
            )
            .bind(s.id)
            .bind(s.id)
            .fetch_all(&state.db)
            .await?;
            let active = rows.iter().filter(|r| r.2 == 1).count();
            render(&RosterFrag {
                rows: rows
                    .into_iter()
                    .map(|(nim, name, active, joined, team)| {
                        let online = state.presence.is_online(&nim);
                        (nim, name, active == 1, joined.is_some(), online, team.unwrap_or_default())
                    })
                    .collect(),
                active,
            })
        }
        "teams" => {
            let teams: Vec<(i64, String, i64)> =
                sqlx::query_as("SELECT id, name, confirmed FROM teams WHERE session_id = ? ORDER BY id").bind(s.id).fetch_all(&state.db).await?;
            let members: Vec<(i64, String, Option<i64>)> = sqlx::query_as(
                "SELECT tm.team_id, tm.nim, p.joined_at FROM team_members tm
                 LEFT JOIN participants p ON p.session_id = tm.session_id AND p.nim = tm.nim WHERE tm.session_id = ? ORDER BY tm.nim",
            )
            .bind(s.id)
            .fetch_all(&state.db)
            .await?;
            let cards: Vec<TeamCard> = teams
                .iter()
                .map(|(id, name, confirmed)| TeamCard {
                    id: *id,
                    name: name.clone(),
                    confirmed: *confirmed == 1,
                    members: members
                        .iter()
                        .filter(|m| m.0 == *id)
                        .map(|m| MemberView { nim: m.1.clone(), online: state.presence.is_online(&m.1), joined: m.2.is_some() })
                        .collect(),
                })
                .collect();
            let waiting: Vec<(String,)> = sqlx::query_as(
                "SELECT p.nim FROM participants p WHERE p.session_id = ? AND p.joined_at IS NOT NULL
                   AND NOT EXISTS (SELECT 1 FROM team_members tm WHERE tm.session_id = p.session_id AND tm.nim = p.nim) ORDER BY p.joined_at",
            )
            .bind(s.id)
            .fetch_all(&state.db)
            .await?;
            let not_joined: Vec<(String,)> = sqlx::query_as(
                "SELECT r.nim FROM roster r WHERE r.active = 1 AND NOT EXISTS
                   (SELECT 1 FROM participants p WHERE p.session_id = ? AND p.nim = r.nim AND p.joined_at IS NOT NULL)
                   AND NOT EXISTS (SELECT 1 FROM team_members tm WHERE tm.session_id = ? AND tm.nim = r.nim) ORDER BY r.nim",
            )
            .bind(s.id)
            .bind(s.id)
            .fetch_all(&state.db)
            .await?;
            let history: Vec<(String, Option<i64>, Option<String>, String, i64)> = sqlx::query_as(
                "SELECT action, team_id, nim, detail, created_at FROM team_history WHERE session_id = ? ORDER BY id DESC LIMIT 40",
            )
            .bind(s.id)
            .fetch_all(&state.db)
            .await?;
            render(&TeamsFrag {
                locked: s.teams_locked == 1,
                drafts: cards.iter().filter(|c| !c.confirmed).cloned().collect(),
                confirmed: cards.iter().filter(|c| c.confirmed).cloned().collect(),
                waiting: waiting
                    .into_iter()
                    .map(|(nim,)| MemberView { online: state.presence.is_online(&nim), joined: true, nim })
                    .collect(),
                not_joined: not_joined.into_iter().map(|r| r.0).collect(),
                history: history
                    .into_iter()
                    .map(|(action, team, nim, detail, at)| {
                        let team = team.and_then(|t| names.get(&t).cloned()).unwrap_or_default();
                        (fmt_time(at), action, format!("{team} {}", nim.unwrap_or_default()).trim().to_string(), detail)
                    })
                    .collect(),
            })
        }
        "round" => round_fragment(state, &s, &names).await,
        "review" => {
            let mut conn = state.db.acquire().await?;
            let round = engine::current_round(&mut conn, s.id).await?.filter(|r| r.state != "setup");
            drop(conn);
            let Some(r) = round else {
                return render(&ReviewFrag { has_round: false, round_number: 0, reports: vec![], patches: vec![], pending_reports: 0, pending_patches: 0 });
            };
            let rows = sqlx::query_as::<_, ReportRow>(&format!(
                "SELECT {REPORT_COLS} FROM attack_reports WHERE round_id = ? AND status <> 'draft'
                 ORDER BY CASE WHEN status IN ('submitted', 'pending_review') THEN 0 ELSE 1 END, submitted_at"
            ))
            .bind(r.id)
            .fetch_all(&state.db)
            .await?;
            let patches = sqlx::query_as::<_, PatchRow>(&format!(
                "SELECT {PATCH_COLS} FROM patches WHERE round_id = ? AND status <> 'draft'
                 ORDER BY CASE WHEN status = 'submitted' THEN 0 ELSE 1 END, submitted_at"
            ))
            .bind(r.id)
            .fetch_all(&state.db)
            .await?;
            let reports = report_views(state, &rows, s.id, true).await?;
            let patches = patch_views(state, &patches, s.id).await?;
            render(&ReviewFrag {
                has_round: true,
                round_number: r.number,
                pending_reports: reports.iter().filter(|r| r.awaiting_review).count(),
                pending_patches: patches.iter().filter(|p| p.awaiting_review).count(),
                reports,
                patches,
            })
        }
        "scores" => {
            let scores = scoring::session_scores(&state.db, s.id).await?;
            let rows = scores
                .iter()
                .map(|sc| ScoreRowView {
                    rank: sc.rank,
                    team_id: sc.team_id,
                    team_name: names.get(&sc.team_id).cloned().unwrap_or_default(),
                    attack: sc.round.attack,
                    defense: sc.round.defense,
                    rebuttal: sc.round.rebuttal,
                    round_total: sc.round.total,
                    adjustments: sc.adjustments,
                    cumulative: sc.cumulative,
                    valid_findings: sc.valid_findings_total,
                    accepted_patches: sc.accepted_patches_total,
                    finalized: sc.round_finalized,
                })
                .collect();
            let adj: Vec<(i64, i64, Option<i64>, String, i64)> = sqlx::query_as(
                "SELECT team_id, points, round_id, reason, created_at FROM score_adjustments WHERE session_id = ? ORDER BY id DESC",
            )
            .bind(s.id)
            .fetch_all(&state.db)
            .await?;
            let mut conn = state.db.acquire().await?;
            let round = engine::current_round(&mut conn, s.id).await?;
            drop(conn);
            let finalized: Vec<(i64, i64, i64, i64, i64, i64)> = sqlx::query_as(
                "SELECT r.number, rs.team_id, rs.attack, rs.defense, rs.rebuttal, rs.total FROM round_scores rs
                 JOIN rounds r ON r.id = rs.round_id WHERE r.session_id = ? ORDER BY r.number, rs.total DESC",
            )
            .bind(s.id)
            .fetch_all(&state.db)
            .await?;
            let mut by_round: Vec<(i64, Vec<(String, i64, i64, i64, i64)>)> = Vec::new();
            for (n, team, a, d, rb, t) in finalized {
                let entry = (names.get(&team).cloned().unwrap_or_default(), a, d, rb, t);
                match by_round.last_mut() {
                    Some((num, list)) if *num == n => list.push(entry),
                    _ => by_round.push((n, vec![entry])),
                }
            }
            let mut teams: Vec<(i64, String)> = names.iter().map(|(k, v)| (*k, v.clone())).collect();
            teams.sort();
            render(&ScoresFrag {
                rows,
                adjustments: adj
                    .into_iter()
                    .map(|(team, pts, round_id, reason, at)| {
                        (fmt_time(at), names.get(&team).cloned().unwrap_or_default(), pts, round_id.map(|r| format!("#{r}")).unwrap_or("-".into()), reason)
                    })
                    .collect(),
                teams,
                round_id: round.map(|r| r.id).unwrap_or(0),
                finalized_rounds: by_round,
            })
        }
        "chat" => {
            let mut rooms = vec![("lobby".to_string(), "Cross-team lobby".to_string()), ("broadcast".to_string(), "Instructor broadcast".to_string())];
            let mut team_list: Vec<(i64, String)> = names.iter().map(|(k, v)| (*k, v.clone())).collect();
            team_list.sort();
            for (id, n) in team_list {
                rooms.push((format!("team:{id}"), format!("Team chat — {n}")));
            }
            let participants: Vec<(String, Option<i64>)> =
                sqlx::query_as("SELECT nim, muted_until FROM participants WHERE session_id = ? AND joined_at IS NOT NULL ORDER BY nim")
                    .bind(s.id)
                    .fetch_all(&state.db)
                    .await?;
            render(&ChatFrag {
                rooms,
                lobby_locked: s.lobby_locked == 1,
                participants: participants
                    .into_iter()
                    .map(|(n, m)| {
                        let muted = m.filter(|u| *u > now_ms()).map(|u| format!("muted s/d {}", fmt_time(u))).unwrap_or_default();
                        (n, muted)
                    })
                    .collect(),
            })
        }
        "audit" => {
            let rows: Vec<(String, String, String, Option<String>, String, i64)> = sqlx::query_as(
                "SELECT actor, action, entity, entity_id, detail, created_at FROM audit_log WHERE session_id = ? OR session_id IS NULL ORDER BY id DESC LIMIT 200",
            )
            .bind(s.id)
            .fetch_all(&state.db)
            .await?;
            render(&AuditFrag {
                rows: rows
                    .into_iter()
                    .map(|(actor, action, entity, id, detail, at)| (fmt_time(at), actor, action, format!("{entity} {}", id.unwrap_or_default()), detail))
                    .collect(),
            })
        }
        _ => Err(AppError::not_found("Fragment tidak dikenal.")),
    }
}

async fn round_fragment(state: &AppState, s: &db::SessionRow, names: &HashMap<i64, String>) -> AppResult<String> {
    let mut conn = state.db.acquire().await?;
    let round = engine::current_round(&mut conn, s.id).await?;
    let (confirmed,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM teams t WHERE t.session_id = ? AND t.confirmed = 1 AND EXISTS (SELECT 1 FROM team_members tm WHERE tm.team_id = t.id)",
    )
    .bind(s.id)
    .fetch_one(&mut *conn)
    .await?;
    let history: Vec<(i64, String, String, Option<i64>)> =
        sqlx::query_as("SELECT number, challenge_id, state, finalized_at FROM rounds WHERE session_id = ? ORDER BY number DESC")
            .bind(s.id)
            .fetch_all(&mut *conn)
            .await?;
    let bank = challenges::load_all(&state.db).await?;
    let mut f = RoundFrag {
        has_round: false,
        round_id: 0,
        number: 0,
        state: String::new(),
        phase: String::new(),
        phase_label: String::new(),
        next_label: String::new(),
        paused: false,
        remaining: String::new(),
        timed: false,
        discussion: false,
        challenge_label: String::new(),
        pairs: vec![],
        teams: {
            let mut t: Vec<(i64, String)> = names.iter().map(|(k, v)| (*k, v.clone())).collect();
            t.sort();
            t
        },
        timeline: vec![],
        can_create: round.as_ref().is_none_or(|r| r.state == "finalized"),
        confirmed_teams: confirmed as usize,
        challenges: bank.iter().map(|c| (c.meta.id.clone(), c.label(), c.materi_label())).collect(),
        defaults: Durations::default(),
        open_reports: 0,
        open_patches: 0,
        history: history
            .into_iter()
            .map(|(n, c, st, fin)| (n, c, st, fin.map(fmt_time).unwrap_or_default()))
            .collect(),
    };
    if let Some(r) = round.filter(|r| r.state != "finalized" || true) {
        let now = now_ms();
        let pairs = engine::pairings(&mut conn, r.id).await?;
        let (open_reports,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM attack_reports WHERE round_id = ? AND status IN ('submitted', 'pending_review', 'needs_revision')",
        )
        .bind(r.id)
        .fetch_one(&mut *conn)
        .await?;
        let (open_patches,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM patches WHERE round_id = ? AND status = 'submitted'").bind(r.id).fetch_one(&mut *conn).await?;
        drop(conn);
        let c = challenges::load(&state.db, &r.challenge_id).await?;
        let rv = crate::views::round_view(state, &r).await?;
        f.has_round = true;
        f.round_id = r.id;
        f.number = r.number;
        f.state = r.state.clone();
        f.phase = r.phase.clone();
        f.phase_label = r.phase().label().into();
        f.next_label = r.phase().next().map(|p| p.label().to_string()).unwrap_or_default();
        f.paused = r.is_paused();
        f.timed = r.phase_deadline.is_some();
        f.remaining = r.remaining_ms(now).map(fmt_duration).unwrap_or_default();
        f.discussion = r.discussion_open == 1;
        f.challenge_label = c.label();
        f.open_reports = open_reports;
        f.open_patches = open_patches;
        f.pairs = pairs
            .iter()
            .map(|(a, t, l)| PairRow {
                attacker_id: *a,
                attacker: names.get(a).cloned().unwrap_or_default(),
                target_id: *t,
                target: names.get(t).cloned().unwrap_or_default(),
                locked: *l == 1,
            })
            .collect();
        let pair_ids: Vec<i64> = pairs.iter().map(|p| p.0).collect();
        f.teams.retain(|(id, _)| pair_ids.contains(id));
        if let Some(tl) = rv["timeline"].as_array() {
            f.timeline = tl
                .iter()
                .map(|p| PhaseRow {
                    label: p["label"].as_str().unwrap_or("").into(),
                    state: p["state"].as_str().unwrap_or("").into(),
                    started: fmt_opt_time(p["started_at"].as_i64()),
                    ended: fmt_opt_time(p["ended_at"].as_i64()),
                    planned: match p["planned_secs"].as_i64().unwrap_or(0) {
                        0 => "manual".into(),
                        secs => fmt_duration(secs * 1000),
                    },
                    actual: p["actual_ms"].as_i64().map(fmt_duration).unwrap_or("-".into()),
                    submissions: p["submissions"].as_i64().unwrap_or(0),
                })
                .collect();
        }
    }
    render(&f)
}

// ------------------------------------------------------------------------------------------------
// Challenge bank pages (instructor answer keys)

#[derive(Template)]
#[template(path = "admin_challenges.html")]
struct ChallengesTpl {
    session_name: String,
    rows: Vec<(String, String, String, usize, bool)>,
    coverage: Vec<(u8, String, Vec<String>)>,
}

pub async fn challenges_page(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    if !auth::instructor_from_headers(&state, &headers).await? {
        return Ok(Redirect::to("/admin/login").into_response());
    }
    let s = db::current_session(&state.db).await?;
    let bank = challenges::load_all(&state.db).await?;
    let coverage = REQUIRED_TOPICS
        .iter()
        .map(|(m, t)| (*m, t.to_string(), bank.iter().filter(|c| c.meta.topics.iter().any(|x| x == t)).map(|c| c.meta.id.clone()).collect()))
        .collect();
    let rows = bank.iter().map(|c| (c.meta.id.clone(), c.meta.title.clone(), c.materi_label(), c.meta.bugs.len(), c.meta.capstone)).collect();
    Ok(page(&ChallengesTpl { session_name: s.name, rows, coverage }))
}

#[derive(Template)]
#[template(path = "admin_challenge.html")]
struct ChallengeTpl {
    session_name: String,
    frag: String,
}

pub async fn challenge_page(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> AppResult<Response> {
    if !auth::instructor_from_headers(&state, &headers).await? {
        return Ok(Redirect::to("/admin/login").into_response());
    }
    let s = db::current_session(&state.db).await?;
    let c = challenges::load(&state.db, &id).await?;
    let frag = render(&super::pages::FragChallenge { challenge: Some(ChallengeView::new(&c, true)), round_number: 0, discussion: true, all_reports: vec![] })?;
    Ok(page(&ChallengeTpl { session_name: s.name, frag }))
}

// ------------------------------------------------------------------------------------------------
// JSON actions

#[derive(Deserialize)]
pub struct LoginBody {
    password: String,
}

pub async fn login(State(state): State<AppState>, Json(b): Json<LoginBody>) -> AppResult<Response> {
    let token = auth::instructor_login(&state, &b.password).await?;
    let cookie = auth::set_cookie_header(auth::ADMIN_COOKIE, &token, auth::admin_cookie_max_age());
    Ok(([cookie], Json(json!({ "ok": true, "redirect": "/admin" }))).into_response())
}

#[derive(Deserialize)]
pub struct NameBody {
    name: String,
}

pub async fn session_rename(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<NameBody>) -> ApiResult {
    let name: String = b.name.trim().chars().take(80).collect();
    if name.is_empty() {
        return Err(AppError::bad("Nama sesi kosong."));
    }
    let _w = state.write.lock().await;
    let s = db::current_session(&state.db).await?;
    sqlx::query("UPDATE game_sessions SET name = ? WHERE id = ?").bind(&name).bind(s.id).execute(&state.db).await?;
    drop(_w);
    let mut out = Outbox::default();
    out.structure("session_renamed");
    out.flush(&state.events);
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct ResetBody {
    confirm: String,
    #[serde(default)]
    name: String,
}

/// Archive the current session (all data kept for export) and open a fresh one.
pub async fn session_reset(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<ResetBody>) -> ApiResult {
    if b.confirm != "RESET" {
        return Err(AppError::bad("Ketik RESET untuk mengonfirmasi."));
    }
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let s = db::current_session_conn(&mut tx).await?;
    let (running,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM rounds WHERE session_id = ? AND state = 'running'")
        .bind(s.id)
        .fetch_one(&mut *tx)
        .await?;
    let now = now_ms();
    sqlx::query("UPDATE rounds SET state = 'cancelled' WHERE session_id = ? AND state = 'setup'").bind(s.id).execute(&mut *tx).await?;
    sqlx::query("UPDATE game_sessions SET status = 'archived', archived_at = ? WHERE id = ?").bind(now).bind(s.id).execute(&mut *tx).await?;
    let name = if b.name.trim().is_empty() { s.name.clone() } else { b.name.trim().chars().take(80).collect() };
    let new_id = sqlx::query("INSERT INTO game_sessions (name, created_at) VALUES (?, ?)").bind(&name).bind(now).execute(&mut *tx).await?.last_insert_rowid();
    sqlx::query("DELETE FROM auth_sessions WHERE role = 'student'").execute(&mut *tx).await?;
    db::audit(&mut tx, Some(s.id), "instructor", "session_reset", "game_session", s.id, json!({ "new_session": new_id, "running_rounds_archived": running }))
        .await?;
    tx.commit().await?;
    drop(_w);
    state.events.emit("force_logout", Audience::Students, json!({ "reason": "Sesi direset oleh instruktur. Silakan join kembali." }));
    let mut out = Outbox::default();
    out.structure("session_reset");
    out.flush(&state.events);
    ok(json!({ "session_id": new_id }))
}

#[derive(Deserialize)]
pub struct RosterAddBody {
    text: String,
}

pub async fn roster_add(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<RosterAddBody>) -> ApiResult {
    let mut added = vec![];
    let mut invalid = vec![];
    let _w = state.write.lock().await;
    let now = now_ms();
    for line in b.text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (nim_raw, name) = line.split_once([',', ';', '\t']).unwrap_or((line, ""));
        match normalize_nim(nim_raw) {
            Some(nim) => {
                sqlx::query(
                    "INSERT INTO roster (nim, name, active, added_at) VALUES (?, ?, 1, ?)
                     ON CONFLICT(nim) DO UPDATE SET active = 1, name = CASE WHEN excluded.name <> '' THEN excluded.name ELSE roster.name END",
                )
                .bind(&nim)
                .bind(name.trim().chars().take(80).collect::<String>())
                .bind(now)
                .execute(&state.db)
                .await?;
                added.push(nim);
            }
            None => invalid.push(line.to_string()),
        }
    }
    let mut conn = state.db.acquire().await?;
    db::audit(&mut conn, None, "instructor", "roster_add", "roster", "", json!({ "added": added, "invalid": invalid })).await?;
    ok(json!({ "added": added.len(), "invalid": invalid }))
}

#[derive(Deserialize)]
pub struct RosterSetBody {
    nim: String,
    active: bool,
}

pub async fn roster_set(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<RosterSetBody>) -> ApiResult {
    let _w = state.write.lock().await;
    let n = sqlx::query("UPDATE roster SET active = ? WHERE nim = ?").bind(b.active as i64).bind(&b.nim).execute(&state.db).await?.rows_affected();
    if n == 0 {
        return Err(AppError::not_found("NIM tidak ada di roster."));
    }
    if !b.active {
        sqlx::query("DELETE FROM auth_sessions WHERE role = 'student' AND nim = ?").bind(&b.nim).execute(&state.db).await?;
    }
    let mut conn = state.db.acquire().await?;
    db::audit(&mut conn, None, "instructor", "roster_set_active", "roster", &b.nim, json!({ "active": b.active })).await?;
    drop(_w);
    if !b.active {
        state.events.emit("force_logout", Audience::Nim(b.nim.clone()), json!({ "reason": "NIM dinonaktifkan oleh instruktur." }));
    }
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct NimBody {
    nim: String,
    #[serde(default)]
    reason: String,
}

/// Revoke a student's login so the NIM can join from another device (e.g. lost laptop).
pub async fn release_device(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<NimBody>) -> ApiResult {
    let _w = state.write.lock().await;
    sqlx::query("DELETE FROM auth_sessions WHERE role = 'student' AND nim = ?").bind(&b.nim).execute(&state.db).await?;
    let mut conn = state.db.acquire().await?;
    let s = db::current_session_conn(&mut conn).await?;
    db::audit(&mut conn, Some(s.id), "instructor", "release_device", "participant", &b.nim, json!({})).await?;
    drop(_w);
    state.events.emit("force_logout", Audience::Nim(b.nim.clone()), json!({ "reason": "Perangkat dilepas oleh instruktur." }));
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct RandomBody {
    #[serde(default = "default_target")]
    target_size: usize,
    #[serde(default)]
    include_offline: bool,
}

fn default_target() -> usize {
    4
}

pub async fn teams_random(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<RandomBody>) -> ApiResult {
    let preview = assignment::random_preview(&state, b.target_size, b.include_offline).await?;
    ok(json!({ "preview": preview }))
}

pub async fn teams_create(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<NameBody>) -> ApiResult {
    let id = assignment::create_team(&state, &b.name).await?;
    ok(json!({ "team_id": id }))
}

#[derive(Deserialize)]
pub struct TeamRenameBody {
    team_id: i64,
    name: String,
}

pub async fn teams_rename(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<TeamRenameBody>) -> ApiResult {
    assignment::rename_team(&state, b.team_id, &b.name).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct TeamIdBody {
    team_id: i64,
}

pub async fn teams_delete(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<TeamIdBody>) -> ApiResult {
    assignment::delete_team(&state, b.team_id).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct MemberBody {
    team_id: i64,
    nim: String,
    #[serde(default)]
    reason: String,
}

pub async fn teams_set_member(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<MemberBody>) -> ApiResult {
    assignment::set_member(&state, b.team_id, &b.nim, &b.reason).await?;
    ok(json!({}))
}

pub async fn teams_remove_member(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<NimBody>) -> ApiResult {
    assignment::remove_member(&state, &b.nim, &b.reason).await?;
    ok(json!({}))
}

pub async fn teams_confirm(State(state): State<AppState>, _a: InstructorAuth) -> ApiResult {
    let n = assignment::confirm(&state).await?;
    ok(json!({ "confirmed": n }))
}

pub async fn teams_discard(State(state): State<AppState>, _a: InstructorAuth) -> ApiResult {
    let n = assignment::discard_preview(&state).await?;
    ok(json!({ "discarded": n }))
}

#[derive(Deserialize)]
pub struct ReasonBody {
    #[serde(default)]
    reason: String,
}

pub async fn teams_place_late(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<ReasonBody>) -> ApiResult {
    let placed = assignment::place_late_arrivals(&state, &b.reason).await?;
    ok(json!({ "placed": placed }))
}

#[derive(Deserialize)]
pub struct CreateRoundBody {
    challenge_id: String,
    #[serde(default)]
    briefing: i64,
    #[serde(default)]
    defense_prep: i64,
    #[serde(default)]
    attack_window: i64,
    #[serde(default)]
    defense_response: i64,
    #[serde(default)]
    verification: i64,
    #[serde(default)]
    auto_advance: bool,
}

/// Durations arrive in minutes from the dashboard form.
pub async fn round_create(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<CreateRoundBody>) -> ApiResult {
    let d = Durations {
        briefing: b.briefing * 60,
        defense_prep: b.defense_prep * 60,
        attack_window: b.attack_window * 60,
        defense_response: b.defense_response * 60,
        verification: b.verification * 60,
    };
    let id = engine::create_round(&state, &b.challenge_id, d, b.auto_advance).await?;
    ok(json!({ "round_id": id }))
}

#[derive(Deserialize)]
pub struct RoundIdBody {
    round_id: i64,
}

#[derive(Deserialize)]
pub struct PairingBody {
    round_id: i64,
    pairs: Vec<PairItem>,
}

#[derive(Deserialize)]
pub struct PairItem {
    attacker: i64,
    target: i64,
}

pub async fn round_pairing(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<PairingBody>) -> ApiResult {
    engine::set_pairing(&state, b.round_id, b.pairs.iter().map(|p| (p.attacker, p.target)).collect()).await?;
    ok(json!({}))
}

pub async fn round_shuffle(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<RoundIdBody>) -> ApiResult {
    engine::shuffle_pairing(&state, b.round_id).await?;
    ok(json!({}))
}

pub async fn round_cancel(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<RoundIdBody>) -> ApiResult {
    engine::cancel_round(&state, b.round_id).await?;
    ok(json!({}))
}

pub async fn round_start(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<RoundIdBody>) -> ApiResult {
    engine::start_round(&state, b.round_id).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct AdvanceBody {
    round_id: i64,
    from: String,
}

pub async fn round_advance(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<AdvanceBody>) -> ApiResult {
    let from = Phase::from_code(&b.from).ok_or_else(|| AppError::bad("Fase tidak dikenal."))?;
    let next = engine::advance_phase(&state, b.round_id, from, "instructor").await?;
    ok(json!({ "phase": next.code() }))
}

pub async fn round_pause(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<RoundIdBody>) -> ApiResult {
    engine::pause(&state, b.round_id).await?;
    ok(json!({}))
}

pub async fn round_resume(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<RoundIdBody>) -> ApiResult {
    engine::resume(&state, b.round_id).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct ExtendBody {
    round_id: i64,
    seconds: i64,
}

pub async fn round_extend(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<ExtendBody>) -> ApiResult {
    engine::extend(&state, b.round_id, b.seconds).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct FinalizeBody {
    round_id: i64,
    #[serde(default)]
    force: bool,
}

pub async fn round_finalize(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<FinalizeBody>) -> ApiResult {
    let scores = engine::finalize_round(&state, b.round_id, b.force).await?;
    let scores: HashMap<String, scoring::RoundScore> = scores.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    ok(json!({ "scores": scores }))
}

#[derive(Deserialize)]
pub struct DiscussionBody {
    round_id: i64,
    open: bool,
}

pub async fn round_discussion(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<DiscussionBody>) -> ApiResult {
    engine::set_discussion(&state, b.round_id, b.open).await?;
    ok(json!({}))
}

pub async fn review_report(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<engine::ReportReview>) -> ApiResult {
    engine::review_report(&state, &b).await?;
    ok(json!({}))
}

pub async fn review_patch(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<engine::PatchReview>) -> ApiResult {
    engine::review_patch(&state, &b).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct AdjustBody {
    team_id: i64,
    #[serde(default)]
    round_id: Option<i64>,
    points: i64,
    reason: String,
}

pub async fn score_adjust(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<AdjustBody>) -> ApiResult {
    let round_id = b.round_id.filter(|r| *r > 0);
    engine::adjust_score(&state, b.team_id, round_id, b.points, &b.reason).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct MessageIdBody {
    message_id: i64,
}

pub async fn chat_delete(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<MessageIdBody>) -> ApiResult {
    chat::delete(&state, b.message_id).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct MuteBody {
    nim: String,
    minutes: i64,
}

pub async fn chat_mute(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<MuteBody>) -> ApiResult {
    chat::mute(&state, &b.nim, b.minutes).await?;
    ok(json!({}))
}

#[derive(Deserialize)]
pub struct LockBody {
    locked: bool,
}

pub async fn chat_lobby_lock(State(state): State<AppState>, _a: InstructorAuth, Json(b): Json<LockBody>) -> ApiResult {
    chat::set_lobby_locked(&state, b.locked).await?;
    ok(json!({}))
}

// ------------------------------------------------------------------------------------------------
// Export

#[derive(Deserialize)]
pub struct ExportQuery {
    session: Option<i64>,
}

async fn export_session_id(state: &AppState, q: &ExportQuery) -> AppResult<i64> {
    match q.session {
        Some(id) => Ok(id),
        None => Ok(db::current_session(&state.db).await?.id),
    }
}

pub async fn export_scores_csv(State(state): State<AppState>, _a: InstructorAuth, Query(q): Query<ExportQuery>) -> AppResult<Response> {
    let sid = export_session_id(&state, &q).await?;
    let names = team_names(&state.db, sid).await?;
    let scores = scoring::session_scores(&state.db, sid).await?;
    let members: Vec<(i64, String)> = sqlx::query_as("SELECT team_id, nim FROM team_members WHERE session_id = ? ORDER BY nim")
        .bind(sid)
        .fetch_all(&state.db)
        .await?;
    let mut csv = String::from("rank,team,members,round_attack,round_defense,round_rebuttal,round_total,adjustments,cumulative,valid_findings,accepted_patches\n");
    for s in scores {
        let m: Vec<&str> = members.iter().filter(|x| x.0 == s.team_id).map(|x| x.1.as_str()).collect();
        csv.push_str(&format!(
            "{},\"{}\",\"{}\",{},{},{},{},{},{},{},{}\n",
            s.rank,
            names.get(&s.team_id).cloned().unwrap_or_default().replace('"', "'"),
            m.join(" "),
            s.round.attack,
            s.round.defense,
            s.round.rebuttal,
            s.round.total,
            s.adjustments,
            s.cumulative,
            s.valid_findings_total,
            s.accepted_patches_total
        ));
    }
    Ok((
        [(CONTENT_TYPE, "text/csv; charset=utf-8".to_string()), (CONTENT_DISPOSITION, format!("attachment; filename=\"codearena-session-{sid}-scores.csv\""))],
        csv,
    )
        .into_response())
}

pub async fn export_session_json(State(state): State<AppState>, _a: InstructorAuth, Query(q): Query<ExportQuery>) -> AppResult<Response> {
    let sid = export_session_id(&state, &q).await?;
    let session: Option<(i64, String, String, i64, Option<i64>)> =
        sqlx::query_as("SELECT id, name, status, created_at, archived_at FROM game_sessions WHERE id = ?").bind(sid).fetch_optional(&state.db).await?;
    let session = session.ok_or_else(|| AppError::not_found("Sesi tidak ditemukan."))?;
    let teams: Vec<(i64, String, i64)> = sqlx::query_as("SELECT id, name, confirmed FROM teams WHERE session_id = ?").bind(sid).fetch_all(&state.db).await?;
    let members: Vec<(i64, String)> = sqlx::query_as("SELECT team_id, nim FROM team_members WHERE session_id = ?").bind(sid).fetch_all(&state.db).await?;
    let participants: Vec<(String, Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT nim, joined_at, last_seen FROM participants WHERE session_id = ?").bind(sid).fetch_all(&state.db).await?;
    let rounds: Vec<engine::RoundRow> = sqlx::query_as(
        "SELECT id, session_id, number, challenge_id, state, phase, phase_started_at, phase_deadline, paused_at, durations_json, auto_advance, discussion_open, created_at, started_at, finalized_at FROM rounds WHERE session_id = ? ORDER BY number",
    )
    .bind(sid)
    .fetch_all(&state.db)
    .await?;
    let mut rounds_json = vec![];
    for r in &rounds {
        let pairs: Vec<(i64, i64, i64)> =
            sqlx::query_as("SELECT attacker_team_id, target_team_id, locked FROM pairings WHERE round_id = ?").bind(r.id).fetch_all(&state.db).await?;
        let phases: Vec<(String, i64, Option<i64>, Option<i64>, i64)> =
            sqlx::query_as("SELECT phase, started_at, ended_at, planned_secs, paused_ms FROM round_phases WHERE round_id = ? ORDER BY id")
                .bind(r.id)
                .fetch_all(&state.db)
                .await?;
        let snaps: Vec<(i64, i64, String, String, i64)> =
            sqlx::query_as("SELECT team_id, version, code_sha256, source, locked_at FROM defense_snapshots WHERE round_id = ?")
                .bind(r.id)
                .fetch_all(&state.db)
                .await?;
        let reports = sqlx::query_as::<_, ReportRow>(&format!("SELECT {REPORT_COLS} FROM attack_reports WHERE round_id = ? ORDER BY id"))
            .bind(r.id)
            .fetch_all(&state.db)
            .await?;
        let patches = sqlx::query_as::<_, PatchRow>(&format!("SELECT {PATCH_COLS} FROM patches WHERE round_id = ? ORDER BY team_id, version"))
            .bind(r.id)
            .fetch_all(&state.db)
            .await?;
        let finals: Vec<(i64, i64, i64, i64, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT team_id, attack, defense, rebuttal, total, valid_findings, accepted_patches, finalized_at FROM round_scores WHERE round_id = ?",
        )
        .bind(r.id)
        .fetch_all(&state.db)
        .await?;
        rounds_json.push(json!({
            "round": r, "pairings": pairs, "phases": phases, "snapshots": snaps, "reports": reports, "patches": patches,
            "final_scores": finals.iter().map(|f| json!({ "team_id": f.0, "attack": f.1, "defense": f.2, "rebuttal": f.3, "total": f.4, "valid_findings": f.5, "accepted_patches": f.6, "finalized_at": f.7 })).collect::<Vec<_>>(),
        }));
    }
    let adjustments: Vec<(i64, Option<i64>, i64, String, i64)> =
        sqlx::query_as("SELECT team_id, round_id, points, reason, created_at FROM score_adjustments WHERE session_id = ?").bind(sid).fetch_all(&state.db).await?;
    let audit: Vec<(String, String, String, Option<String>, String, i64)> =
        sqlx::query_as("SELECT actor, action, entity, entity_id, detail, created_at FROM audit_log WHERE session_id = ? ORDER BY id")
            .bind(sid)
            .fetch_all(&state.db)
            .await?;
    let chat: Vec<(String, String, String, String, i64, i64)> =
        sqlx::query_as("SELECT room, sender_kind, sender_label, body, created_at, deleted FROM chat_messages WHERE session_id = ? ORDER BY id")
            .bind(sid)
            .fetch_all(&state.db)
            .await?;
    let body = json!({
        "exported_at": now_ms(),
        "session": { "id": session.0, "name": session.1, "status": session.2, "created_at": session.3, "archived_at": session.4 },
        "teams": teams.iter().map(|t| json!({ "id": t.0, "name": t.1, "confirmed": t.2 == 1,
            "members": members.iter().filter(|m| m.0 == t.0).map(|m| m.1.clone()).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "participants": participants.iter().map(|p| json!({ "nim": p.0, "joined_at": p.1, "last_seen": p.2 })).collect::<Vec<_>>(),
        "rounds": rounds_json,
        "scoreboard": scoring::session_scores(&state.db, sid).await?,
        "adjustments": adjustments.iter().map(|a| json!({ "team_id": a.0, "round_id": a.1, "points": a.2, "reason": a.3, "created_at": a.4 })).collect::<Vec<_>>(),
        "audit_log": audit.iter().map(|a| json!({ "actor": a.0, "action": a.1, "entity": a.2, "entity_id": a.3, "detail": serde_json::from_str::<Value>(&a.4).unwrap_or(Value::Null), "at": a.5 })).collect::<Vec<_>>(),
        "chat": chat.iter().map(|c| json!({ "room": c.0, "sender_kind": c.1, "sender": c.2, "body": c.3, "at": c.4, "deleted": c.5 == 1 })).collect::<Vec<_>>(),
    });
    Ok((
        [(CONTENT_TYPE, "application/json".to_string()), (CONTENT_DISPOSITION, format!("attachment; filename=\"codearena-session-{sid}.json\""))],
        serde_json::to_string_pretty(&body)?,
    )
        .into_response())
}
