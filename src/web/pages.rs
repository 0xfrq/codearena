//! Server-rendered pages and HTML fragments for students and the Live Arena.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde_json::json;
use sqlx::SqlitePool;

use super::vm::*;
use crate::auth::{self, StudentAuth};
use crate::challenges;
use crate::db;
use crate::engine::{self, PatchRow, ReportRow, RoundRow, PATCH_COLS, REPORT_COLS};
use crate::error::{AppError, AppResult};
use crate::events::Viewer;
use crate::models::*;
use crate::scoring;
use crate::state::AppState;
use crate::util::{fmt_time, json_for_script, now_ms};

pub fn render<T: Template>(t: &T) -> AppResult<String> {
    Ok(t.render()?)
}

pub fn page<T: Template>(t: &T) -> Response {
    match t.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => AppError::from(e).into_response(),
    }
}

pub async fn team_names(pool: &SqlitePool, session_id: i64) -> AppResult<HashMap<i64, String>> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM teams WHERE session_id = ?").bind(session_id).fetch_all(pool).await?;
    Ok(rows.into_iter().collect())
}

pub async fn members_of(state: &AppState, team_id: i64) -> AppResult<Vec<MemberView>> {
    let session = db::current_session(&state.db).await?;
    let rows: Vec<(String, Option<i64>)> = sqlx::query_as(
        "SELECT tm.nim, p.joined_at FROM team_members tm LEFT JOIN participants p ON p.session_id = tm.session_id AND p.nim = tm.nim
         WHERE tm.team_id = ? AND tm.session_id = ? ORDER BY tm.nim",
    )
    .bind(team_id)
    .bind(session.id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(nim, joined)| MemberView { online: state.presence.is_online(&nim), joined: joined.is_some(), nim })
        .collect())
}

async fn reports_where(pool: &SqlitePool, clause: &str, round_id: i64, team_id: Option<i64>) -> AppResult<Vec<ReportRow>> {
    let sql = format!("SELECT {REPORT_COLS} FROM attack_reports WHERE round_id = ? {clause} ORDER BY id");
    let mut q = sqlx::query_as::<_, ReportRow>(&sql).bind(round_id);
    if let Some(t) = team_id {
        q = q.bind(t);
    }
    Ok(q.fetch_all(pool).await?)
}

pub async fn report_views(state: &AppState, rows: &[ReportRow], session_id: i64, with_dup_hint: bool) -> AppResult<Vec<ReportView>> {
    let names = team_names(&state.db, session_id).await?;
    let snaps: Vec<(i64, i64, String)> = sqlx::query_as("SELECT id, version, code_sha256 FROM defense_snapshots").fetch_all(&state.db).await?;
    let snaps: HashMap<i64, String> = snaps.into_iter().map(|(id, v, sha)| (id, format!("v{v} · {}", &sha[..10]))).collect();
    let mut out = Vec::with_capacity(rows.len());
    let mut conn = state.db.acquire().await?;
    for r in rows {
        let dup = if with_dup_hint && r.status != "draft" { engine::duplicate_hint(&mut conn, r).await? } else { None };
        out.push(ReportView::from_row(r, &names, snaps.get(&r.snapshot_id).cloned().unwrap_or_default(), dup));
    }
    Ok(out)
}

pub async fn patch_views(state: &AppState, rows: &[PatchRow], session_id: i64) -> AppResult<Vec<PatchView>> {
    let names = team_names(&state.db, session_id).await?;
    Ok(rows.iter().map(|p| PatchView::from_row(p, &names)).collect())
}

async fn current_round(state: &AppState, session_id: i64) -> AppResult<Option<RoundRow>> {
    let mut conn = state.db.acquire().await?;
    engine::current_round(&mut conn, session_id).await
}

// ------------------------------------------------------------------------------------------------
// Landing / join / waiting room

#[derive(Template)]
#[template(path = "landing.html")]
pub struct LandingTpl {
    pub session_name: String,
    pub student: String,
    pub has_team: bool,
    pub instructor: bool,
    pub roster_size: i64,
}

pub async fn landing(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let session = db::current_session(&state.db).await?;
    let student = auth::student_from_headers(&state, &headers).await?;
    let instructor = auth::instructor_from_headers(&state, &headers).await?;
    let (roster_size,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM roster WHERE active = 1").fetch_one(&state.db).await?;
    Ok(page(&LandingTpl {
        session_name: session.name,
        student: student.as_ref().map(|s| s.nim.clone()).unwrap_or_default(),
        has_team: student.as_ref().is_some_and(|s| s.team_id.is_some()),
        instructor,
        roster_size,
    }))
}

#[derive(Template)]
#[template(path = "wait.html")]
pub struct WaitTpl {
    pub session_name: String,
    pub nim: String,
    pub has_team: bool,
}

pub async fn wait(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let Some(s) = auth::student_from_headers(&state, &headers).await? else {
        return Ok(Redirect::to("/").into_response());
    };
    let session = db::current_session(&state.db).await?;
    Ok(page(&WaitTpl { session_name: session.name, nim: s.nim, has_team: s.team_id.is_some() }))
}

// ------------------------------------------------------------------------------------------------
// Team dashboard

#[derive(Template)]
#[template(path = "team.html")]
pub struct TeamTpl {
    pub session_name: String,
    pub nim: String,
    pub team_id: i64,
    pub team_name: String,
    pub members: Vec<MemberView>,
    pub has_round: bool,
    pub round_number: i64,
    pub round_state: String,
    pub phase: String,
    pub target_name: String,
    pub attacker_name: String,
    pub prep_code: String,
    pub prep_notes: String,
    pub patch_code: String,
    pub patch_root_cause: String,
    pub patch_tests: String,
    pub patch_explanation: String,
    pub patch_label: String,
    pub frag_challenge: String,
    pub frag_prep: String,
    pub frag_snapshots: String,
    pub frag_reports_out: String,
    pub frag_reports_in: String,
    pub frag_patches: String,
    pub frag_addressed: String,
    pub boot_json: String,
}

pub async fn team_page(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let Some(s) = auth::student_from_headers(&state, &headers).await? else {
        return Ok(Redirect::to("/").into_response());
    };
    let Some(team_id) = s.team_id else {
        return Ok(Redirect::to("/wait").into_response());
    };
    let session = db::current_session(&state.db).await?;
    let names = team_names(&state.db, session.id).await?;
    let round = current_round(&state, session.id).await?;
    let mut tpl = TeamTpl {
        session_name: session.name.clone(),
        nim: s.nim.clone(),
        team_id,
        team_name: names.get(&team_id).cloned().unwrap_or_default(),
        members: members_of(&state, team_id).await?,
        has_round: false,
        round_number: 0,
        round_state: String::new(),
        phase: String::new(),
        target_name: String::new(),
        attacker_name: String::new(),
        prep_code: String::new(),
        prep_notes: String::new(),
        patch_code: String::new(),
        patch_root_cause: String::new(),
        patch_tests: String::new(),
        patch_explanation: String::new(),
        patch_label: String::new(),
        frag_challenge: String::new(),
        frag_prep: String::new(),
        frag_snapshots: String::new(),
        frag_reports_out: String::new(),
        frag_reports_in: String::new(),
        frag_patches: String::new(),
        frag_addressed: String::new(),
        boot_json: String::new(),
    };
    let mut round_id = 0;
    let mut discussion = false;
    if let Some(r) = round.as_ref().filter(|r| r.state != "setup") {
        round_id = r.id;
        discussion = r.discussion_open == 1;
        tpl.has_round = true;
        tpl.round_number = r.number;
        tpl.round_state = r.state.clone();
        tpl.phase = r.phase.clone();
        let mut conn = state.db.acquire().await?;
        let pairs = engine::pairings(&mut conn, r.id).await?;
        if let Some(p) = pairs.iter().find(|p| p.0 == team_id) {
            tpl.target_name = names.get(&p.1).cloned().unwrap_or_default();
        }
        if let Some(p) = pairs.iter().find(|p| p.1 == team_id) {
            tpl.attacker_name = names.get(&p.0).cloned().unwrap_or_default();
        }
        let prep: Option<(String, String)> = sqlx::query_as("SELECT code, notes FROM prep_drafts WHERE round_id = ? AND team_id = ?")
            .bind(r.id)
            .bind(team_id)
            .fetch_optional(&mut *conn)
            .await?;
        if let Some((code, notes)) = prep {
            tpl.prep_code = code;
            tpl.prep_notes = notes;
        }
        let latest = sqlx::query_as::<_, PatchRow>(&format!(
            "SELECT {PATCH_COLS} FROM patches WHERE round_id = ? AND team_id = ? ORDER BY version DESC LIMIT 1"
        ))
        .bind(r.id)
        .bind(team_id)
        .fetch_optional(&mut *conn)
        .await?;
        let challenge = challenges::load(&state.db, &r.challenge_id).await?;
        match latest {
            Some(p) => {
                tpl.patch_label = if p.status == "draft" {
                    format!("Draft v{} (tersimpan {})", p.version, fmt_time(p.updated_at))
                } else {
                    format!("Berbasis v{} ({}) — simpan untuk membuat v{}", p.version, p.status, p.version + 1)
                };
                tpl.patch_code = p.code;
                tpl.patch_root_cause = p.root_cause;
                tpl.patch_tests = p.regression_tests;
                tpl.patch_explanation = p.explanation;
            }
            None => {
                if let Some(snap) = engine::snapshot_of(&mut conn, r.id, team_id).await? {
                    tpl.patch_code = snap.code;
                    tpl.patch_label = "Berbasis snapshot v1 — simpan untuk membuat draft v2".into();
                }
                tpl.patch_tests = challenges::regression_template(&challenge);
            }
        }
        drop(conn);
        tpl.frag_prep = team_fragment(&state, &s, "prep").await?;
        tpl.frag_snapshots = team_fragment(&state, &s, "snapshots").await?;
        tpl.frag_reports_out = team_fragment(&state, &s, "reports_out").await?;
        tpl.frag_reports_in = team_fragment(&state, &s, "reports_in").await?;
        tpl.frag_patches = team_fragment(&state, &s, "patches").await?;
        tpl.frag_addressed = team_fragment(&state, &s, "addressed").await?;
    }
    tpl.frag_challenge = team_fragment(&state, &s, "challenge").await?;
    tpl.boot_json = json_for_script(&json!({
        "nim": s.nim, "team_id": team_id, "team_name": tpl.team_name, "round_id": round_id,
        "round_state": tpl.round_state, "discussion": discussion,
        "target": tpl.target_name, "attacker": tpl.attacker_name,
    }));
    Ok(page(&tpl))
}

#[derive(Template)]
#[template(path = "frag_team_reports_out.html")]
struct FragReportsOut {
    reports: Vec<ReportView>,
    can_edit: bool,
}

#[derive(Template)]
#[template(path = "frag_team_reports_in.html")]
struct FragReportsIn {
    reports: Vec<ReportView>,
    can_work: bool,
    can_rebut: bool,
}

#[derive(Template)]
#[template(path = "frag_team_patches.html")]
struct FragPatches {
    patches: Vec<PatchView>,
}

#[derive(Template)]
#[template(path = "frag_team_prep.html")]
struct FragPrep {
    phase: String,
    saves: i64,
    ready: bool,
    updated_at: String,
    updated_by: String,
    snapshot: Option<SnapshotView>,
}

#[derive(Template)]
#[template(path = "frag_team_snapshots.html")]
struct FragSnapshots {
    phase: String,
    target: Option<SnapshotView>,
    mine: Option<SnapshotView>,
}

#[derive(Template)]
#[template(path = "frag_challenge.html")]
pub struct FragChallenge {
    pub challenge: Option<ChallengeView>,
    pub round_number: i64,
    pub discussion: bool,
    pub all_reports: Vec<ReportView>,
}

#[derive(Template)]
#[template(path = "frag_team_addressed.html")]
struct FragAddressed {
    items: Vec<(i64, String, bool)>,
}

/// Render one team-dashboard fragment. All access checks are server-side: a team only receives
/// its own drafts/reports, the reports aimed at it, and its official target's locked snapshot.
pub async fn team_fragment(state: &AppState, s: &StudentAuth, name: &str) -> AppResult<String> {
    let team_id = s.team_id.ok_or_else(|| AppError::forbidden("Anda belum memiliki tim."))?;
    let session = db::current_session(&state.db).await?;
    let round = current_round(state, session.id).await?.filter(|r| r.state != "setup");
    let now = now_ms();
    let Some(r) = round else {
        return Ok(match name {
            "challenge" => render(&FragChallenge { challenge: None, round_number: 0, discussion: false, all_reports: vec![] })?,
            _ => "<p class=\"muted\">Belum ada ronde yang dimulai.</p>".into(),
        });
    };
    let phase = r.phase();
    let names = team_names(&state.db, session.id).await?;
    match name {
        "reports_out" => {
            let rows = reports_where(&state.db, "AND attacker_team_id = ?", r.id, Some(team_id)).await?;
            let reports = report_views(state, &rows, session.id, false).await?;
            render(&FragReportsOut { reports, can_edit: phase == Phase::AttackWindow && r.accepting(now) })
        }
        "reports_in" => {
            let rows = reports_where(&state.db, "AND target_team_id = ? AND status <> 'draft'", r.id, Some(team_id)).await?;
            let reports = report_views(state, &rows, session.id, false).await?;
            let working = matches!(phase, Phase::AttackWindow | Phase::DefenseResponse) && r.accepting(now);
            let rebut = matches!(phase, Phase::AttackWindow | Phase::DefenseResponse | Phase::Verification) && r.accepting(now);
            render(&FragReportsIn { reports, can_work: working, can_rebut: rebut })
        }
        "patches" => {
            let rows = sqlx::query_as::<_, PatchRow>(&format!(
                "SELECT {PATCH_COLS} FROM patches WHERE round_id = ? AND team_id = ? ORDER BY version DESC"
            ))
            .bind(r.id)
            .bind(team_id)
            .fetch_all(&state.db)
            .await?;
            render(&FragPatches { patches: patch_views(state, &rows, session.id).await? })
        }
        "addressed" => {
            let rows = reports_where(&state.db, "AND target_team_id = ? AND status <> 'draft'", r.id, Some(team_id)).await?;
            let draft: Option<(String,)> = sqlx::query_as(
                "SELECT addressed_reports FROM patches WHERE round_id = ? AND team_id = ? ORDER BY version DESC LIMIT 1",
            )
            .bind(r.id)
            .bind(team_id)
            .fetch_optional(&state.db)
            .await?;
            let checked: Vec<i64> = draft.and_then(|d| serde_json::from_str(&d.0).ok()).unwrap_or_default();
            let items = rows
                .iter()
                .map(|rep| (rep.id, format!("#{} {} [{}]", rep.id, rep.title, rep.status().label()), checked.contains(&rep.id)))
                .collect();
            render(&FragAddressed { items })
        }
        "prep" => {
            let prep: Option<(i64, i64, String, Option<i64>)> =
                sqlx::query_as("SELECT saves, updated_at, updated_by, ready_at FROM prep_drafts WHERE round_id = ? AND team_id = ?")
                    .bind(r.id)
                    .bind(team_id)
                    .fetch_optional(&state.db)
                    .await?;
            let mut conn = state.db.acquire().await?;
            let snap = engine::snapshot_of(&mut conn, r.id, team_id).await?;
            let team_name = names.get(&team_id).cloned().unwrap_or_default();
            render(&FragPrep {
                phase: r.phase.clone(),
                saves: prep.as_ref().map(|p| p.0).unwrap_or(0),
                ready: prep.as_ref().is_some_and(|p| p.3.is_some()),
                updated_at: prep.as_ref().map(|p| fmt_time(p.1)).unwrap_or_default(),
                updated_by: prep.as_ref().map(|p| p.2.clone()).unwrap_or_default(),
                snapshot: snap.as_ref().map(|x| SnapshotView::from_row(x, &team_name)),
            })
        }
        "snapshots" => {
            let mut conn = state.db.acquire().await?;
            let pairs = engine::pairings(&mut conn, r.id).await?;
            let target_id = pairs.iter().find(|p| p.0 == team_id).map(|p| p.1);
            let target = match target_id {
                Some(t) => engine::snapshot_of(&mut conn, r.id, t).await?.map(|x| SnapshotView::from_row(&x, names.get(&t).map(|s| s.as_str()).unwrap_or(""))),
                None => None,
            };
            let mine = engine::snapshot_of(&mut conn, r.id, team_id)
                .await?
                .map(|x| SnapshotView::from_row(&x, names.get(&team_id).map(|s| s.as_str()).unwrap_or("")));
            render(&FragSnapshots { phase: r.phase.clone(), target, mine })
        }
        "challenge" => {
            let c = challenges::load(&state.db, &r.challenge_id).await?;
            let discussion = r.discussion_open == 1;
            let all_reports = if discussion {
                let rows = reports_where(&state.db, "AND status <> 'draft'", r.id, None).await?;
                report_views(state, &rows, session.id, false).await?
            } else {
                vec![]
            };
            render(&FragChallenge { challenge: Some(ChallengeView::new(&c, discussion)), round_number: r.number, discussion, all_reports })
        }
        _ => Err(AppError::not_found("Fragment tidak dikenal.")),
    }
}

pub async fn team_fragment_handler(State(state): State<AppState>, s: StudentAuth, Path(name): Path<String>) -> AppResult<Html<String>> {
    Ok(Html(team_fragment(&state, &s, &name).await?))
}

// ------------------------------------------------------------------------------------------------
// Live Arena

#[derive(Template)]
#[template(path = "arena.html")]
pub struct ArenaTpl {
    pub session_name: String,
    pub instructor: bool,
    pub nim: String,
    pub boot_json: String,
}

pub async fn arena(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let Some(viewer) = auth::viewer_from_headers(&state, &headers).await? else {
        return Ok(Redirect::to("/").into_response());
    };
    let session = db::current_session(&state.db).await?;
    let (instructor, nim) = match &viewer {
        Viewer::Instructor => (true, String::new()),
        Viewer::Student { nim, .. } => (false, nim.clone()),
    };
    Ok(page(&ArenaTpl {
        session_name: session.name,
        instructor,
        nim,
        boot_json: json_for_script(&json!({ "instructor": instructor })),
    }))
}

#[derive(Template)]
#[template(path = "frag_inspector.html")]
pub struct FragInspector {
    pub instructor: bool,
    pub team_id: i64,
    pub team_name: String,
    pub members: Vec<MemberView>,
    pub has_round: bool,
    pub round_label: String,
    pub phase_label: String,
    pub challenge_label: String,
    pub target_name: String,
    pub attacker_name: String,
    pub attack_status: Option<StatusView>,
    pub defense_status: Option<StatusView>,
    pub incoming: Vec<ReportView>,
    pub outgoing: Vec<ReportView>,
    pub patches: Vec<PatchView>,
    pub latest_patch_label: String,
    pub regression_progress: String,
    pub prep_label: String,
    pub snapshot_label: String,
    pub score: scoring::TeamScore,
    pub score_history: Vec<(String, String, i64)>,
    pub activity: Vec<ActivityView>,
}

pub async fn arena_team_fragment(State(state): State<AppState>, headers: HeaderMap, Path(team_id): Path<i64>) -> AppResult<Html<String>> {
    let viewer = auth::viewer_from_headers(&state, &headers).await?.ok_or(AppError::Unauthorized)?;
    let instructor = viewer.is_instructor();
    let session = db::current_session(&state.db).await?;
    let names = team_names(&state.db, session.id).await?;
    let team_name = names.get(&team_id).cloned().ok_or_else(|| AppError::not_found("Kelompok tidak ditemukan."))?;
    let round = current_round(&state, session.id).await?.filter(|r| r.state != "setup" || instructor);
    let scores = scoring::session_scores(&state.db, session.id).await?;
    let score = scores.into_iter().find(|s| s.team_id == team_id).unwrap_or_default();
    let mut tpl = FragInspector {
        instructor,
        team_id,
        team_name,
        members: members_of(&state, team_id).await?,
        has_round: round.is_some(),
        round_label: String::new(),
        phase_label: String::new(),
        challenge_label: String::new(),
        target_name: "-".into(),
        attacker_name: "-".into(),
        attack_status: None,
        defense_status: None,
        incoming: vec![],
        outgoing: vec![],
        patches: vec![],
        latest_patch_label: "-".into(),
        regression_progress: "-".into(),
        prep_label: "-".into(),
        snapshot_label: "-".into(),
        score,
        score_history: vec![],
        activity: vec![],
    };
    let adjustments: Vec<(i64, String, i64)> =
        sqlx::query_as("SELECT points, reason, created_at FROM score_adjustments WHERE session_id = ? AND team_id = ? ORDER BY id")
            .bind(session.id)
            .bind(team_id)
            .fetch_all(&state.db)
            .await?;
    let finalized: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT r.number, rs.total, rs.finalized_at FROM round_scores rs JOIN rounds r ON r.id = rs.round_id
         WHERE r.session_id = ? AND rs.team_id = ? ORDER BY r.number",
    )
    .bind(session.id)
    .bind(team_id)
    .fetch_all(&state.db)
    .await?;
    let mut history: Vec<(i64, String, i64)> = finalized
        .into_iter()
        .map(|(n, total, at)| (at, format!("Ronde {n} difinalisasi (total resmi)"), total))
        .chain(adjustments.into_iter().map(|(p, reason, at)| (at, format!("Koreksi instruktur: {reason}"), p)))
        .collect();
    if let Some(r) = &round {
        let c = challenges::load(&state.db, &r.challenge_id).await?;
        tpl.round_label = format!("Ronde {} · {}", r.number, r.state);
        tpl.phase_label = if r.is_paused() { format!("{} (PAUSED)", r.phase().label()) } else { r.phase().label().to_string() };
        tpl.challenge_label = c.label();
        let mut conn = state.db.acquire().await?;
        let pairs = engine::pairings(&mut conn, r.id).await?;
        if let Some(p) = pairs.iter().find(|p| p.0 == team_id) {
            tpl.target_name = names.get(&p.1).cloned().unwrap_or_default();
        }
        if let Some(p) = pairs.iter().find(|p| p.1 == team_id) {
            tpl.attacker_name = names.get(&p.0).cloned().unwrap_or_default();
        }
        let st: Option<(String, String, i64, String, String, i64)> = sqlx::query_as(
            "SELECT attack_status, attack_reason, attack_at, defense_status, defense_reason, defense_at FROM team_status WHERE round_id = ? AND team_id = ?",
        )
        .bind(r.id)
        .bind(team_id)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(s) = st {
            tpl.attack_status = Some(StatusView::attack(&s.0, &s.1, s.2));
            tpl.defense_status = Some(StatusView::defense(&s.3, &s.4, s.5));
        }
        if let Some(snap) = engine::snapshot_of(&mut conn, r.id, team_id).await? {
            tpl.snapshot_label = format!("v{} · sha256 {} · dikunci {}", snap.version, &snap.code_sha256[..12], fmt_time(snap.locked_at));
        }
        drop(conn);
        let prep: Option<(i64, Option<i64>)> = sqlx::query_as("SELECT saves, ready_at FROM prep_drafts WHERE round_id = ? AND team_id = ?")
            .bind(r.id)
            .bind(team_id)
            .fetch_optional(&state.db)
            .await?;
        if let Some((saves, ready)) = prep {
            tpl.prep_label = format!("{saves} kali disimpan{}", if ready.is_some() { " · ditandai siap" } else { "" });
        }
        let incoming = reports_where(&state.db, "AND target_team_id = ? AND status <> 'draft'", r.id, Some(team_id)).await?;
        let outgoing_clause = if instructor { "AND attacker_team_id = ?" } else { "AND attacker_team_id = ? AND status <> 'draft'" };
        let outgoing = reports_where(&state.db, outgoing_clause, r.id, Some(team_id)).await?;
        tpl.incoming = report_views(&state, &incoming, session.id, instructor).await?;
        tpl.outgoing = report_views(&state, &outgoing, session.id, instructor).await?;
        for rep in incoming.iter().chain(outgoing.iter()) {
            if rep.status == "valid" && rep.attacker_team_id == team_id {
                history.push((rep.reviewed_at.unwrap_or(0), format!("Temuan valid #{} (rubric, max 2 temuan dihitung)", rep.id), rep.points()));
            }
            if rep.rebuttal_bonus > 0 && rep.target_team_id == team_id {
                history.push((rep.reviewed_at.unwrap_or(0), format!("Bonus rebuttal laporan #{}", rep.id), rep.rebuttal_bonus));
            }
        }
        let patch_rows = sqlx::query_as::<_, PatchRow>(&format!(
            "SELECT {PATCH_COLS} FROM patches WHERE round_id = ? AND team_id = ? ORDER BY version DESC"
        ))
        .bind(r.id)
        .bind(team_id)
        .fetch_all(&state.db)
        .await?;
        for p in &patch_rows {
            if p.reviewed_at.is_some() {
                history.push((p.reviewed_at.unwrap_or(0), format!("Patch v{} {} (defense = patch terbaik)", p.version, p.status), p.points()));
            }
        }
        if let Some(p) = patch_rows.first() {
            tpl.latest_patch_label = format!("v{} — {}", p.version, p.status().label());
            tpl.regression_progress = format!("{} regression test pada v{} (dihitung statis, tidak dieksekusi)", p.test_count(), p.version);
        }
        tpl.patches = patch_views(&state, &patch_rows, session.id).await?;
        let acts: Vec<(String, Option<String>, Option<String>, String, String, i64)> = sqlx::query_as(
            "SELECT kind, side, status, message, actor, created_at FROM activity WHERE round_id = ? AND team_id = ? ORDER BY id DESC LIMIT 80",
        )
        .bind(r.id)
        .bind(team_id)
        .fetch_all(&state.db)
        .await?;
        tpl.activity = acts.into_iter().map(|a| ActivityView::new(a.0, a.1, a.2, a.3, a.4, a.5)).collect();
    }
    history.sort_by_key(|h| h.0);
    tpl.score_history = history.into_iter().map(|(at, label, pts)| (fmt_time(at), label, pts)).collect();
    Ok(Html(render(&tpl)?))
}

#[derive(Template)]
#[template(path = "frag_edge.html")]
pub struct FragEdge {
    pub instructor: bool,
    pub attacker_name: String,
    pub target_name: String,
    pub round_number: i64,
    pub phase_label: String,
    pub locked: bool,
    pub snapshot_label: String,
    pub reports: Vec<ReportView>,
    pub attack_status: Option<StatusView>,
    pub defense_status: Option<StatusView>,
}

pub async fn arena_edge_fragment(State(state): State<AppState>, headers: HeaderMap, Path(attacker): Path<i64>) -> AppResult<Html<String>> {
    let viewer = auth::viewer_from_headers(&state, &headers).await?.ok_or(AppError::Unauthorized)?;
    let instructor = viewer.is_instructor();
    let session = db::current_session(&state.db).await?;
    let names = team_names(&state.db, session.id).await?;
    let round = current_round(&state, session.id)
        .await?
        .filter(|r| r.state != "setup" || instructor)
        .ok_or_else(|| AppError::not_found("Belum ada ronde."))?;
    let mut conn = state.db.acquire().await?;
    let pairs = engine::pairings(&mut conn, round.id).await?;
    let (a, t, locked) = *pairs.iter().find(|p| p.0 == attacker).ok_or_else(|| AppError::not_found("Relasi serangan tidak ditemukan."))?;
    let snapshot_label = match engine::snapshot_of(&mut conn, round.id, t).await? {
        Some(s) => format!("Snapshot target v{} · sha256 {} · dikunci {}", s.version, &s.code_sha256[..12], fmt_time(s.locked_at)),
        None => "Snapshot target belum dikunci (dikunci saat Attack Window dibuka)".into(),
    };
    let st_a: Option<(String, String, i64)> =
        sqlx::query_as("SELECT attack_status, attack_reason, attack_at FROM team_status WHERE round_id = ? AND team_id = ?")
            .bind(round.id)
            .bind(a)
            .fetch_optional(&mut *conn)
            .await?;
    let st_d: Option<(String, String, i64)> =
        sqlx::query_as("SELECT defense_status, defense_reason, defense_at FROM team_status WHERE round_id = ? AND team_id = ?")
            .bind(round.id)
            .bind(t)
            .fetch_optional(&mut *conn)
            .await?;
    drop(conn);
    let rows = sqlx::query_as::<_, ReportRow>(&format!(
        "SELECT {REPORT_COLS} FROM attack_reports WHERE round_id = ? AND attacker_team_id = ? AND target_team_id = ? AND status <> 'draft' ORDER BY id"
    ))
    .bind(round.id)
    .bind(a)
    .bind(t)
    .fetch_all(&state.db)
    .await?;
    Ok(Html(render(&FragEdge {
        instructor,
        attacker_name: names.get(&a).cloned().unwrap_or_default(),
        target_name: names.get(&t).cloned().unwrap_or_default(),
        round_number: round.number,
        phase_label: round.phase().label().into(),
        locked: locked == 1,
        snapshot_label,
        reports: report_views(&state, &rows, session.id, instructor).await?,
        attack_status: st_a.map(|s| StatusView::attack(&s.0, &s.1, s.2)),
        defense_status: st_d.map(|s| StatusView::defense(&s.0, &s.1, s.2)),
    })?))
}
