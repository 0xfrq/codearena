//! Snapshot builders. Every number shown in the arena is computed here from the database —
//! the browser never derives scores or counts from events on its own.
//! `instructor == false` produces the public view: no report titles/content, no unlocked pairings.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::challenges;
use crate::db::{self, SessionRow};
use crate::error::AppResult;
use crate::events::{Audience, Viewer};
use crate::engine::{self, RoundRow};
use crate::models::*;
use crate::scoring;
use crate::state::AppState;
use crate::util::{count_python_tests, now_ms};

pub async fn round_view(state: &AppState, round: &RoundRow) -> AppResult<Value> {
    let now = now_ms();
    let phases: Vec<(String, i64, Option<i64>, Option<i64>, i64)> = sqlx::query_as(
        "SELECT phase, started_at, ended_at, planned_secs, paused_ms FROM round_phases WHERE round_id = ? ORDER BY id",
    )
    .bind(round.id)
    .fetch_all(&state.db)
    .await?;
    let by_phase: HashMap<String, (i64, Option<i64>, Option<i64>, i64)> =
        phases.into_iter().map(|(p, s, e, pl, pm)| (p, (s, e, pl, pm))).collect();
    let report_times: Vec<(i64,)> =
        sqlx::query_as("SELECT submitted_at FROM attack_reports WHERE round_id = ? AND submitted_at IS NOT NULL")
            .bind(round.id)
            .fetch_all(&state.db)
            .await?;
    let patch_times: Vec<(i64,)> = sqlx::query_as("SELECT submitted_at FROM patches WHERE round_id = ? AND submitted_at IS NOT NULL")
        .bind(round.id)
        .fetch_all(&state.db)
        .await?;
    let (prep_saved,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM prep_drafts WHERE round_id = ? AND saves > 0")
        .bind(round.id)
        .fetch_one(&state.db)
        .await?;
    let durations = round.durations();
    let current = round.phase();
    let finalized = round.state == "finalized";
    let mut timeline = Vec::new();
    for p in Phase::TIMELINE {
        let rec = by_phase.get(p.code());
        let st = if finalized || rec.is_some_and(|r| r.1.is_some()) {
            "completed"
        } else if rec.is_some() && p == current {
            "active"
        } else {
            "upcoming"
        };
        let (started, ended, planned, paused_ms) = match rec {
            Some((s, e, pl, pm)) => (Some(*s), *e, pl.unwrap_or(0), *pm),
            None => (None, None, durations.secs(p), 0),
        };
        let in_window = |t: i64| started.is_some_and(|s| t >= s && ended.is_none_or(|e| t < e));
        let submissions = match p {
            Phase::DefensePrep if started.is_some() => prep_saved,
            _ => report_times.iter().filter(|t| in_window(t.0)).count() as i64
                + patch_times.iter().filter(|t| in_window(t.0)).count() as i64,
        };
        timeline.push(json!({
            "phase": p.code(), "label": p.label(), "state": st, "started_at": started, "ended_at": ended,
            "planned_secs": planned, "paused_ms": paused_ms,
            "actual_ms": started.map(|s| ended.unwrap_or(now) - s - paused_ms),
            "submissions": submissions, "auto": p.auto_advances(), "description": p.description(),
        }));
    }
    let challenge = challenges::load(&state.db, &round.challenge_id).await?;
    Ok(json!({
        "id": round.id, "number": round.number, "state": round.state,
        "phase": current.code(), "phase_label": current.label(), "phase_tone": current.tone().code(),
        "phase_description": current.description(), "phase_index": current.index(),
        "paused": round.is_paused(), "deadline": round.phase_deadline, "remaining_ms": round.remaining_ms(now),
        "phase_started_at": round.phase_started_at, "auto_advance": round.auto_advance == 1,
        "discussion_open": round.discussion_open == 1, "finalized": finalized,
        "challenge": { "id": challenge.id(), "title": challenge.title(), "label": challenge.label(), "materi": challenge.materi_label() },
        "timeline": timeline,
    }))
}

struct TeamBasic {
    id: i64,
    name: String,
    members: Vec<String>,
}

async fn confirmed_teams(state: &AppState, session_id: i64) -> AppResult<Vec<TeamBasic>> {
    let rows: Vec<(i64, String)> = sqlx::query_as("SELECT id, name FROM teams WHERE session_id = ? AND confirmed = 1 ORDER BY id")
        .bind(session_id)
        .fetch_all(&state.db)
        .await?;
    let members: Vec<(i64, String)> = sqlx::query_as(
        "SELECT tm.team_id, tm.nim FROM team_members tm JOIN teams t ON t.id = tm.team_id
         WHERE tm.session_id = ? AND t.confirmed = 1 ORDER BY tm.nim",
    )
    .bind(session_id)
    .fetch_all(&state.db)
    .await?;
    let mut by_team: HashMap<i64, Vec<String>> = HashMap::new();
    for (t, n) in members {
        by_team.entry(t).or_default().push(n);
    }
    Ok(rows
        .into_iter()
        .map(|(id, name)| TeamBasic { id, name, members: by_team.remove(&id).unwrap_or_default() })
        .collect())
}

/// Aggregated, server-computed numbers: attack edges, per-team counters and the global summary.
pub async fn stats(state: &AppState, session: &SessionRow, round: Option<&RoundRow>, instructor: bool) -> AppResult<Value> {
    let teams = confirmed_teams(state, session.id).await?;
    let online: HashSet<String> = state.presence.online().into_iter().collect();
    let (joined,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM participants WHERE session_id = ? AND joined_at IS NOT NULL")
        .bind(session.id)
        .fetch_one(&state.db)
        .await?;
    let participants: Vec<(String,)> = sqlx::query_as("SELECT nim FROM participants WHERE session_id = ? AND joined_at IS NOT NULL")
        .bind(session.id)
        .fetch_all(&state.db)
        .await?;
    let online_count = participants.iter().filter(|p| online.contains(&p.0)).count();

    let mut edges = Vec::new();
    let mut team_stats: serde_json::Map<String, Value> = serde_json::Map::new();
    let mut summary = json!({
        "teams": teams.len(), "participants": joined, "online": online_count,
        "reports": 0, "valid": 0, "invalid": 0, "pending_reports": 0, "verified_patches": 0,
        "submitted_patches": 0, "awaiting_review_teams": 0, "disputes": 0,
    });
    let Some(round) = round else {
        return Ok(json!({ "edges": edges, "teams": team_stats, "summary": summary }));
    };
    let mut conn = state.db.acquire().await?;
    let pairs = engine::pairings(&mut conn, round.id).await?;
    drop(conn);
    let locked = round.state != "setup";
    let reports: Vec<(i64, i64, i64, String, String)> = sqlx::query_as(
        "SELECT id, attacker_team_id, target_team_id, status, rebuttal FROM attack_reports WHERE round_id = ?",
    )
    .bind(round.id)
    .fetch_all(&state.db)
    .await?;
    let patches: Vec<(i64, i64, String, String, String)> = sqlx::query_as(
        "SELECT team_id, version, status, addressed_reports, regression_tests FROM patches WHERE round_id = ? ORDER BY version",
    )
    .bind(round.id)
    .fetch_all(&state.db)
    .await?;
    let preps: Vec<(i64, i64, Option<i64>)> =
        sqlx::query_as("SELECT team_id, saves, ready_at FROM prep_drafts WHERE round_id = ?").bind(round.id).fetch_all(&state.db).await?;
    let snaps: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT team_id, code_sha256, source FROM defense_snapshots WHERE round_id = ?").bind(round.id).fetch_all(&state.db).await?;

    let mut fixed: HashSet<i64> = HashSet::new();
    for (_, _, status, addressed, _) in &patches {
        if status == "accepted" {
            fixed.extend(serde_json::from_str::<Vec<i64>>(addressed).unwrap_or_default());
        }
    }
    let count = |f: &dyn Fn(&(i64, i64, i64, String, String)) -> bool| reports.iter().filter(|r| f(r)).count();
    let mut awaiting_teams = 0;
    for t in &teams {
        let out = |s: &str| count(&|r| r.1 == t.id && r.3 == s);
        let inc = |s: &str| count(&|r| r.2 == t.id && r.3 == s);
        let outgoing_official = count(&|r| r.1 == t.id && r.3 != "draft");
        let outgoing_pending = out("submitted") + out("pending_review");
        let open_findings = count(&|r| {
            r.2 == t.id && matches!(r.3.as_str(), "submitted" | "pending_review" | "valid" | "needs_revision") && !fixed.contains(&r.0)
        });
        let disputes = count(&|r| r.2 == t.id && !r.4.trim().is_empty());
        let latest_patch = patches.iter().filter(|p| p.0 == t.id).last();
        let patch_pending = patches.iter().any(|p| p.0 == t.id && p.2 == "submitted");
        if patch_pending || outgoing_pending > 0 {
            awaiting_teams += 1;
        }
        let prep = preps.iter().find(|p| p.0 == t.id);
        let snap = snaps.iter().find(|s| s.0 == t.id);
        team_stats.insert(
            t.id.to_string(),
            json!({
                "outgoing": { "official": outgoing_official, "pending": outgoing_pending, "valid": out("valid"),
                              "invalid": out("invalid"), "duplicate": out("duplicate"), "needs_revision": out("needs_revision"),
                              "drafting": instructor && count(&|r| r.1 == t.id && r.3 == "draft") > 0 },
                "incoming": { "official": count(&|r| r.2 == t.id && r.3 != "draft"), "pending": inc("submitted") + inc("pending_review"),
                              "valid": inc("valid"), "invalid": inc("invalid"), "duplicate": inc("duplicate") },
                "open_findings": open_findings,
                "disputes": disputes,
                "patch": latest_patch.map(|p| {
                    let st = PatchStatus::from_code(&p.2).unwrap_or(PatchStatus::Draft);
                    json!({ "version": p.1, "status": p.2, "label": st.label(), "tone": st.tone().code(), "icon": st.tone().icon(),
                            "tests": count_python_tests(&p.4) })
                }),
                "patches_accepted": patches.iter().filter(|p| p.0 == t.id && p.2 == "accepted").count(),
                "prep": prep.map(|p| json!({ "saves": p.1, "ready": p.2.is_some() })),
                "snapshot": snap.map(|s| json!({ "sha": &s.1[..12], "source": s.2 })),
            }),
        );
    }
    if locked || instructor {
        for (a, t, l) in &pairs {
            let rel = |s: &str| count(&|r| r.1 == *a && r.2 == *t && r.3 == s);
            edges.push(json!({
                "attacker_id": a, "target_id": t, "round": round.number, "locked": *l == 1, "proposed": !locked,
                "counts": {
                    "official": count(&|r| r.1 == *a && r.2 == *t && r.3 != "draft"),
                    "pending": rel("submitted") + rel("pending_review"),
                    "valid": rel("valid"), "invalid": rel("invalid"), "duplicate": rel("duplicate"), "needs_revision": rel("needs_revision"),
                },
            }));
        }
    }
    summary["reports"] = json!(count(&|r| r.3 != "draft"));
    summary["valid"] = json!(count(&|r| r.3 == "valid"));
    summary["invalid"] = json!(count(&|r| r.3 == "invalid" || r.3 == "duplicate"));
    summary["pending_reports"] = json!(count(&|r| r.3 == "submitted" || r.3 == "pending_review"));
    summary["verified_patches"] = json!(patches.iter().filter(|p| p.2 == "accepted").count());
    summary["submitted_patches"] = json!(patches.iter().filter(|p| p.2 != "draft").count());
    summary["awaiting_review_teams"] = json!(awaiting_teams);
    summary["disputes"] = json!(count(&|r| r.3 != "draft" && !r.4.trim().is_empty()));
    Ok(json!({ "edges": edges, "teams": team_stats, "summary": summary }))
}

pub async fn scores_value(state: &AppState, session_id: i64) -> AppResult<Value> {
    Ok(serde_json::to_value(scoring::session_scores(&state.db, session_id).await?)?)
}

/// Full arena snapshot (public or instructor).
pub async fn arena_snapshot(state: &AppState, instructor: bool) -> AppResult<Value> {
    let session = db::current_session(&state.db).await?;
    let mut conn = state.db.acquire().await?;
    let round = engine::current_round(&mut conn, session.id).await?;
    drop(conn);
    let teams = confirmed_teams(state, session.id).await?;
    let online: HashSet<String> = state.presence.online().into_iter().collect();
    let statuses: Vec<(i64, String, String, i64, String, String, i64)> = match &round {
        Some(r) => sqlx::query_as(
            "SELECT team_id, attack_status, attack_reason, attack_at, defense_status, defense_reason, defense_at FROM team_status WHERE round_id = ?",
        )
        .bind(r.id)
        .fetch_all(&state.db)
        .await?,
        None => vec![],
    };
    let pairs = match &round {
        Some(r) if r.state != "setup" || instructor => {
            let mut c = state.db.acquire().await?;
            engine::pairings(&mut c, r.id).await?
        }
        _ => vec![],
    };
    let team_values: Vec<Value> = teams
        .iter()
        .map(|t| {
            let st = statuses.iter().find(|s| s.0 == t.id);
            json!({
                "id": t.id, "name": t.name,
                "members": t.members.iter().map(|n| json!({ "nim": n, "online": online.contains(n) })).collect::<Vec<_>>(),
                "online": t.members.iter().filter(|n| online.contains(*n)).count(),
                "attack": st.map(|s| StatusView::attack(&s.1, &s.2, s.3)),
                "defense": st.map(|s| StatusView::defense(&s.4, &s.5, s.6)),
                "target_id": pairs.iter().find(|p| p.0 == t.id).map(|p| p.1),
                "attacker_id": pairs.iter().find(|p| p.1 == t.id).map(|p| p.0),
            })
        })
        .collect();
    let round_value = match &round {
        Some(r) => Some(round_view(state, r).await?),
        None => None,
    };
    let feed: Vec<(i64, Option<i64>, String, Option<String>, Option<String>, String, i64)> = match &round {
        Some(r) => sqlx::query_as(
            "SELECT id, team_id, kind, side, status, message, created_at FROM activity WHERE round_id = ? ORDER BY id DESC LIMIT 60",
        )
        .bind(r.id)
        .fetch_all(&state.db)
        .await?,
        None => vec![],
    };
    let feed: Vec<Value> = feed
        .into_iter()
        .map(|(id, team_id, kind, side, status, message, at)| {
            json!({ "id": id, "team_id": team_id, "kind": kind, "side": side, "status": status, "message": message, "at": at })
        })
        .collect();
    Ok(json!({
        "now": now_ms(),
        "instructor": instructor,
        "session": { "id": session.id, "name": session.name, "status": session.status, "teams_locked": session.teams_locked == 1 },
        "round": round_value,
        "teams": team_values,
        "stats": stats(state, &session, round.as_ref(), instructor).await?,
        "scores": scores_value(state, session.id).await?,
        "feed": feed,
        "statuses": {
            "attack": AttackStatus::ALL.iter().map(|s| json!({ "code": s.code(), "label": s.label(), "tone": s.tone().code(), "icon": s.tone().icon() })).collect::<Vec<_>>(),
            "defense": DefenseStatus::ALL.iter().map(|s| json!({ "code": s.code(), "label": s.label(), "tone": s.tone().code(), "icon": s.tone().icon() })).collect::<Vec<_>>(),
        },
    }))
}

pub async fn waiting_snapshot(state: &AppState, viewer: &Viewer) -> AppResult<Value> {
    let session = db::current_session(&state.db).await?;
    let online: HashSet<String> = state.presence.online().into_iter().collect();
    let rows: Vec<(String, Option<i64>, Option<i64>, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT p.nim, p.joined_at, t.id, t.confirmed, t.name FROM participants p
         LEFT JOIN team_members tm ON tm.session_id = p.session_id AND tm.nim = p.nim
         LEFT JOIN teams t ON t.id = tm.team_id
         WHERE p.session_id = ? ORDER BY p.joined_at IS NULL, p.joined_at, p.nim",
    )
    .bind(session.id)
    .fetch_all(&state.db)
    .await?;
    let instructor = viewer.is_instructor();
    let participants: Vec<Value> = rows
        .iter()
        .filter(|r| r.1.is_some() || instructor)
        .map(|(nim, joined, team_id, confirmed, team_name)| {
            let confirmed = *confirmed == Some(1);
            json!({
                "nim": nim, "online": online.contains(nim), "joined": joined.is_some(), "joined_at": joined,
                "assigned": confirmed || (instructor && team_id.is_some()),
                "team": if confirmed || instructor { team_name.clone() } else { None },
                "draft": instructor && team_id.is_some() && !confirmed,
            })
        })
        .collect();
    let (drafts,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM teams WHERE session_id = ? AND confirmed = 0")
        .bind(session.id)
        .fetch_one(&state.db)
        .await?;
    let (confirmed,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM teams WHERE session_id = ? AND confirmed = 1")
        .bind(session.id)
        .fetch_one(&state.db)
        .await?;
    let my_team = match viewer {
        Viewer::Student { team_id: Some(tid), .. } => {
            let name: Option<(String,)> = sqlx::query_as("SELECT name FROM teams WHERE id = ?").bind(tid).fetch_optional(&state.db).await?;
            let members: Vec<(String,)> =
                sqlx::query_as("SELECT nim FROM team_members WHERE team_id = ? ORDER BY nim").bind(tid).fetch_all(&state.db).await?;
            name.map(|(n,)| {
                json!({ "id": tid, "name": n,
                        "members": members.iter().map(|m| json!({ "nim": m.0, "online": online.contains(&m.0) })).collect::<Vec<_>>() })
            })
        }
        _ => None,
    };
    let joined: Vec<&Value> = participants.iter().filter(|p| p["joined"] == json!(true)).collect();
    Ok(json!({
        "now": now_ms(),
        "session": { "id": session.id, "name": session.name, "teams_locked": session.teams_locked == 1 },
        "me": match viewer { Viewer::Student { nim, .. } => json!({ "nim": nim }), Viewer::Instructor => json!({ "instructor": true }) },
        "my_team": my_team,
        "participants": participants,
        "counts": {
            "joined": joined.len(),
            "online": joined.iter().filter(|p| p["online"] == json!(true)).count(),
            "assigned": joined.iter().filter(|p| p["assigned"] == json!(true)).count(),
        },
        "assignment": if confirmed > 0 { "confirmed" } else if drafts > 0 { "preview" } else { "waiting" },
    }))
}

/// Viewer-specific snapshot sent over the WebSocket on connect and after structural changes.
pub async fn snapshot_for(state: &AppState, viewer: &Viewer, view: &str) -> AppResult<Value> {
    let instructor = viewer.is_instructor();
    Ok(match view {
        "wait" => json!({ "waiting": waiting_snapshot(state, viewer).await? }),
        "admin" if instructor => json!({ "arena": arena_snapshot(state, true).await?, "waiting": waiting_snapshot(state, viewer).await? }),
        "team" => json!({
            "arena": arena_snapshot(state, false).await?,
            "waiting": waiting_snapshot(state, viewer).await?,
        }),
        _ => json!({ "arena": arena_snapshot(state, instructor).await? }),
    })
}

/// Recompute and publish aggregated counters and scores (after reports, patches, reviews).
pub async fn publish_live(state: &AppState) {
    let result: AppResult<()> = async {
        let session = db::current_session(&state.db).await?;
        let mut conn = state.db.acquire().await?;
        let round = engine::current_round(&mut conn, session.id).await?;
        drop(conn);
        let public = stats(state, &session, round.as_ref(), false).await?;
        let full = stats(state, &session, round.as_ref(), true).await?;
        state.events.emit("stats", Audience::Students, public);
        state.events.emit("stats", Audience::Instructors, full);
        state.events.emit("scores", Audience::All, json!({ "scores": scores_value(state, session.id).await? }));
        Ok(())
    }
    .await;
    if let Err(e) = result {
        tracing::error!("publish_live failed: {e}");
    }
}

pub async fn publish_round(state: &AppState) {
    let result: AppResult<()> = async {
        let session = db::current_session(&state.db).await?;
        let mut conn = state.db.acquire().await?;
        let round = engine::current_round(&mut conn, session.id).await?;
        drop(conn);
        if let Some(r) = round {
            let v = round_view(state, &r).await?;
            state.events.emit("round", Audience::All, json!({ "round": v, "now": now_ms() }));
        }
        Ok(())
    }
    .await;
    if let Err(e) = result {
        tracing::error!("publish_round failed: {e}");
    }
}

pub async fn publish_presence(state: &AppState, nim: &str, online: bool) {
    let result: AppResult<()> = async {
        let s = db::current_session(&state.db).await?;
        let team: Option<(i64,)> = sqlx::query_as(
            "SELECT tm.team_id FROM team_members tm JOIN teams t ON t.id = tm.team_id WHERE tm.session_id = ? AND tm.nim = ? AND t.confirmed = 1",
        )
        .bind(s.id)
        .bind(nim)
        .fetch_optional(&state.db)
        .await?;
        let joined: Vec<(String,)> = sqlx::query_as("SELECT nim FROM participants WHERE session_id = ? AND joined_at IS NOT NULL")
            .bind(s.id)
            .fetch_all(&state.db)
            .await?;
        let online_set: HashSet<String> = state.presence.online().into_iter().collect();
        let online_count = joined.iter().filter(|j| online_set.contains(&j.0)).count();
        state.events.emit(
            "presence",
            Audience::All,
            json!({ "nim": nim, "online": online, "team_id": team.map(|t| t.0), "online_count": online_count, "joined_count": joined.len() }),
        );
        Ok(())
    }
    .await;
    if let Err(e) = result {
        tracing::error!("publish_presence failed: {e}");
    }
}
