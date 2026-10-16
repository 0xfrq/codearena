//! Attack–defense round engine. Every mutation:
//!   1. takes the global write lock,
//!   2. validates phase / deadline / ownership on the server,
//!   3. writes inside one SQLite transaction (status rows, activity, audit),
//!   4. publishes events only after commit.

use std::collections::{HashMap, HashSet};

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqliteConnection;

use crate::challenges;
use crate::db::{self, current_session_conn};
use crate::error::{AppError, AppResult};
use crate::events::{Audience, Outbox};
use crate::models::*;
use crate::scoring;
use crate::state::AppState;
use crate::util::{clean_text, count_python_tests, now_ms, sha256_hex};

pub const MAX_OFFICIAL_REPORTS: i64 = 6;
pub const MAX_REPORTS_TOTAL: i64 = 10;
pub const MAX_PATCH_SUBMISSIONS: i64 = 5;
pub const MAX_CODE: usize = 100_000;

// ------------------------------------------------------------------------------------------------
// Rows

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct RoundRow {
    pub id: i64,
    pub session_id: i64,
    pub number: i64,
    pub challenge_id: String,
    pub state: String,
    pub phase: String,
    pub phase_started_at: Option<i64>,
    pub phase_deadline: Option<i64>,
    pub paused_at: Option<i64>,
    pub durations_json: String,
    pub auto_advance: i64,
    pub discussion_open: i64,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finalized_at: Option<i64>,
}

const ROUND_COLS: &str = "id, session_id, number, challenge_id, state, phase, phase_started_at, phase_deadline, paused_at, durations_json, auto_advance, discussion_open, created_at, started_at, finalized_at";

impl RoundRow {
    pub fn phase(&self) -> Phase {
        Phase::from_code(&self.phase).unwrap_or(Phase::Setup)
    }
    pub fn durations(&self) -> Durations {
        serde_json::from_str(&self.durations_json).unwrap_or_default()
    }
    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }
    pub fn is_running(&self) -> bool {
        self.state == "running"
    }
    /// Remaining time of the current phase; frozen while paused.
    pub fn remaining_ms(&self, now: i64) -> Option<i64> {
        let deadline = self.phase_deadline?;
        let reference = self.paused_at.unwrap_or(now);
        Some(deadline - reference)
    }
    /// Gameplay writes are accepted only while running, not paused and before the deadline.
    pub fn accepting(&self, now: i64) -> bool {
        self.is_running() && !self.is_paused() && self.phase_deadline.is_none_or(|d| now <= d)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Durations {
    #[serde(default)]
    pub briefing: i64,
    #[serde(default)]
    pub defense_prep: i64,
    #[serde(default)]
    pub attack_window: i64,
    #[serde(default)]
    pub defense_response: i64,
    #[serde(default)]
    pub verification: i64,
}

impl Default for Durations {
    fn default() -> Self {
        Durations { briefing: 180, defense_prep: 600, attack_window: 900, defense_response: 600, verification: 0 }
    }
}

impl Durations {
    pub fn secs(&self, p: Phase) -> i64 {
        match p {
            Phase::Briefing => self.briefing,
            Phase::DefensePrep => self.defense_prep,
            Phase::AttackWindow => self.attack_window,
            Phase::DefenseResponse => self.defense_response,
            Phase::Verification => self.verification,
            _ => 0,
        }
    }
    pub fn sanitized(self) -> Self {
        let c = |v: i64| v.clamp(0, 4 * 3600);
        Durations {
            briefing: c(self.briefing),
            defense_prep: c(self.defense_prep),
            attack_window: c(self.attack_window),
            defense_response: c(self.defense_response),
            verification: c(self.verification),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct ReportRow {
    pub id: i64,
    pub round_id: i64,
    pub attacker_team_id: i64,
    pub target_team_id: i64,
    pub snapshot_id: i64,
    pub status: String,
    pub title: String,
    pub code_location: String,
    pub trigger_input: String,
    pub expected: String,
    pub actual: String,
    pub repro_steps: String,
    pub root_cause: String,
    pub concept: String,
    pub evidence: String,
    pub revision: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub submitted_at: Option<i64>,
    pub submitted_by: Option<String>,
    pub reviewer: Option<String>,
    pub reviewed_at: Option<i64>,
    pub review_note: String,
    pub score_repro: i64,
    pub score_rootcause: i64,
    pub score_evidence: i64,
    pub duplicate_of: Option<i64>,
    pub rebuttal: String,
    pub rebuttal_at: Option<i64>,
    pub rebuttal_by: Option<String>,
    pub rebuttal_bonus: i64,
    pub defense_state: String,
    pub defense_analysis: String,
    pub defense_updated_at: Option<i64>,
}

pub const REPORT_COLS: &str = "id, round_id, attacker_team_id, target_team_id, snapshot_id, status, title, code_location, trigger_input, expected, actual, repro_steps, root_cause, concept, evidence, revision, created_by, created_at, updated_at, submitted_at, submitted_by, reviewer, reviewed_at, review_note, score_repro, score_rootcause, score_evidence, duplicate_of, rebuttal, rebuttal_at, rebuttal_by, rebuttal_bonus, defense_state, defense_analysis, defense_updated_at";

impl ReportRow {
    pub fn status(&self) -> ReportStatus {
        ReportStatus::from_code(&self.status).unwrap_or(ReportStatus::Draft)
    }
    pub fn points(&self) -> i64 {
        if self.status == "valid" {
            scoring::report_points(self.score_repro, self.score_rootcause, self.score_evidence)
        } else {
            0
        }
    }
    pub fn content_json(&self) -> Value {
        json!({
            "title": self.title, "code_location": self.code_location, "trigger_input": self.trigger_input,
            "expected": self.expected, "actual": self.actual, "repro_steps": self.repro_steps,
            "root_cause": self.root_cause, "concept": self.concept, "evidence": self.evidence,
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct PatchRow {
    pub id: i64,
    pub round_id: i64,
    pub team_id: i64,
    pub version: i64,
    pub status: String,
    pub code: String,
    pub root_cause: String,
    pub regression_tests: String,
    pub explanation: String,
    pub addressed_reports: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub updated_by: String,
    pub submitted_at: Option<i64>,
    pub submitted_by: Option<String>,
    pub reviewer: Option<String>,
    pub reviewed_at: Option<i64>,
    pub review_note: String,
    pub score_rootfix: i64,
    pub score_regression: i64,
    pub score_edge: i64,
    pub score_explain: i64,
}

pub const PATCH_COLS: &str = "id, round_id, team_id, version, status, code, root_cause, regression_tests, explanation, addressed_reports, created_at, updated_at, updated_by, submitted_at, submitted_by, reviewer, reviewed_at, review_note, score_rootfix, score_regression, score_edge, score_explain";

impl PatchRow {
    pub fn status(&self) -> PatchStatus {
        PatchStatus::from_code(&self.status).unwrap_or(PatchStatus::Draft)
    }
    pub fn addressed(&self) -> Vec<i64> {
        serde_json::from_str(&self.addressed_reports).unwrap_or_default()
    }
    pub fn test_count(&self) -> usize {
        count_python_tests(&self.regression_tests)
    }
    pub fn points(&self) -> i64 {
        if matches!(self.status(), PatchStatus::Accepted | PatchStatus::Rejected) {
            scoring::patch_points(self.score_rootfix, self.score_regression, self.score_edge, self.score_explain)
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct SnapshotRow {
    pub id: i64,
    pub round_id: i64,
    pub team_id: i64,
    pub version: i64,
    pub code: String,
    pub code_sha256: String,
    pub source: String,
    pub locked_at: i64,
}

// ------------------------------------------------------------------------------------------------
// Queries

pub async fn round_by_id(conn: &mut SqliteConnection, id: i64) -> AppResult<RoundRow> {
    sqlx::query_as::<_, RoundRow>(&format!("SELECT {ROUND_COLS} FROM rounds WHERE id = ?"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::not_found("Ronde tidak ditemukan."))
}

/// Latest non-cancelled round of a session (setup, running or finalized).
pub async fn current_round(conn: &mut SqliteConnection, session_id: i64) -> AppResult<Option<RoundRow>> {
    Ok(sqlx::query_as::<_, RoundRow>(&format!(
        "SELECT {ROUND_COLS} FROM rounds WHERE session_id = ? AND state <> 'cancelled' ORDER BY number DESC LIMIT 1"
    ))
    .bind(session_id)
    .fetch_optional(&mut *conn)
    .await?)
}

pub async fn report_by_id(conn: &mut SqliteConnection, id: i64) -> AppResult<ReportRow> {
    sqlx::query_as::<_, ReportRow>(&format!("SELECT {REPORT_COLS} FROM attack_reports WHERE id = ?"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::not_found("Laporan tidak ditemukan."))
}

pub async fn patch_by_id(conn: &mut SqliteConnection, id: i64) -> AppResult<PatchRow> {
    sqlx::query_as::<_, PatchRow>(&format!("SELECT {PATCH_COLS} FROM patches WHERE id = ?"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::not_found("Patch tidak ditemukan."))
}

pub async fn pairings(conn: &mut SqliteConnection, round_id: i64) -> AppResult<Vec<(i64, i64, i64)>> {
    Ok(sqlx::query_as("SELECT attacker_team_id, target_team_id, locked FROM pairings WHERE round_id = ? ORDER BY attacker_team_id")
        .bind(round_id)
        .fetch_all(&mut *conn)
        .await?)
}

pub async fn snapshot_of(conn: &mut SqliteConnection, round_id: i64, team_id: i64) -> AppResult<Option<SnapshotRow>> {
    Ok(sqlx::query_as::<_, SnapshotRow>(
        "SELECT id, round_id, team_id, version, code, code_sha256, source, locked_at FROM defense_snapshots WHERE round_id = ? AND team_id = ?",
    )
    .bind(round_id)
    .bind(team_id)
    .fetch_optional(&mut *conn)
    .await?)
}

async fn team_status_codes(conn: &mut SqliteConnection, round_id: i64, team_id: i64) -> AppResult<(String, String)> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT attack_status, defense_status FROM team_status WHERE round_id = ? AND team_id = ?")
            .bind(round_id)
            .bind(team_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(row.unwrap_or_else(|| ("NOT_STARTED".into(), "NOT_STARTED".into())))
}

// ------------------------------------------------------------------------------------------------
// Pairing

/// Ring pairing over `teams` in the given order: team i attacks team (i + offset) mod n.
/// Every team attacks exactly one other team and is attacked by exactly one team.
pub fn ring_pairing(teams: &[i64], offset: usize) -> Vec<(i64, i64)> {
    let n = teams.len();
    (0..n).map(|i| (teams[i], teams[(i + offset) % n])).collect()
}

/// Every team is attacker exactly once and target exactly once, never itself.
pub fn validate_pairing(teams: &[i64], pairs: &[(i64, i64)]) -> Result<(), String> {
    let set: HashSet<i64> = teams.iter().copied().collect();
    if pairs.len() != teams.len() {
        return Err(format!("Pairing harus memuat {} relasi (satu per tim).", teams.len()));
    }
    let mut attackers = HashSet::new();
    let mut targets = HashSet::new();
    for (a, t) in pairs {
        if !set.contains(a) || !set.contains(t) {
            return Err("Pairing memuat tim yang tidak ikut ronde.".into());
        }
        if a == t {
            return Err("Tim tidak boleh menyerang dirinya sendiri.".into());
        }
        if !attackers.insert(*a) {
            return Err("Setiap tim hanya boleh menyerang satu target.".into());
        }
        if !targets.insert(*t) {
            return Err("Setiap tim hanya boleh diserang oleh satu tim.".into());
        }
    }
    Ok(())
}

/// Random ring pairing that avoids mutual pairs (n > 2) and minimises repeats of earlier rounds.
pub fn generate_pairing(teams: &[i64], previous: &HashSet<(i64, i64)>, rng: &mut impl rand::Rng) -> Vec<(i64, i64)> {
    let n = teams.len();
    let offsets: Vec<usize> = (1..n).filter(|k| n <= 2 || (2 * k) % n != 0).collect();
    let mut best: Option<(usize, Vec<(i64, i64)>)> = None;
    for _ in 0..40 {
        let mut order = teams.to_vec();
        order.shuffle(rng);
        let offset = *offsets.choose(rng).unwrap_or(&1);
        let pairs = ring_pairing(&order, offset);
        let repeats = pairs.iter().filter(|p| previous.contains(p)).count();
        if best.as_ref().is_none_or(|(r, _)| repeats < *r) {
            best = Some((repeats, pairs));
        }
        if repeats == 0 {
            break;
        }
    }
    best.map(|b| b.1).unwrap_or_default()
}

// ------------------------------------------------------------------------------------------------
// Status helpers

#[allow(clippy::too_many_arguments)]
async fn set_attack(
    conn: &mut SqliteConnection,
    out: &mut Outbox,
    session_id: i64,
    round_id: i64,
    team_id: i64,
    status: AttackStatus,
    reason: &str,
    actor: &str,
) -> AppResult<()> {
    let now = now_ms();
    sqlx::query("UPDATE team_status SET attack_status = ?, attack_reason = ?, attack_at = ? WHERE round_id = ? AND team_id = ?")
        .bind(status.code())
        .bind(reason)
        .bind(now)
        .bind(round_id)
        .bind(team_id)
        .execute(&mut *conn)
        .await?;
    db::activity(conn, session_id, Some(round_id), Some(team_id), "status", Some("attack"), Some(status.code()), reason, actor)
        .await?;
    out.push(
        "team_status",
        Audience::All,
        json!({ "round_id": round_id, "team_id": team_id, "side": "attack", "status": StatusView::attack(status.code(), reason, now) }),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn set_defense(
    conn: &mut SqliteConnection,
    out: &mut Outbox,
    session_id: i64,
    round_id: i64,
    team_id: i64,
    status: DefenseStatus,
    reason: &str,
    actor: &str,
) -> AppResult<()> {
    let now = now_ms();
    sqlx::query("UPDATE team_status SET defense_status = ?, defense_reason = ?, defense_at = ? WHERE round_id = ? AND team_id = ?")
        .bind(status.code())
        .bind(reason)
        .bind(now)
        .bind(round_id)
        .bind(team_id)
        .execute(&mut *conn)
        .await?;
    db::activity(conn, session_id, Some(round_id), Some(team_id), "status", Some("defense"), Some(status.code()), reason, actor)
        .await?;
    out.push(
        "team_status",
        Audience::All,
        json!({ "round_id": round_id, "team_id": team_id, "side": "defense", "status": StatusView::defense(status.code(), reason, now) }),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn log_activity(
    conn: &mut SqliteConnection,
    out: &mut Outbox,
    session_id: i64,
    round_id: i64,
    team_id: Option<i64>,
    kind: &str,
    message: &str,
    actor: &str,
) -> AppResult<()> {
    let id = db::activity(conn, session_id, Some(round_id), team_id, kind, None, None, message, actor).await?;
    out.push(
        "activity",
        Audience::All,
        json!({ "id": id, "round_id": round_id, "team_id": team_id, "kind": kind, "message": message, "at": now_ms() }),
    );
    Ok(())
}

fn refresh(out: &mut Outbox, team_id: i64, parts: &[&str]) {
    out.push("team_refresh", Audience::Team(team_id), json!({ "parts": parts }));
}

async fn system_broadcast(conn: &mut SqliteConnection, out: &mut Outbox, session_id: i64, body: &str) -> AppResult<()> {
    let now = now_ms();
    let id = sqlx::query(
        "INSERT INTO chat_messages (session_id, room, sender_kind, sender_nim, sender_label, body, created_at)
         VALUES (?, 'broadcast', 'system', NULL, 'Sistem', ?, ?)",
    )
    .bind(session_id)
    .bind(body)
    .bind(now)
    .execute(&mut *conn)
    .await?
    .last_insert_rowid();
    out.push(
        "chat",
        Audience::All,
        json!({ "id": id, "room": "broadcast", "sender_kind": "system", "sender_label": "Sistem", "body": body, "at": now }),
    );
    Ok(())
}

fn require(cond: bool, msg: &str) -> AppResult<()> {
    if cond {
        Ok(())
    } else {
        Err(AppError::conflict(msg))
    }
}

fn require_phase(round: &RoundRow, allowed: &[Phase], now: i64) -> AppResult<()> {
    if !round.is_running() {
        return Err(AppError::conflict("Tidak ada ronde yang sedang berjalan."));
    }
    if !allowed.contains(&round.phase()) {
        let names: Vec<&str> = allowed.iter().map(|p| p.label()).collect();
        return Err(AppError::conflict(format!(
            "Aksi ini hanya tersedia pada fase: {} (fase saat ini: {}).",
            names.join(", "),
            round.phase().label()
        )));
    }
    if round.is_paused() {
        return Err(AppError::conflict("Ronde sedang di-pause oleh instruktur."));
    }
    if round.phase_deadline.is_some_and(|d| now > d) {
        return Err(AppError::conflict("Batas waktu fase ini sudah lewat."));
    }
    Ok(())
}

/// Team context for gameplay actions: the running round in which the team is paired.
pub struct TeamCtx {
    pub session_id: i64,
    pub round: RoundRow,
    pub target: i64,
    pub attacker: i64,
}

pub async fn team_ctx(conn: &mut SqliteConnection, team_id: i64) -> AppResult<TeamCtx> {
    let s = current_session_conn(conn).await?;
    let round = current_round(conn, s.id).await?.ok_or_else(|| AppError::conflict("Belum ada ronde."))?;
    if !round.is_running() {
        return Err(AppError::conflict("Tidak ada ronde yang sedang berjalan."));
    }
    let pairs = pairings(conn, round.id).await?;
    let target = pairs.iter().find(|p| p.0 == team_id).map(|p| p.1);
    let attacker = pairs.iter().find(|p| p.1 == team_id).map(|p| p.0);
    match (target, attacker) {
        (Some(target), Some(attacker)) => Ok(TeamCtx { session_id: s.id, round, target, attacker }),
        _ => Err(AppError::forbidden("Tim Anda tidak terdaftar dalam pairing ronde ini.")),
    }
}

// ------------------------------------------------------------------------------------------------
// Round lifecycle (instructor)

pub async fn create_round(state: &AppState, challenge_id: &str, durations: Durations, auto_advance: bool) -> AppResult<i64> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let s = current_session_conn(&mut tx).await?;
    let (open,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM rounds WHERE session_id = ? AND state IN ('setup', 'running')")
        .bind(s.id)
        .fetch_one(&mut *tx)
        .await?;
    require(open == 0, "Masih ada ronde yang belum selesai. Finalisasi atau batalkan terlebih dahulu.")?;
    let teams: Vec<(i64,)> = sqlx::query_as(
        "SELECT t.id FROM teams t WHERE t.session_id = ? AND t.confirmed = 1
           AND EXISTS (SELECT 1 FROM team_members tm WHERE tm.team_id = t.id) ORDER BY t.id",
    )
    .bind(s.id)
    .fetch_all(&mut *tx)
    .await?;
    let teams: Vec<i64> = teams.into_iter().map(|t| t.0).collect();
    require(teams.len() >= 2, "Minimal dua kelompok terkonfirmasi diperlukan untuk attack-defense.")?;
    let (exists,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM challenges WHERE id = ?").bind(challenge_id).fetch_one(&mut *tx).await?;
    require(exists == 1, "Challenge tidak ditemukan.")?;
    let (number,): (i64,) = sqlx::query_as("SELECT COALESCE(MAX(number), 0) + 1 FROM rounds WHERE session_id = ?")
        .bind(s.id)
        .fetch_one(&mut *tx)
        .await?;
    let now = now_ms();
    let durations = durations.sanitized();
    let round_id = sqlx::query(
        "INSERT INTO rounds (session_id, number, challenge_id, state, phase, durations_json, auto_advance, created_at)
         VALUES (?, ?, ?, 'setup', 'SETUP', ?, ?, ?)",
    )
    .bind(s.id)
    .bind(number)
    .bind(challenge_id)
    .bind(serde_json::to_string(&durations)?)
    .bind(auto_advance as i64)
    .bind(now)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    let previous: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT p.attacker_team_id, p.target_team_id FROM pairings p JOIN rounds r ON r.id = p.round_id
         WHERE r.session_id = ? AND r.state = 'finalized'",
    )
    .bind(s.id)
    .fetch_all(&mut *tx)
    .await?;
    let previous: HashSet<(i64, i64)> = previous.into_iter().collect();
    let pairs = generate_pairing(&teams, &previous, &mut rand::thread_rng());
    validate_pairing(&teams, &pairs).map_err(AppError::Internal)?;
    for (a, t) in &pairs {
        sqlx::query("INSERT INTO pairings (round_id, attacker_team_id, target_team_id) VALUES (?, ?, ?)")
            .bind(round_id)
            .bind(a)
            .bind(t)
            .execute(&mut *tx)
            .await?;
    }
    db::audit(
        &mut tx,
        Some(s.id),
        "instructor",
        "round_created",
        "round",
        round_id,
        json!({ "number": number, "challenge": challenge_id, "durations": durations, "pairs": pairs }),
    )
    .await?;
    tx.commit().await?;
    let mut out = Outbox::default();
    out.structure("round_created");
    out.flush(&state.events);
    Ok(round_id)
}

pub async fn set_pairing(state: &AppState, round_id: i64, pairs: Vec<(i64, i64)>) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let round = round_by_id(&mut tx, round_id).await?;
    require(round.state == "setup", "Pairing hanya dapat diubah sebelum ronde dimulai (pairing sudah dikunci).")?;
    let current = pairings(&mut tx, round_id).await?;
    let teams: Vec<i64> = current.iter().map(|p| p.0).collect();
    validate_pairing(&teams, &pairs).map_err(AppError::BadRequest)?;
    sqlx::query("DELETE FROM pairings WHERE round_id = ?").bind(round_id).execute(&mut *tx).await?;
    for (a, t) in &pairs {
        sqlx::query("INSERT INTO pairings (round_id, attacker_team_id, target_team_id) VALUES (?, ?, ?)")
            .bind(round_id)
            .bind(a)
            .bind(t)
            .execute(&mut *tx)
            .await?;
    }
    db::audit(&mut tx, Some(round.session_id), "instructor", "pairing_set", "round", round_id, json!({ "pairs": pairs })).await?;
    tx.commit().await?;
    let mut out = Outbox::default();
    out.structure("pairing_changed");
    out.flush(&state.events);
    Ok(())
}

pub async fn shuffle_pairing(state: &AppState, round_id: i64) -> AppResult<()> {
    let teams: Vec<i64> = {
        let mut conn = state.db.acquire().await?;
        pairings(&mut conn, round_id).await?.into_iter().map(|p| p.0).collect()
    };
    let pairs = generate_pairing(&teams, &HashSet::new(), &mut rand::thread_rng());
    set_pairing(state, round_id, pairs).await
}

pub async fn cancel_round(state: &AppState, round_id: i64) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let round = round_by_id(&mut tx, round_id).await?;
    require(round.state == "setup", "Hanya ronde yang belum dimulai yang dapat dibatalkan.")?;
    sqlx::query("DELETE FROM pairings WHERE round_id = ?").bind(round_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE rounds SET state = 'cancelled' WHERE id = ?").bind(round_id).execute(&mut *tx).await?;
    db::audit(&mut tx, Some(round.session_id), "instructor", "round_cancelled", "round", round_id, json!({})).await?;
    tx.commit().await?;
    let mut out = Outbox::default();
    out.structure("round_cancelled");
    out.flush(&state.events);
    Ok(())
}

pub async fn start_round(state: &AppState, round_id: i64) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let round = round_by_id(&mut tx, round_id).await?;
    require(round.state == "setup", "Ronde sudah dimulai atau tidak valid.")?;
    let challenge = challenges::load(&state.db, &round.challenge_id).await?;
    let now = now_ms();
    let d = round.durations();
    let deadline = (d.briefing > 0).then(|| now + d.briefing * 1000);
    let n = sqlx::query(
        "UPDATE rounds SET state = 'running', phase = 'BRIEFING', phase_started_at = ?, phase_deadline = ?, started_at = ?
         WHERE id = ? AND state = 'setup'",
    )
    .bind(now)
    .bind(deadline)
    .bind(now)
    .bind(round_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    require(n == 1, "Ronde sudah dimulai.")?;
    sqlx::query("UPDATE pairings SET locked = 1 WHERE round_id = ?").bind(round_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE game_sessions SET teams_locked = 1, status = 'running' WHERE id = ?")
        .bind(round.session_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO round_phases (round_id, phase, started_at, planned_secs) VALUES (?, 'BRIEFING', ?, ?)")
        .bind(round_id)
        .bind(now)
        .bind(d.briefing)
        .execute(&mut *tx)
        .await?;
    let mut out = Outbox::default();
    for (attacker, target, _) in pairings(&mut tx, round_id).await? {
        let target_name = db::team_name(&mut tx, target).await?;
        let reason_a = format!("Pairing dikunci — target: {target_name}");
        let reason_d = format!("Ronde {} dimulai (briefing)", round.number);
        sqlx::query(
            "INSERT INTO team_status (round_id, team_id, attack_status, attack_reason, attack_at, defense_status, defense_reason, defense_at)
             VALUES (?, ?, 'TARGET_ASSIGNED', ?, ?, 'NOT_STARTED', ?, ?)",
        )
        .bind(round_id)
        .bind(attacker)
        .bind(&reason_a)
        .bind(now)
        .bind(&reason_d)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        db::activity(&mut tx, round.session_id, Some(round_id), Some(attacker), "status", Some("attack"), Some("TARGET_ASSIGNED"), &reason_a, "system")
            .await?;
    }
    system_broadcast(
        &mut tx,
        &mut out,
        round.session_id,
        &format!("Ronde {} dimulai — challenge {}. Fase: Briefing.", round.number, challenge.label()),
    )
    .await?;
    db::audit(&mut tx, Some(round.session_id), "instructor", "round_started", "round", round_id, json!({ "matchup_locked": true }))
        .await?;
    tx.commit().await?;
    out.structure("round_started");
    out.flush(&state.events);
    Ok(())
}

/// Move a running round to the next phase. `expected` makes the transition idempotent
/// (double clicks and the timer racing with the instructor cannot skip a phase).
pub async fn advance_phase(state: &AppState, round_id: i64, expected: Phase, actor: &str) -> AppResult<Phase> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let round = round_by_id(&mut tx, round_id).await?;
    require(round.is_running(), "Ronde tidak sedang berjalan.")?;
    require(round.phase() == expected, "Fase sudah berubah — muat ulang tampilan.")?;
    let next = expected.next().ok_or_else(|| AppError::conflict("Ronde sudah berada di fase terakhir; gunakan Finalisasi."))?;
    let now = now_ms();
    let paused_extra = round.paused_at.map(|p| now - p).unwrap_or(0);
    sqlx::query("UPDATE round_phases SET ended_at = ?, paused_ms = paused_ms + ? WHERE round_id = ? AND phase = ?")
        .bind(now)
        .bind(paused_extra)
        .bind(round_id)
        .bind(expected.code())
        .execute(&mut *tx)
        .await?;
    let secs = round.durations().secs(next);
    let deadline = (secs > 0).then(|| now + secs * 1000);
    let n = sqlx::query(
        "UPDATE rounds SET phase = ?, phase_started_at = ?, phase_deadline = ?, paused_at = NULL WHERE id = ? AND phase = ?",
    )
    .bind(next.code())
    .bind(now)
    .bind(deadline)
    .bind(round_id)
    .bind(expected.code())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    require(n == 1, "Fase sudah berubah.")?;
    sqlx::query("INSERT INTO round_phases (round_id, phase, started_at, planned_secs) VALUES (?, ?, ?, ?)")
        .bind(round_id)
        .bind(next.code())
        .bind(now)
        .bind(secs)
        .execute(&mut *tx)
        .await?;

    let mut out = Outbox::default();
    let sid = round.session_id;
    let pairs = pairings(&mut tx, round_id).await?;
    match next {
        Phase::DefensePrep => {
            let challenge = challenges::load(&state.db, &round.challenge_id).await?;
            for (team, _, _) in &pairs {
                sqlx::query(
                    "INSERT INTO prep_drafts (round_id, team_id, code, updated_at, updated_by) VALUES (?, ?, ?, ?, 'system')
                     ON CONFLICT(round_id, team_id) DO NOTHING",
                )
                .bind(round_id)
                .bind(team)
                .bind(&challenge.starter_code)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                set_defense(&mut tx, &mut out, sid, round_id, *team, DefenseStatus::DefensePreparation, "Fase Defense Preparation dibuka", actor)
                    .await?;
            }
        }
        Phase::AttackWindow => {
            let challenge = challenges::load(&state.db, &round.challenge_id).await?;
            for (team, _, _) in &pairs {
                let draft: Option<(String, i64)> =
                    sqlx::query_as("SELECT code, saves FROM prep_drafts WHERE round_id = ? AND team_id = ?")
                        .bind(round_id)
                        .bind(team)
                        .fetch_optional(&mut *tx)
                        .await?;
                let (code, source) = match draft {
                    Some((code, saves)) if saves > 0 => (code, "team_draft"),
                    _ => (challenge.starter_code.clone(), "starter_default"),
                };
                let hash = sha256_hex(&code);
                sqlx::query(
                    "INSERT INTO defense_snapshots (round_id, team_id, version, code, code_sha256, source, locked_at) VALUES (?, ?, 1, ?, ?, ?, ?)",
                )
                .bind(round_id)
                .bind(team)
                .bind(&code)
                .bind(&hash)
                .bind(source)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                let reason = format!("Snapshot v1 dikunci (sha256 {})", &hash[..10]);
                set_defense(&mut tx, &mut out, sid, round_id, *team, DefenseStatus::AwaitingAttack, &reason, actor).await?;
                refresh(&mut out, *team, &["snapshots", "prep"]);
            }
        }
        Phase::DefenseResponse => {
            sqlx::query("UPDATE attack_reports SET status = 'pending_review' WHERE round_id = ? AND status = 'submitted'")
                .bind(round_id)
                .execute(&mut *tx)
                .await?;
            for (team, _, _) in &pairs {
                let (official, pending, drafts): (i64, i64, i64) = sqlx::query_as(
                    "SELECT COALESCE(SUM(status <> 'draft'), 0), COALESCE(SUM(status IN ('pending_review', 'submitted')), 0), COALESCE(SUM(status = 'draft'), 0)
                     FROM attack_reports WHERE round_id = ? AND attacker_team_id = ?",
                )
                .bind(round_id)
                .bind(team)
                .fetch_one(&mut *tx)
                .await?;
                if pending > 0 {
                    let reason = format!("Attack window ditutup — {pending} laporan menunggu validasi");
                    set_attack(&mut tx, &mut out, sid, round_id, *team, AttackStatus::AwaitingValidation, &reason, actor).await?;
                } else if official == 0 {
                    set_attack(&mut tx, &mut out, sid, round_id, *team, AttackStatus::AttackCompleted, "Attack window ditutup tanpa laporan resmi", actor)
                        .await?;
                }
                if drafts > 0 {
                    let msg = format!("{drafts} draft laporan tidak disubmit sebelum attack window ditutup");
                    log_activity(&mut tx, &mut out, sid, round_id, Some(*team), "report_expired", &msg, actor).await?;
                }
                refresh(&mut out, *team, &["reports_out", "reports_in"]);
            }
        }
        Phase::Verification => {
            for (team, _, _) in &pairs {
                let pending: Option<(i64,)> = sqlx::query_as(
                    "SELECT version FROM patches WHERE round_id = ? AND team_id = ? AND status = 'submitted' ORDER BY version DESC LIMIT 1",
                )
                .bind(round_id)
                .bind(team)
                .fetch_optional(&mut *tx)
                .await?;
                if let Some((v,)) = pending {
                    let reason = format!("Defense window ditutup — patch v{v} menunggu review");
                    set_defense(&mut tx, &mut out, sid, round_id, *team, DefenseStatus::AwaitingReview, &reason, actor).await?;
                }
                refresh(&mut out, *team, &["patches"]);
            }
        }
        _ => {}
    }
    system_broadcast(&mut tx, &mut out, sid, &format!("Ronde {}: fase {} dimulai. {}", round.number, next.label(), next.description()))
        .await?;
    db::audit(&mut tx, Some(sid), actor, "phase_changed", "round", round_id, json!({ "from": expected.code(), "to": next.code() }))
        .await?;
    tx.commit().await?;
    drop(_w);
    out.flush(&state.events);
    crate::views::publish_round(state).await;
    crate::views::publish_live(state).await;
    Ok(next)
}

pub async fn pause(state: &AppState, round_id: i64) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let round = round_by_id(&mut tx, round_id).await?;
    require(round.is_running() && !round.is_paused(), "Ronde tidak dapat di-pause.")?;
    let now = now_ms();
    sqlx::query("UPDATE rounds SET paused_at = ? WHERE id = ? AND paused_at IS NULL").bind(now).bind(round_id).execute(&mut *tx).await?;
    let mut out = Outbox::default();
    system_broadcast(&mut tx, &mut out, round.session_id, "Ronde di-pause oleh instruktur. Timer dibekukan.").await?;
    db::audit(&mut tx, Some(round.session_id), "instructor", "paused", "round", round_id, json!({ "phase": round.phase })).await?;
    tx.commit().await?;
    drop(_w);
    out.flush(&state.events);
    crate::views::publish_round(state).await;
    Ok(())
}

pub async fn resume(state: &AppState, round_id: i64) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let round = round_by_id(&mut tx, round_id).await?;
    let paused_at = round.paused_at.ok_or_else(|| AppError::conflict("Ronde tidak sedang di-pause."))?;
    let now = now_ms();
    let paused_for = now - paused_at;
    sqlx::query("UPDATE rounds SET paused_at = NULL, phase_deadline = CASE WHEN phase_deadline IS NULL THEN NULL ELSE phase_deadline + ? END WHERE id = ?")
        .bind(paused_for)
        .bind(round_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE round_phases SET paused_ms = paused_ms + ? WHERE round_id = ? AND phase = ?")
        .bind(paused_for)
        .bind(round_id)
        .bind(&round.phase)
        .execute(&mut *tx)
        .await?;
    let mut out = Outbox::default();
    system_broadcast(&mut tx, &mut out, round.session_id, "Ronde dilanjutkan.").await?;
    db::audit(&mut tx, Some(round.session_id), "instructor", "resumed", "round", round_id, json!({ "paused_ms": paused_for })).await?;
    tx.commit().await?;
    drop(_w);
    out.flush(&state.events);
    crate::views::publish_round(state).await;
    Ok(())
}

pub async fn extend(state: &AppState, round_id: i64, seconds: i64) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let round = round_by_id(&mut tx, round_id).await?;
    require(round.is_running(), "Ronde tidak berjalan.")?;
    let now = now_ms();
    let base = round.phase_deadline.unwrap_or(round.paused_at.unwrap_or(now));
    let new_deadline = base + seconds * 1000;
    let reference = round.paused_at.unwrap_or(now);
    require(new_deadline >= reference + 10_000, "Sisa waktu minimal 10 detik.")?;
    sqlx::query("UPDATE rounds SET phase_deadline = ? WHERE id = ?").bind(new_deadline).bind(round_id).execute(&mut *tx).await?;
    let mut out = Outbox::default();
    let verb = if seconds >= 0 { "ditambah" } else { "dikurangi" };
    system_broadcast(&mut tx, &mut out, round.session_id, &format!("Waktu fase {} {verb} {} detik.", round.phase().label(), seconds.abs()))
        .await?;
    db::audit(&mut tx, Some(round.session_id), "instructor", "deadline_changed", "round", round_id, json!({ "seconds": seconds })).await?;
    tx.commit().await?;
    drop(_w);
    out.flush(&state.events);
    crate::views::publish_round(state).await;
    Ok(())
}

pub async fn finalize_round(state: &AppState, round_id: i64, force: bool) -> AppResult<HashMap<i64, scoring::RoundScore>> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let round = round_by_id(&mut tx, round_id).await?;
    require(round.is_running() && round.phase() == Phase::Results, "Finalisasi hanya dapat dilakukan pada fase Round Results.")?;
    let (open_reports,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM attack_reports WHERE round_id = ? AND status IN ('submitted', 'pending_review', 'needs_revision')",
    )
    .bind(round_id)
    .fetch_one(&mut *tx)
    .await?;
    let (open_patches,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM patches WHERE round_id = ? AND status = 'submitted'").bind(round_id).fetch_one(&mut *tx).await?;
    if (open_reports > 0 || open_patches > 0) && !force {
        return Err(AppError::conflict(format!(
            "Masih ada {open_reports} laporan dan {open_patches} patch yang belum direview. Review terlebih dahulu atau finalisasi paksa (nilai 0 untuk yang belum direview)."
        )));
    }
    let scores = scoring::compute_round(&state.db, round_id).await?;
    let now = now_ms();
    let mut out = Outbox::default();
    for (team, s) in &scores {
        sqlx::query(
            "INSERT INTO round_scores (round_id, team_id, attack, defense, rebuttal, total, valid_findings, accepted_patches, finalized_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(round_id)
        .bind(team)
        .bind(s.attack)
        .bind(s.defense)
        .bind(s.rebuttal)
        .bind(s.total)
        .bind(s.valid_findings)
        .bind(s.accepted_patches)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let ra = format!("Ronde difinalisasi — skor attack {}", s.attack);
        let rd = format!("Ronde difinalisasi — skor defense {} (+rebuttal {})", s.defense, s.rebuttal);
        set_attack(&mut tx, &mut out, round.session_id, round_id, *team, AttackStatus::AttackCompleted, &ra, "instructor").await?;
        set_defense(&mut tx, &mut out, round.session_id, round_id, *team, DefenseStatus::DefenseCompleted, &rd, "instructor").await?;
    }
    sqlx::query("UPDATE rounds SET state = 'finalized', finalized_at = ?, phase_deadline = NULL, paused_at = NULL WHERE id = ? AND state = 'running'")
        .bind(now)
        .bind(round_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE round_phases SET ended_at = ? WHERE round_id = ? AND phase = 'RESULTS' AND ended_at IS NULL")
        .bind(now)
        .bind(round_id)
        .execute(&mut *tx)
        .await?;
    system_broadcast(&mut tx, &mut out, round.session_id, &format!("Hasil ronde {} telah disahkan. Lihat leaderboard.", round.number)).await?;
    let detail: HashMap<String, scoring::RoundScore> = scores.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    db::audit(&mut tx, Some(round.session_id), "instructor", "round_finalized", "round", round_id, json!({ "force": force, "scores": detail }))
        .await?;
    tx.commit().await?;
    drop(_w);
    out.structure("round_finalized");
    out.flush(&state.events);
    Ok(scores)
}

pub async fn set_discussion(state: &AppState, round_id: i64, open: bool) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let round = round_by_id(&mut tx, round_id).await?;
    require(
        round.state == "finalized" || (round.is_running() && round.phase() == Phase::Results),
        "Pembahasan hanya dapat dibuka setelah ronde mencapai Round Results.",
    )?;
    sqlx::query("UPDATE rounds SET discussion_open = ? WHERE id = ?").bind(open as i64).bind(round_id).execute(&mut *tx).await?;
    let mut out = Outbox::default();
    if open {
        system_broadcast(&mut tx, &mut out, round.session_id, &format!("Pembahasan ronde {} dibuka: answer key dan seluruh laporan dapat dipelajari.", round.number))
            .await?;
    }
    db::audit(&mut tx, Some(round.session_id), "instructor", "discussion", "round", round_id, json!({ "open": open })).await?;
    tx.commit().await?;
    drop(_w);
    out.structure("discussion");
    out.flush(&state.events);
    Ok(())
}

/// Called every second: auto-advances timed work phases whose server deadline has passed.
pub async fn tick(state: &AppState) {
    let now = now_ms();
    let due: Result<Vec<(i64, String)>, _> = sqlx::query_as(
        "SELECT id, phase FROM rounds WHERE state = 'running' AND paused_at IS NULL AND auto_advance = 1
           AND phase_deadline IS NOT NULL AND phase_deadline <= ? AND phase IN ('DEFENSE_PREP', 'ATTACK_WINDOW', 'DEFENSE_RESPONSE')",
    )
    .bind(now)
    .fetch_all(&state.db)
    .await;
    match due {
        Ok(rows) => {
            for (id, phase) in rows {
                if let Some(p) = Phase::from_code(&phase) {
                    match advance_phase(state, id, p, "system:timer").await {
                        Ok(next) => tracing::info!("round {id}: timer advanced {phase} -> {}", next.code()),
                        Err(e) => tracing::debug!("round {id}: timer advance skipped: {e}"),
                    }
                }
            }
        }
        Err(e) => tracing::error!("tick query failed: {e}"),
    }
}

// ------------------------------------------------------------------------------------------------
// Team actions (students)

pub async fn set_explicit_status(state: &AppState, team_id: i64, nim: &str, side: &str, code: &str) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let ctx = team_ctx(&mut tx, team_id).await?;
    let now = now_ms();
    let mut out = Outbox::default();
    let actor = format!("student:{nim}");
    match side {
        "attack" => {
            let status = AttackStatus::from_code(code).ok_or_else(|| AppError::bad("Status tidak dikenal."))?;
            require(
                matches!(status, AttackStatus::InspectingTarget | AttackStatus::PreparingReport),
                "Status attack ini tidak dapat dipilih manual.",
            )?;
            require_phase(&ctx.round, &[Phase::AttackWindow], now)?;
            let reason = format!("{} (dipilih oleh {nim})", status.label());
            set_attack(&mut tx, &mut out, ctx.session_id, ctx.round.id, team_id, status, &reason, &actor).await?;
        }
        "defense" => {
            let status = DefenseStatus::from_code(code).ok_or_else(|| AppError::bad("Status tidak dikenal."))?;
            require(
                matches!(status, DefenseStatus::AnalyzingBug | DefenseStatus::FixingBug | DefenseStatus::WritingRegressionTest),
                "Status defense ini tidak dapat dipilih manual.",
            )?;
            require_phase(&ctx.round, &[Phase::AttackWindow, Phase::DefenseResponse], now)?;
            let reason = format!("{} (dipilih oleh {nim})", status.label());
            set_defense(&mut tx, &mut out, ctx.session_id, ctx.round.id, team_id, status, &reason, &actor).await?;
        }
        _ => return Err(AppError::bad("side harus attack atau defense.")),
    }
    tx.commit().await?;
    out.flush(&state.events);
    Ok(())
}

pub async fn save_prep(state: &AppState, team_id: i64, nim: &str, code: &str, notes: &str) -> AppResult<i64> {
    if code.len() > MAX_CODE {
        return Err(AppError::bad("Kode terlalu panjang (maks 100 KB)."));
    }
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let ctx = team_ctx(&mut tx, team_id).await?;
    require_phase(&ctx.round, &[Phase::DefensePrep], now_ms())?;
    sqlx::query(
        "UPDATE prep_drafts SET code = ?, notes = ?, saves = saves + 1, updated_at = ?, updated_by = ? WHERE round_id = ? AND team_id = ?",
    )
    .bind(code)
    .bind(clean_text(notes, 4000))
    .bind(now_ms())
    .bind(nim)
    .bind(ctx.round.id)
    .bind(team_id)
    .execute(&mut *tx)
    .await?;
    let (saves,): (i64,) = sqlx::query_as("SELECT saves FROM prep_drafts WHERE round_id = ? AND team_id = ?")
        .bind(ctx.round.id)
        .bind(team_id)
        .fetch_one(&mut *tx)
        .await?;
    let mut out = Outbox::default();
    let msg = format!("Draft implementasi pertahanan disimpan (simpanan #{saves}) oleh {nim}");
    log_activity(&mut tx, &mut out, ctx.session_id, ctx.round.id, Some(team_id), "prep_saved", &msg, &format!("student:{nim}")).await?;
    tx.commit().await?;
    refresh(&mut out, team_id, &["prep"]);
    out.flush(&state.events);
    Ok(saves)
}

pub async fn mark_prep_ready(state: &AppState, team_id: i64, nim: &str) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let ctx = team_ctx(&mut tx, team_id).await?;
    require_phase(&ctx.round, &[Phase::DefensePrep], now_ms())?;
    sqlx::query("UPDATE prep_drafts SET ready_at = ? WHERE round_id = ? AND team_id = ?")
        .bind(now_ms())
        .bind(ctx.round.id)
        .bind(team_id)
        .execute(&mut *tx)
        .await?;
    let mut out = Outbox::default();
    let msg = format!("Tim menandai implementasi pertahanan siap dikunci ({nim})");
    log_activity(&mut tx, &mut out, ctx.session_id, ctx.round.id, Some(team_id), "prep_ready", &msg, &format!("student:{nim}")).await?;
    tx.commit().await?;
    refresh(&mut out, team_id, &["prep"]);
    out.flush(&state.events);
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReportFields {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub code_location: String,
    #[serde(default)]
    pub trigger_input: String,
    #[serde(default)]
    pub expected: String,
    #[serde(default)]
    pub actual: String,
    #[serde(default)]
    pub repro_steps: String,
    #[serde(default)]
    pub root_cause: String,
    #[serde(default)]
    pub concept: String,
    #[serde(default)]
    pub evidence: String,
}

impl ReportFields {
    fn cleaned(&self) -> ReportFields {
        ReportFields {
            title: clean_text(&self.title, 200),
            code_location: clean_text(&self.code_location, 300),
            trigger_input: clean_text(&self.trigger_input, 6000),
            expected: clean_text(&self.expected, 3000),
            actual: clean_text(&self.actual, 3000),
            repro_steps: clean_text(&self.repro_steps, 6000),
            root_cause: clean_text(&self.root_cause, 6000),
            concept: clean_text(&self.concept, 200),
            evidence: clean_text(&self.evidence, 10000),
        }
    }
    fn missing(&self) -> Vec<&'static str> {
        let mut m = vec![];
        for (v, name) in [
            (&self.title, "judul temuan"),
            (&self.code_location, "lokasi kode"),
            (&self.trigger_input, "input/pemicu"),
            (&self.expected, "expected behavior"),
            (&self.actual, "actual behavior"),
            (&self.repro_steps, "instruksi reproduksi"),
            (&self.root_cause, "root cause"),
            (&self.concept, "konsep PBO"),
        ] {
            if v.trim().is_empty() {
                m.push(name);
            }
        }
        m
    }
}

async fn write_report_fields(conn: &mut SqliteConnection, id: i64, f: &ReportFields) -> AppResult<()> {
    sqlx::query(
        "UPDATE attack_reports SET title = ?, code_location = ?, trigger_input = ?, expected = ?, actual = ?, repro_steps = ?,
         root_cause = ?, concept = ?, evidence = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&f.title)
    .bind(&f.code_location)
    .bind(&f.trigger_input)
    .bind(&f.expected)
    .bind(&f.actual)
    .bind(&f.repro_steps)
    .bind(&f.root_cause)
    .bind(&f.concept)
    .bind(&f.evidence)
    .bind(now_ms())
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Create or update a draft attack report against the team's official target snapshot.
pub async fn save_report(state: &AppState, team_id: i64, nim: &str, id: Option<i64>, fields: &ReportFields) -> AppResult<i64> {
    let f = fields.cleaned();
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let ctx = team_ctx(&mut tx, team_id).await?;
    let now = now_ms();
    require_phase(&ctx.round, &[Phase::AttackWindow], now)?;
    let mut out = Outbox::default();
    let actor = format!("student:{nim}");
    let report_id = match id {
        Some(id) => {
            let r = report_by_id(&mut tx, id).await?;
            require(r.attacker_team_id == team_id && r.round_id == ctx.round.id, "Laporan bukan milik tim Anda.")?;
            require(r.status == "draft", "Laporan yang sudah disubmit terkunci. Revisi hanya jika instruktur meminta.")?;
            write_report_fields(&mut tx, id, &f).await?;
            id
        }
        None => {
            let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM attack_reports WHERE round_id = ? AND attacker_team_id = ?")
                .bind(ctx.round.id)
                .bind(team_id)
                .fetch_one(&mut *tx)
                .await?;
            require(total < MAX_REPORTS_TOTAL, "Batas jumlah laporan per ronde tercapai.")?;
            let snap = snapshot_of(&mut tx, ctx.round.id, ctx.target)
                .await?
                .ok_or_else(|| AppError::conflict("Snapshot target belum dikunci."))?;
            let id = sqlx::query(
                "INSERT INTO attack_reports (round_id, attacker_team_id, target_team_id, snapshot_id, status, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'draft', ?, ?, ?)",
            )
            .bind(ctx.round.id)
            .bind(team_id)
            .bind(ctx.target)
            .bind(snap.id)
            .bind(nim)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            write_report_fields(&mut tx, id, &f).await?;
            id
        }
    };
    let (attack_status, _) = team_status_codes(&mut tx, ctx.round.id, team_id).await?;
    if attack_status != AttackStatus::PreparingReport.code() {
        let reason = format!("Draft laporan #{report_id} disimpan oleh {nim}");
        set_attack(&mut tx, &mut out, ctx.session_id, ctx.round.id, team_id, AttackStatus::PreparingReport, &reason, &actor).await?;
    }
    tx.commit().await?;
    refresh(&mut out, team_id, &["reports_out"]);
    out.flush(&state.events);
    Ok(report_id)
}

pub async fn delete_report_draft(state: &AppState, team_id: i64, id: i64) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let r = report_by_id(&mut tx, id).await?;
    require(r.attacker_team_id == team_id, "Laporan bukan milik tim Anda.")?;
    require(r.status == "draft", "Hanya draft yang dapat dihapus. Laporan resmi tidak dapat dihapus.")?;
    sqlx::query("DELETE FROM attack_reports WHERE id = ? AND status = 'draft'").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    let mut out = Outbox::default();
    refresh(&mut out, team_id, &["reports_out"]);
    out.flush(&state.events);
    Ok(())
}

/// Earliest other official report against the same target in the same round that looks identical.
pub async fn duplicate_hint(conn: &mut SqliteConnection, r: &ReportRow) -> AppResult<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM attack_reports WHERE round_id = ? AND target_team_id = ? AND id <> ? AND status <> 'draft'
           AND (lower(trim(code_location)) = lower(trim(?)) OR lower(trim(title)) = lower(trim(?)))
           AND COALESCE(submitted_at, 0) <= COALESCE(?, 9223372036854775807)
         ORDER BY submitted_at, id LIMIT 1",
    )
    .bind(r.round_id)
    .bind(r.target_team_id)
    .bind(r.id)
    .bind(&r.code_location)
    .bind(&r.title)
    .bind(r.submitted_at)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn submit_report(state: &AppState, team_id: i64, nim: &str, id: i64) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let ctx = team_ctx(&mut tx, team_id).await?;
    let now = now_ms();
    require_phase(&ctx.round, &[Phase::AttackWindow], now)?;
    let r = report_by_id(&mut tx, id).await?;
    require(r.attacker_team_id == team_id && r.round_id == ctx.round.id, "Laporan bukan milik tim Anda.")?;
    require(r.status == "draft", "Laporan ini sudah disubmit.")?;
    let f = ReportFields {
        title: r.title.clone(),
        code_location: r.code_location.clone(),
        trigger_input: r.trigger_input.clone(),
        expected: r.expected.clone(),
        actual: r.actual.clone(),
        repro_steps: r.repro_steps.clone(),
        root_cause: r.root_cause.clone(),
        concept: r.concept.clone(),
        evidence: r.evidence.clone(),
    };
    let missing = f.missing();
    if !missing.is_empty() {
        return Err(AppError::bad(format!("Lengkapi dulu: {}.", missing.join(", "))));
    }
    let (official,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM attack_reports WHERE round_id = ? AND attacker_team_id = ? AND status <> 'draft'",
    )
    .bind(ctx.round.id)
    .bind(team_id)
    .fetch_one(&mut *tx)
    .await?;
    require(official < MAX_OFFICIAL_REPORTS, "Batas 6 laporan resmi per ronde tercapai.")?;
    let identical: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM attack_reports WHERE round_id = ? AND attacker_team_id = ? AND status <> 'draft' AND id <> ?
           AND lower(trim(code_location)) = lower(trim(?)) AND trim(trigger_input) = trim(?) LIMIT 1",
    )
    .bind(ctx.round.id)
    .bind(team_id)
    .bind(id)
    .bind(&r.code_location)
    .bind(&r.trigger_input)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((dup,)) = identical {
        return Err(AppError::conflict(format!("Laporan dengan lokasi dan pemicu yang sama sudah dikirim (#{dup}).")));
    }
    sqlx::query("UPDATE attack_reports SET status = 'submitted', submitted_at = ?, submitted_by = ?, revision = 1 WHERE id = ? AND status = 'draft'")
        .bind(now)
        .bind(nim)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO report_revisions (report_id, revision, content_json, reason, created_by, created_at) VALUES (?, 1, ?, 'submitted', ?, ?)")
        .bind(id)
        .bind(r.content_json().to_string())
        .bind(nim)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let mut out = Outbox::default();
    let actor = format!("student:{nim}");
    let target_name = db::team_name(&mut tx, ctx.target).await?;
    let attacker_name = db::team_name(&mut tx, team_id).await?;
    set_attack(&mut tx, &mut out, ctx.session_id, ctx.round.id, team_id, AttackStatus::ReportSubmitted, &format!("Laporan #{id} dikirim ke {target_name}"), &actor)
        .await?;
    let (_, defense_status) = team_status_codes(&mut tx, ctx.round.id, ctx.target).await?;
    let passive = matches!(
        DefenseStatus::from_code(&defense_status),
        Some(
            DefenseStatus::NotStarted
                | DefenseStatus::DefensePreparation
                | DefenseStatus::AwaitingAttack
                | DefenseStatus::AttackReceived
                | DefenseStatus::PatchSubmitted
                | DefenseStatus::AwaitingReview
                | DefenseStatus::PatchAccepted
                | DefenseStatus::PatchRejected
        )
    );
    let msg = format!("Laporan #{id} diterima dari {attacker_name}");
    if passive {
        set_defense(&mut tx, &mut out, ctx.session_id, ctx.round.id, ctx.target, DefenseStatus::AttackReceived, &msg, "system").await?;
    } else {
        log_activity(&mut tx, &mut out, ctx.session_id, ctx.round.id, Some(ctx.target), "report_received", &msg, "system").await?;
    }
    log_activity(
        &mut tx,
        &mut out,
        ctx.session_id,
        ctx.round.id,
        Some(team_id),
        "report_submitted",
        &format!("{attacker_name} → {target_name}: laporan #{id} dikirim"),
        &actor,
    )
    .await?;
    tx.commit().await?;
    drop(_w);
    refresh(&mut out, team_id, &["reports_out"]);
    refresh(&mut out, ctx.target, &["reports_in"]);
    out.push("review_queue", Audience::Instructors, json!({ "report_id": id }));
    out.flush(&state.events);
    crate::views::publish_live(state).await;
    Ok(())
}

pub async fn revise_report(state: &AppState, team_id: i64, nim: &str, id: i64, fields: &ReportFields, reason: &str) -> AppResult<()> {
    let f = fields.cleaned();
    let missing = f.missing();
    if !missing.is_empty() {
        return Err(AppError::bad(format!("Lengkapi dulu: {}.", missing.join(", "))));
    }
    let reason = clean_text(reason, 1000);
    if reason.is_empty() {
        return Err(AppError::bad("Jelaskan perubahan revisi."));
    }
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let ctx = team_ctx(&mut tx, team_id).await?;
    let now = now_ms();
    require_phase(&ctx.round, &[Phase::AttackWindow, Phase::DefenseResponse, Phase::Verification], now)?;
    let r = report_by_id(&mut tx, id).await?;
    require(r.attacker_team_id == team_id && r.round_id == ctx.round.id, "Laporan bukan milik tim Anda.")?;
    require(r.status == "needs_revision", "Revisi hanya dapat dilakukan jika instruktur meminta revisi.")?;
    write_report_fields(&mut tx, id, &f).await?;
    let new_status = if ctx.round.phase() == Phase::AttackWindow { "submitted" } else { "pending_review" };
    let revision = r.revision + 1;
    sqlx::query(
        "UPDATE attack_reports SET status = ?, revision = ?, score_repro = 0, score_rootcause = 0, score_evidence = 0 WHERE id = ?",
    )
    .bind(new_status)
    .bind(revision)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let content = json!({
        "title": f.title, "code_location": f.code_location, "trigger_input": f.trigger_input, "expected": f.expected,
        "actual": f.actual, "repro_steps": f.repro_steps, "root_cause": f.root_cause, "concept": f.concept, "evidence": f.evidence,
    });
    sqlx::query("INSERT INTO report_revisions (report_id, revision, content_json, reason, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(id)
        .bind(revision)
        .bind(content.to_string())
        .bind(format!("revision: {reason}"))
        .bind(nim)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let mut out = Outbox::default();
    let actor = format!("student:{nim}");
    set_attack(&mut tx, &mut out, ctx.session_id, ctx.round.id, team_id, AttackStatus::ReportSubmitted, &format!("Laporan #{id} direvisi (rev {revision})"), &actor)
        .await?;
    log_activity(&mut tx, &mut out, ctx.session_id, ctx.round.id, Some(r.target_team_id), "report_revised", &format!("Laporan #{id} direvisi oleh penyerang"), &actor)
        .await?;
    tx.commit().await?;
    drop(_w);
    refresh(&mut out, team_id, &["reports_out"]);
    refresh(&mut out, r.target_team_id, &["reports_in"]);
    out.push("review_queue", Audience::Instructors, json!({ "report_id": id }));
    out.flush(&state.events);
    crate::views::publish_live(state).await;
    Ok(())
}

/// Defender records explicit per-report progress (analysis notes / working state).
pub async fn update_report_defense(state: &AppState, team_id: i64, nim: &str, report_id: i64, state_code: &str, analysis: &str) -> AppResult<()> {
    let ds = ReportDefenseState::from_code(state_code).ok_or_else(|| AppError::bad("State tidak dikenal."))?;
    require(
        matches!(ds, ReportDefenseState::Analyzing | ReportDefenseState::Fixing | ReportDefenseState::Testing),
        "State ini diatur otomatis oleh sistem.",
    )?;
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let ctx = team_ctx(&mut tx, team_id).await?;
    require_phase(&ctx.round, &[Phase::AttackWindow, Phase::DefenseResponse], now_ms())?;
    let r = report_by_id(&mut tx, report_id).await?;
    require(r.target_team_id == team_id && r.round_id == ctx.round.id && r.status != "draft", "Laporan tidak ditujukan ke tim Anda.")?;
    sqlx::query("UPDATE attack_reports SET defense_state = ?, defense_analysis = ?, defense_updated_at = ? WHERE id = ?")
        .bind(ds.code())
        .bind(clean_text(analysis, 6000))
        .bind(now_ms())
        .bind(report_id)
        .execute(&mut *tx)
        .await?;
    let team_status = match ds {
        ReportDefenseState::Analyzing => DefenseStatus::AnalyzingBug,
        ReportDefenseState::Fixing => DefenseStatus::FixingBug,
        _ => DefenseStatus::WritingRegressionTest,
    };
    let mut out = Outbox::default();
    let reason = format!("Laporan #{report_id}: {} ({nim})", ds.label().to_lowercase());
    set_defense(&mut tx, &mut out, ctx.session_id, ctx.round.id, team_id, team_status, &reason, &format!("student:{nim}")).await?;
    tx.commit().await?;
    refresh(&mut out, team_id, &["reports_in"]);
    out.flush(&state.events);
    Ok(())
}

pub async fn submit_rebuttal(state: &AppState, team_id: i64, nim: &str, report_id: i64, text: &str) -> AppResult<()> {
    let text = clean_text(text, 6000);
    if text.chars().count() < 20 {
        return Err(AppError::bad("Rebuttal minimal 20 karakter dan harus berisi alasan teknis."));
    }
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let ctx = team_ctx(&mut tx, team_id).await?;
    require_phase(&ctx.round, &[Phase::AttackWindow, Phase::DefenseResponse, Phase::Verification], now_ms())?;
    let r = report_by_id(&mut tx, report_id).await?;
    require(r.target_team_id == team_id && r.round_id == ctx.round.id, "Laporan tidak ditujukan ke tim Anda.")?;
    require(
        matches!(r.status(), ReportStatus::Submitted | ReportStatus::PendingReview | ReportStatus::NeedsRevision),
        "Rebuttal hanya untuk laporan yang belum direview.",
    )?;
    sqlx::query("UPDATE attack_reports SET rebuttal = ?, rebuttal_at = ?, rebuttal_by = ?, defense_state = 'disputed', defense_updated_at = ? WHERE id = ?")
        .bind(&text)
        .bind(now_ms())
        .bind(nim)
        .bind(now_ms())
        .bind(report_id)
        .execute(&mut *tx)
        .await?;
    let mut out = Outbox::default();
    log_activity(&mut tx, &mut out, ctx.session_id, ctx.round.id, Some(team_id), "rebuttal", &format!("Rebuttal dikirim untuk laporan #{report_id}"), &format!("student:{nim}"))
        .await?;
    tx.commit().await?;
    refresh(&mut out, team_id, &["reports_in"]);
    refresh(&mut out, r.attacker_team_id, &["reports_out"]);
    out.push("review_queue", Audience::Instructors, json!({ "report_id": report_id }));
    out.flush(&state.events);
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PatchFields {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub root_cause: String,
    #[serde(default)]
    pub regression_tests: String,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub addressed: Vec<i64>,
}

/// Save the team's working patch draft (next version after the locked snapshot / last submission).
/// The locked snapshot attackers test is never modified.
pub async fn save_patch(state: &AppState, team_id: i64, nim: &str, fields: &PatchFields) -> AppResult<i64> {
    if fields.code.len() > MAX_CODE || fields.regression_tests.len() > MAX_CODE {
        return Err(AppError::bad("Kode atau test terlalu panjang (maks 100 KB)."));
    }
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let ctx = team_ctx(&mut tx, team_id).await?;
    let now = now_ms();
    require_phase(&ctx.round, &[Phase::AttackWindow, Phase::DefenseResponse], now)?;
    let incoming: Vec<(i64,)> = sqlx::query_as("SELECT id FROM attack_reports WHERE round_id = ? AND target_team_id = ? AND status <> 'draft'")
        .bind(ctx.round.id)
        .bind(team_id)
        .fetch_all(&mut *tx)
        .await?;
    let incoming: HashSet<i64> = incoming.into_iter().map(|r| r.0).collect();
    let mut addressed: Vec<i64> = fields.addressed.iter().copied().filter(|id| incoming.contains(id)).collect();
    addressed.sort_unstable();
    addressed.dedup();
    let root_cause = clean_text(&fields.root_cause, 6000);
    let explanation = clean_text(&fields.explanation, 6000);
    let draft = sqlx::query_as::<_, PatchRow>(&format!(
        "SELECT {PATCH_COLS} FROM patches WHERE round_id = ? AND team_id = ? AND status = 'draft' ORDER BY version DESC LIMIT 1"
    ))
    .bind(ctx.round.id)
    .bind(team_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (prev_code, prev_tests, prev_rc, prev_ex, version, patch_id, created) = match &draft {
        Some(d) => (d.code.clone(), d.regression_tests.clone(), d.root_cause.clone(), d.explanation.clone(), d.version, d.id, false),
        None => {
            let base: Option<(String, String, String, String)> = sqlx::query_as(
                "SELECT code, regression_tests, root_cause, explanation FROM patches WHERE round_id = ? AND team_id = ? ORDER BY version DESC LIMIT 1",
            )
            .bind(ctx.round.id)
            .bind(team_id)
            .fetch_optional(&mut *tx)
            .await?;
            let snap = snapshot_of(&mut tx, ctx.round.id, team_id).await?.ok_or_else(|| AppError::conflict("Snapshot belum dikunci."))?;
            let (bc, bt, br, be) = base.unwrap_or((snap.code, String::new(), String::new(), String::new()));
            let (max_v,): (i64,) = sqlx::query_as("SELECT COALESCE(MAX(version), 1) FROM patches WHERE round_id = ? AND team_id = ?")
                .bind(ctx.round.id)
                .bind(team_id)
                .fetch_one(&mut *tx)
                .await?;
            let version = max_v + 1;
            let id = sqlx::query(
                "INSERT INTO patches (round_id, team_id, version, status, code, created_at, updated_at, updated_by) VALUES (?, ?, ?, 'draft', ?, ?, ?, ?)",
            )
            .bind(ctx.round.id)
            .bind(team_id)
            .bind(version)
            .bind(&bc)
            .bind(now)
            .bind(now)
            .bind(nim)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            (bc, bt, br, be, version, id, true)
        }
    };
    sqlx::query(
        "UPDATE patches SET code = ?, root_cause = ?, regression_tests = ?, explanation = ?, addressed_reports = ?, updated_at = ?, updated_by = ?
         WHERE id = ? AND status = 'draft'",
    )
    .bind(&fields.code)
    .bind(&root_cause)
    .bind(&fields.regression_tests)
    .bind(&explanation)
    .bind(serde_json::to_string(&addressed)?)
    .bind(now)
    .bind(nim)
    .bind(patch_id)
    .execute(&mut *tx)
    .await?;

    let code_changed = fields.code != prev_code;
    let tests_changed = fields.regression_tests != prev_tests;
    let analysis_changed = root_cause != prev_rc || explanation != prev_ex;
    let mut out = Outbox::default();
    let actor = format!("student:{nim}");
    let new_status = if code_changed {
        Some((DefenseStatus::FixingBug, format!("Patch v{version} draft: kode diperbarui oleh {nim}")))
    } else if tests_changed {
        Some((
            DefenseStatus::WritingRegressionTest,
            format!("Patch v{version}: {} regression test ditulis ({nim})", count_python_tests(&fields.regression_tests)),
        ))
    } else if analysis_changed {
        Some((DefenseStatus::AnalyzingBug, format!("Patch v{version}: analisis root cause diperbarui ({nim})")))
    } else {
        None
    };
    if let Some((status, reason)) = new_status {
        set_defense(&mut tx, &mut out, ctx.session_id, ctx.round.id, team_id, status, &reason, &actor).await?;
    }
    if created {
        log_activity(&mut tx, &mut out, ctx.session_id, ctx.round.id, Some(team_id), "patch_draft", &format!("Patch v{version} draft dibuat"), &actor)
            .await?;
    }
    tx.commit().await?;
    refresh(&mut out, team_id, &["patches"]);
    out.flush(&state.events);
    Ok(version)
}

pub async fn submit_patch(state: &AppState, team_id: i64, nim: &str) -> AppResult<i64> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let ctx = team_ctx(&mut tx, team_id).await?;
    let now = now_ms();
    require_phase(&ctx.round, &[Phase::AttackWindow, Phase::DefenseResponse], now)?;
    let draft = sqlx::query_as::<_, PatchRow>(&format!(
        "SELECT {PATCH_COLS} FROM patches WHERE round_id = ? AND team_id = ? AND status = 'draft' ORDER BY version DESC LIMIT 1"
    ))
    .bind(ctx.round.id)
    .bind(team_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::conflict("Simpan draft patch terlebih dahulu."))?;
    if draft.code.trim().is_empty() || draft.root_cause.trim().is_empty() {
        return Err(AppError::bad("Patch wajib memuat kode dan analisis root cause."));
    }
    if draft.test_count() == 0 {
        return Err(AppError::bad("Patch wajib memuat minimal satu regression test (def test_...)."));
    }
    let (submitted,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM patches WHERE round_id = ? AND team_id = ? AND status <> 'draft'")
        .bind(ctx.round.id)
        .bind(team_id)
        .fetch_one(&mut *tx)
        .await?;
    require(submitted < MAX_PATCH_SUBMISSIONS, "Batas 5 submission patch per ronde tercapai.")?;
    sqlx::query("UPDATE patches SET status = 'submitted', submitted_at = ?, submitted_by = ? WHERE id = ? AND status = 'draft'")
        .bind(now)
        .bind(nim)
        .bind(draft.id)
        .execute(&mut *tx)
        .await?;
    for rid in draft.addressed() {
        sqlx::query("UPDATE attack_reports SET defense_state = 'patched', defense_updated_at = ? WHERE id = ? AND target_team_id = ?")
            .bind(now)
            .bind(rid)
            .bind(team_id)
            .execute(&mut *tx)
            .await?;
    }
    let mut out = Outbox::default();
    let actor = format!("student:{nim}");
    let reason = format!("Patch v{} disubmit oleh {nim} — {} regression test", draft.version, draft.test_count());
    set_defense(&mut tx, &mut out, ctx.session_id, ctx.round.id, team_id, DefenseStatus::PatchSubmitted, &reason, &actor).await?;
    tx.commit().await?;
    drop(_w);
    refresh(&mut out, team_id, &["patches", "reports_in"]);
    out.push("review_queue", Audience::Instructors, json!({ "patch_id": draft.id }));
    out.flush(&state.events);
    crate::views::publish_live(state).await;
    Ok(draft.version)
}

// ------------------------------------------------------------------------------------------------
// Instructor reviews and score corrections

#[derive(Debug, Clone, Deserialize)]
pub struct ReportReview {
    pub report_id: i64,
    pub verdict: String,
    #[serde(default)]
    pub repro: i64,
    #[serde(default)]
    pub rootcause: i64,
    #[serde(default)]
    pub evidence: i64,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub duplicate_of: Option<i64>,
    #[serde(default)]
    pub rebuttal_bonus: i64,
}

pub async fn review_report(state: &AppState, rv: &ReportReview) -> AppResult<()> {
    let verdict = ReportStatus::from_code(&rv.verdict).ok_or_else(|| AppError::bad("Verdict tidak dikenal."))?;
    require(
        matches!(verdict, ReportStatus::Valid | ReportStatus::Invalid | ReportStatus::Duplicate | ReportStatus::NeedsRevision),
        "Verdict harus valid, invalid, duplicate, atau needs_revision.",
    )?;
    let note = clean_text(&rv.note, 4000);
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let r = report_by_id(&mut tx, rv.report_id).await?;
    let round = round_by_id(&mut tx, r.round_id).await?;
    require(round.is_running(), "Ronde sudah difinalisasi. Gunakan koreksi skor yang tercatat.")?;
    require(r.status != "draft", "Draft belum disubmit.")?;
    if verdict != ReportStatus::Valid && note.is_empty() {
        return Err(AppError::bad("Catatan review wajib diisi untuk verdict selain valid (umpan balik edukatif)."));
    }
    let (repro, rootcause, evidence) = if verdict == ReportStatus::Valid {
        (rv.repro.clamp(0, rubric::ATTACK_REPRO), rv.rootcause.clamp(0, rubric::ATTACK_ROOTCAUSE), rv.evidence.clamp(0, rubric::ATTACK_EVIDENCE))
    } else {
        (0, 0, 0)
    };
    let bonus = if verdict == ReportStatus::Invalid { rv.rebuttal_bonus.clamp(0, rubric::REBUTTAL_MAX) } else { 0 };
    if bonus > 0 && r.rebuttal.trim().is_empty() {
        return Err(AppError::bad("Bonus rebuttal hanya dapat diberikan jika defender mengirim rebuttal."));
    }
    let dup = if verdict == ReportStatus::Duplicate {
        let d = rv.duplicate_of.ok_or_else(|| AppError::bad("Pilih laporan asli untuk verdict duplicate."))?;
        require(d != r.id, "Laporan tidak bisa menjadi duplikat dirinya sendiri.")?;
        let orig = report_by_id(&mut tx, d).await?;
        require(orig.round_id == r.round_id && orig.target_team_id == r.target_team_id && orig.status != "draft", "Laporan asli harus laporan resmi pada target dan ronde yang sama.")?;
        Some(d)
    } else {
        None
    };
    let now = now_ms();
    sqlx::query(
        "UPDATE attack_reports SET status = ?, score_repro = ?, score_rootcause = ?, score_evidence = ?, review_note = ?, duplicate_of = ?,
         rebuttal_bonus = ?, reviewer = 'instructor', reviewed_at = ? WHERE id = ?",
    )
    .bind(verdict.code())
    .bind(repro)
    .bind(rootcause)
    .bind(evidence)
    .bind(&note)
    .bind(dup)
    .bind(bonus)
    .bind(now)
    .bind(r.id)
    .execute(&mut *tx)
    .await?;
    db::audit(
        &mut tx,
        Some(round.session_id),
        "instructor",
        "report_reviewed",
        "attack_report",
        r.id,
        json!({
            "old": { "status": r.status, "repro": r.score_repro, "rootcause": r.score_rootcause, "evidence": r.score_evidence, "rebuttal_bonus": r.rebuttal_bonus },
            "new": { "status": verdict.code(), "repro": repro, "rootcause": rootcause, "evidence": evidence, "rebuttal_bonus": bonus, "duplicate_of": dup },
            "note": note,
        }),
    )
    .await?;
    let mut out = Outbox::default();
    let pts = scoring::report_points(repro, rootcause, evidence);
    let (status, reason) = match verdict {
        ReportStatus::Valid => (AttackStatus::ValidFinding, format!("Laporan #{} valid — rubric {pts}/25", r.id)),
        ReportStatus::Invalid => (AttackStatus::InvalidFinding, format!("Laporan #{} dinyatakan tidak valid", r.id)),
        ReportStatus::Duplicate => (AttackStatus::DuplicateFinding, format!("Laporan #{} duplikat dari #{}", r.id, dup.unwrap_or(0))),
        _ => (AttackStatus::PreparingReport, format!("Instruktur meminta revisi laporan #{}", r.id)),
    };
    set_attack(&mut tx, &mut out, round.session_id, round.id, r.attacker_team_id, status, &reason, "instructor").await?;
    if bonus > 0 {
        log_activity(&mut tx, &mut out, round.session_id, round.id, Some(r.target_team_id), "rebuttal_bonus", &format!("Rebuttal laporan #{} diterima (+{bonus})", r.id), "instructor")
            .await?;
    }
    tx.commit().await?;
    drop(_w);
    refresh(&mut out, r.attacker_team_id, &["reports_out"]);
    refresh(&mut out, r.target_team_id, &["reports_in"]);
    out.push("review_queue", Audience::Instructors, json!({ "report_id": r.id }));
    out.flush(&state.events);
    crate::views::publish_live(state).await;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchReview {
    pub patch_id: i64,
    pub verdict: String,
    #[serde(default)]
    pub rootfix: i64,
    #[serde(default)]
    pub regression: i64,
    #[serde(default)]
    pub edge: i64,
    #[serde(default)]
    pub explain: i64,
    #[serde(default)]
    pub note: String,
}

pub async fn review_patch(state: &AppState, rv: &PatchReview) -> AppResult<()> {
    let verdict = PatchStatus::from_code(&rv.verdict).ok_or_else(|| AppError::bad("Verdict tidak dikenal."))?;
    require(matches!(verdict, PatchStatus::Accepted | PatchStatus::Rejected), "Verdict harus accepted atau rejected.")?;
    let note = clean_text(&rv.note, 4000);
    if verdict == PatchStatus::Rejected && note.is_empty() {
        return Err(AppError::bad("Alasan penolakan patch wajib diisi."));
    }
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let p = patch_by_id(&mut tx, rv.patch_id).await?;
    let round = round_by_id(&mut tx, p.round_id).await?;
    require(round.is_running(), "Ronde sudah difinalisasi. Gunakan koreksi skor yang tercatat.")?;
    require(p.status != "draft", "Patch draft belum disubmit.")?;
    let (a, b, c, d) = (
        rv.rootfix.clamp(0, rubric::DEFENSE_ROOTFIX),
        rv.regression.clamp(0, rubric::DEFENSE_REGRESSION),
        rv.edge.clamp(0, rubric::DEFENSE_EDGE),
        rv.explain.clamp(0, rubric::DEFENSE_EXPLAIN),
    );
    sqlx::query(
        "UPDATE patches SET status = ?, score_rootfix = ?, score_regression = ?, score_edge = ?, score_explain = ?, review_note = ?,
         reviewer = 'instructor', reviewed_at = ? WHERE id = ?",
    )
    .bind(verdict.code())
    .bind(a)
    .bind(b)
    .bind(c)
    .bind(d)
    .bind(&note)
    .bind(now_ms())
    .bind(p.id)
    .execute(&mut *tx)
    .await?;
    db::audit(
        &mut tx,
        Some(round.session_id),
        "instructor",
        "patch_reviewed",
        "patch",
        p.id,
        json!({
            "old": { "status": p.status, "rootfix": p.score_rootfix, "regression": p.score_regression, "edge": p.score_edge, "explain": p.score_explain },
            "new": { "status": verdict.code(), "rootfix": a, "regression": b, "edge": c, "explain": d },
            "note": note,
        }),
    )
    .await?;
    let pts = scoring::patch_points(a, b, c, d);
    let mut out = Outbox::default();
    let (status, reason) = if verdict == PatchStatus::Accepted {
        (DefenseStatus::PatchAccepted, format!("Patch v{} diterima — rubric {pts}/50", p.version))
    } else {
        (DefenseStatus::PatchRejected, format!("Patch v{} ditolak — rubric {pts}/50", p.version))
    };
    set_defense(&mut tx, &mut out, round.session_id, round.id, p.team_id, status, &reason, "instructor").await?;
    tx.commit().await?;
    drop(_w);
    refresh(&mut out, p.team_id, &["patches"]);
    out.push("review_queue", Audience::Instructors, json!({ "patch_id": p.id }));
    out.flush(&state.events);
    crate::views::publish_live(state).await;
    Ok(())
}

pub async fn adjust_score(state: &AppState, team_id: i64, round_id: Option<i64>, points: i64, reason: &str) -> AppResult<()> {
    let reason = clean_text(reason, 1000);
    if reason.is_empty() {
        return Err(AppError::bad("Alasan koreksi skor wajib diisi."));
    }
    if points == 0 || !(-100..=100).contains(&points) {
        return Err(AppError::bad("Koreksi harus antara -100 dan 100 (bukan 0)."));
    }
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let s = current_session_conn(&mut tx).await?;
    let (exists,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM teams WHERE id = ? AND session_id = ? AND confirmed = 1")
        .bind(team_id)
        .bind(s.id)
        .fetch_one(&mut *tx)
        .await?;
    require(exists == 1, "Kelompok tidak ditemukan.")?;
    sqlx::query("INSERT INTO score_adjustments (session_id, round_id, team_id, points, reason, actor, created_at) VALUES (?, ?, ?, ?, ?, 'instructor', ?)")
        .bind(s.id)
        .bind(round_id)
        .bind(team_id)
        .bind(points)
        .bind(&reason)
        .bind(now_ms())
        .execute(&mut *tx)
        .await?;
    db::audit(&mut tx, Some(s.id), "instructor", "score_adjusted", "team", team_id, json!({ "points": points, "round_id": round_id, "reason": reason }))
        .await?;
    let mut out = Outbox::default();
    if let Some(rid) = round_id {
        log_activity(&mut tx, &mut out, s.id, rid, Some(team_id), "score_adjusted", &format!("Koreksi skor {points:+}: {reason}"), "instructor").await?;
    }
    tx.commit().await?;
    drop(_w);
    out.flush(&state.events);
    crate::views::publish_live(state).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn ring_pairing_is_a_valid_derangement() {
        let teams = vec![1, 2, 3, 4, 5];
        for offset in 1..5 {
            let p = ring_pairing(&teams, offset);
            assert!(validate_pairing(&teams, &p).is_ok(), "offset {offset}");
        }
    }

    #[test]
    fn validate_rejects_bad_pairings() {
        let teams = vec![1, 2, 3];
        assert!(validate_pairing(&teams, &[(1, 1), (2, 3), (3, 2)]).is_err());
        assert!(validate_pairing(&teams, &[(1, 2), (2, 2), (3, 1)]).is_err());
        assert!(validate_pairing(&teams, &[(1, 2), (2, 3)]).is_err());
        assert!(validate_pairing(&teams, &[(1, 2), (2, 3), (3, 9)]).is_err());
        assert!(validate_pairing(&teams, &[(1, 2), (2, 3), (3, 1)]).is_ok());
    }

    #[test]
    fn generated_pairing_avoids_mutual_pairs_and_repeats() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for n in 2..=10 {
            let teams: Vec<i64> = (1..=n).collect();
            let p = generate_pairing(&teams, &HashSet::new(), &mut rng);
            assert!(validate_pairing(&teams, &p).is_ok());
            if n > 2 {
                let set: HashSet<(i64, i64)> = p.iter().copied().collect();
                assert!(p.iter().all(|(a, t)| !set.contains(&(*t, *a))), "mutual pair for n={n}: {p:?}");
            }
            let prev: HashSet<(i64, i64)> = p.iter().copied().collect();
            if n >= 4 {
                let q = generate_pairing(&teams, &prev, &mut rng);
                assert_eq!(q.iter().filter(|x| prev.contains(x)).count(), 0, "repeat for n={n}");
            }
        }
    }

    #[test]
    fn durations_are_clamped() {
        let d = Durations { briefing: -5, defense_prep: 10_000_000, attack_window: 60, defense_response: 0, verification: 0 }.sanitized();
        assert_eq!(d.briefing, 0);
        assert_eq!(d.defense_prep, 4 * 3600);
        assert_eq!(d.secs(Phase::AttackWindow), 60);
    }
}
