//! End-to-end engine tests against a real (temporary) SQLite database.

use std::collections::HashSet;

use codearena::chat::{self, Room};
use codearena::engine::{self, Durations, PatchFields, PatchReview, ReportFields, ReportReview};
use codearena::events::{Audience, Viewer};
use codearena::models::Phase;
use codearena::state::{AppState, Config};
use codearena::{assignment, auth, challenges, db, scoring, views};

const NIMS: &[&str] = &[
    "25032014005", "25032014006", "25032014007", "25032014018", "25032014021", "25032014025",
    "25032014029", "25032014031", "25032014032", "25032014039", "25032014043", "25032014045",
    "25032014050", "25032014051", "25032014053", "25032014058", "25032014062", "25032014071",
    "25032014075", "25032014090",
];

fn config(name: &str) -> Config {
    let dir = std::env::temp_dir().join(format!("codearena-test-{name}-{}-{}", std::process::id(), codearena::util::now_ms()));
    Config {
        bind: "127.0.0.1:0".into(),
        data_dir: dir.clone(),
        db_path: dir.join("test.db"),
        admin_password_env: Some("test-password".into()),
        run_timer: false,
    }
}

async fn setup(name: &str) -> AppState {
    codearena::build_state(config(name)).await.expect("state")
}

async fn join_online(state: &AppState, nims: &[&str]) {
    for n in nims {
        auth::join(state, n, None).await.expect("join");
        state.presence.connect(n);
    }
}

async fn team_ids(state: &AppState) -> Vec<i64> {
    let s = db::current_session(&state.db).await.unwrap();
    let rows: Vec<(i64,)> = sqlx::query_as("SELECT id FROM teams WHERE session_id = ? AND confirmed = 1 ORDER BY id")
        .bind(s.id)
        .fetch_all(&state.db)
        .await
        .unwrap();
    rows.into_iter().map(|r| r.0).collect()
}

async fn statuses(state: &AppState, round_id: i64, team: i64) -> (String, String) {
    sqlx::query_as("SELECT attack_status, defense_status FROM team_status WHERE round_id = ? AND team_id = ?")
        .bind(round_id)
        .bind(team)
        .fetch_one(&state.db)
        .await
        .unwrap()
}

async fn phase_of(state: &AppState, round_id: i64) -> String {
    let (p,): (String,) = sqlx::query_as("SELECT phase FROM rounds WHERE id = ?").bind(round_id).fetch_one(&state.db).await.unwrap();
    p
}

fn report(title: &str, location: &str, trigger: &str) -> ReportFields {
    ReportFields {
        title: title.into(),
        code_location: location.into(),
        trigger_input: trigger.into(),
        expected: "ValueError / saldo tetap".into(),
        actual: "saldo berubah".into(),
        repro_steps: "jalankan potongan kode".into(),
        root_cause: "setter tanpa validasi membuka akses langsung ke atribut privat".into(),
        concept: "encapsulation".into(),
        evidence: "def test_x(self): ...".into(),
    }
}

/// Three manual teams of the given sizes, confirmed.
async fn manual_teams(state: &AppState, sizes: &[usize]) -> Vec<i64> {
    let mut idx = 0;
    for (i, size) in sizes.iter().enumerate() {
        let id = assignment::create_team(state, &format!("T{i}")).await.unwrap();
        for _ in 0..*size {
            assignment::set_member(state, id, NIMS[idx], "").await.unwrap();
            idx += 1;
        }
    }
    assignment::confirm(state).await.unwrap();
    team_ids(state).await
}

#[tokio::test]
async fn nim_validation_and_single_device() {
    let state = setup("nim").await;
    assert!(auth::join(&state, "99999999999", None).await.is_err(), "unknown NIM rejected");
    assert!(auth::join(&state, "abc;--", None).await.is_err(), "malformed NIM rejected");
    let first = auth::join(&state, NIMS[0], None).await.unwrap();
    assert!(first.token.is_some());
    state.presence.connect(NIMS[0]);
    // second device while the first is connected
    let second = auth::join(&state, NIMS[0], None).await;
    assert!(second.is_err(), "same NIM cannot be active on two devices");
    // reconnect after disconnect restores the same participant row (no duplicate)
    state.presence.disconnect(NIMS[0]);
    auth::join(&state, NIMS[0], None).await.unwrap();
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM participants WHERE nim = ?").bind(NIMS[0]).fetch_one(&state.db).await.unwrap();
    assert_eq!(n, 1);
    let (tokens,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM auth_sessions WHERE nim = ? AND role = 'student'").bind(NIMS[0]).fetch_one(&state.db).await.unwrap();
    assert_eq!(tokens, 1, "only one live login per NIM");
}

#[tokio::test]
async fn random_assignment_snapshot_late_arrivals_and_lock() {
    let state = setup("assign").await;
    join_online(&state, &NIMS[..13]).await;
    // joined but offline: not eligible by default
    auth::join(&state, NIMS[13], None).await.unwrap();
    let preview = assignment::random_preview(&state, 4, false).await.unwrap();
    let mut sizes: Vec<usize> = preview.iter().map(|t| t.members.len()).collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(sizes, vec![4, 3, 3, 3]);
    let all: Vec<&String> = preview.iter().flat_map(|t| t.members.iter()).collect();
    let unique: HashSet<&&String> = all.iter().collect();
    assert_eq!(all.len(), 13);
    assert_eq!(unique.len(), 13, "no duplicates");
    assert!(!all.iter().any(|n| n.as_str() == NIMS[13]), "offline participant not in snapshot");

    // draft teams are invisible to students until confirmed
    let s = db::current_session(&state.db).await.unwrap();
    assert!(auth::confirmed_team_of(&state.db, s.id, NIMS[0]).await.unwrap().is_none());

    // late arrival after snapshot keeps waiting
    join_online(&state, &[NIMS[14]]).await;
    assignment::confirm(&state).await.unwrap();
    assert!(auth::confirmed_team_of(&state.db, s.id, NIMS[14]).await.unwrap().is_none());
    assert!(auth::confirmed_team_of(&state.db, s.id, NIMS[0]).await.unwrap().is_some());

    // explicit placement of late arrivals into the smallest teams
    let placed = assignment::place_late_arrivals(&state, "").await.unwrap();
    assert_eq!(placed.len(), 2, "both waiting participants placed");
    let counts: Vec<(i64,)> = sqlx::query_as("SELECT COUNT(*) FROM team_members WHERE session_id = ? GROUP BY team_id")
        .bind(s.id)
        .fetch_all(&state.db)
        .await
        .unwrap();
    let total: i64 = counts.iter().map(|c| c.0).sum();
    assert_eq!(total, 15);
    assert!(counts.iter().all(|c| c.0 == 3 || c.0 == 4));

    // moving a member keeps exactly one membership row
    let teams = team_ids(&state).await;
    assignment::set_member(&state, teams[1], NIMS[0], "").await.unwrap();
    let (rows,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM team_members WHERE nim = ?").bind(NIMS[0]).fetch_one(&state.db).await.unwrap();
    assert_eq!(rows, 1);

    // after a round starts teams are locked: randomize refused, changes need a reason
    let round = engine::create_round(&state, "C01", Durations::default(), false).await.unwrap();
    engine::start_round(&state, round).await.unwrap();
    assert!(assignment::random_preview(&state, 4, true).await.is_err());
    assert!(assignment::set_member(&state, teams[0], NIMS[1], "").await.is_err(), "reason required after lock");
    assignment::set_member(&state, teams[0], NIMS[1], "laptop rusak, pindah meja").await.unwrap();
    let (hist,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM team_history WHERE action = 'member_moved' AND detail LIKE '%laptop%'")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(hist, 1, "locked change recorded with reason");
}

#[tokio::test]
async fn full_round_attack_defense_scoring_and_finalization() {
    let state = setup("round").await;
    join_online(&state, &NIMS[..9]).await;
    let teams = manual_teams(&state, &[3, 3, 3]).await;
    assert!(engine::create_round(&state, "NOPE", Durations::default(), false).await.is_err());
    let round_id = engine::create_round(&state, "C03", Durations::default(), false).await.unwrap();

    // pairing: every team attacks exactly one and is attacked by exactly one
    let mut conn = state.db.acquire().await.unwrap();
    let pairs = engine::pairings(&mut conn, round_id).await.unwrap();
    drop(conn);
    let simple: Vec<(i64, i64)> = pairs.iter().map(|p| (p.0, p.1)).collect();
    engine::validate_pairing(&teams, &simple).unwrap();

    // manual pairing override before start, then lock
    let manual = vec![(teams[0], teams[1]), (teams[1], teams[2]), (teams[2], teams[0])];
    assert!(engine::set_pairing(&state, round_id, vec![(teams[0], teams[0]), (teams[1], teams[2]), (teams[2], teams[1])]).await.is_err());
    engine::set_pairing(&state, round_id, manual.clone()).await.unwrap();
    engine::start_round(&state, round_id).await.unwrap();
    assert!(engine::set_pairing(&state, round_id, manual.clone()).await.is_err(), "pairing locked after start");
    let tamper = sqlx::query("UPDATE pairings SET target_team_id = ? WHERE round_id = ? AND attacker_team_id = ?")
        .bind(teams[2])
        .bind(round_id)
        .bind(teams[0])
        .execute(&state.db)
        .await;
    assert!(tamper.is_err(), "DB trigger forbids changing a locked pairing");

    let (a, d) = statuses(&state, round_id, teams[0]).await;
    assert_eq!((a.as_str(), d.as_str()), ("TARGET_ASSIGNED", "NOT_STARTED"), "attack/defense statuses are separate");

    // idempotent phase transitions
    assert_eq!(engine::advance_phase(&state, round_id, Phase::Briefing, "instructor").await.unwrap(), Phase::DefensePrep);
    assert!(engine::advance_phase(&state, round_id, Phase::Briefing, "instructor").await.is_err(), "double click cannot skip a phase");
    assert_eq!(statuses(&state, round_id, teams[1]).await.1, "DEFENSE_PREPARATION");

    // A attacks B, B attacks C, C attacks A. Team C hardens its code with the reference solution.
    let (ta, tb, tc) = (teams[0], teams[1], teams[2]);
    let c03 = challenges::load(&state.db, "C03").await.unwrap();
    engine::save_prep(&state, tc, NIMS[6], &c03.reference_code, "pakai property read-only").await.unwrap();
    assert!(engine::save_report(&state, ta, NIMS[0], None, &report("x", "y", "z")).await.is_err(), "no attack before attack window");

    engine::advance_phase(&state, round_id, Phase::DefensePrep, "instructor").await.unwrap();
    let mut conn = state.db.acquire().await.unwrap();
    let snap_b = engine::snapshot_of(&mut conn, round_id, tb).await.unwrap().unwrap();
    let snap_c = engine::snapshot_of(&mut conn, round_id, tc).await.unwrap().unwrap();
    drop(conn);
    assert_eq!(snap_b.source, "starter_default");
    assert_eq!(snap_b.code, c03.starter_code);
    assert_eq!(snap_c.source, "team_draft");
    assert_eq!(snap_c.code_sha256, codearena::util::sha256_hex(&c03.reference_code));
    assert!(sqlx::query("UPDATE defense_snapshots SET code = 'hacked' WHERE id = ?").bind(snap_b.id).execute(&state.db).await.is_err());
    assert!(sqlx::query("DELETE FROM defense_snapshots WHERE id = ?").bind(snap_b.id).execute(&state.db).await.is_err());
    assert_eq!(statuses(&state, round_id, tb).await.1, "AWAITING_ATTACK");

    // A attacks B (explicit status, draft, submit)
    let mut rx = state.events.subscribe();
    engine::set_explicit_status(&state, ta, NIMS[0], "attack", "INSPECTING_TARGET").await.unwrap();
    assert!(engine::set_explicit_status(&state, ta, NIMS[0], "attack", "VALID_FINDING").await.is_err(), "verdict statuses cannot be self-assigned");
    let secret_title = "SECRET-TITLE saldo dapat ditimpa langsung";
    let r1 = engine::save_report(&state, ta, NIMS[0], None, &report(secret_title, "Tabungan.saldo setter", "t.saldo = 10**9")).await.unwrap();
    assert_eq!(statuses(&state, round_id, ta).await.0, "PREPARING_REPORT");
    let mut incomplete = report("", "", "");
    incomplete.title = "judul saja".into();
    let r_bad = engine::save_report(&state, ta, NIMS[1], None, &incomplete).await.unwrap();
    assert!(engine::submit_report(&state, ta, NIMS[1], r_bad).await.is_err(), "incomplete report cannot be submitted");
    engine::delete_report_draft(&state, ta, r_bad).await.unwrap();
    engine::submit_report(&state, ta, NIMS[0], r1).await.unwrap();
    let (a_att, a_def) = statuses(&state, round_id, ta).await;
    let (b_att, b_def) = statuses(&state, round_id, tb).await;
    assert_eq!(a_att, "REPORT_SUBMITTED");
    assert_eq!(b_def, "ATTACK_RECEIVED");
    assert_eq!(b_att, "TARGET_ASSIGNED", "defender's attack status untouched");
    assert_ne!(a_def, "ATTACK_RECEIVED");
    assert!(engine::delete_report_draft(&state, ta, r1).await.is_err(), "submitted report cannot be deleted");
    assert!(sqlx::query("DELETE FROM attack_reports WHERE id = ?").bind(r1).execute(&state.db).await.is_err());
    assert!(engine::save_report(&state, ta, NIMS[0], Some(r1), &report("edit", "x", "y")).await.is_err(), "submitted report locked");

    // event privacy: the report title never travels on a broadcast-to-all event
    let mut saw_team_refresh_for_b = false;
    while let Ok(ev) = rx.try_recv() {
        if ev.audience == Audience::All || ev.audience == Audience::Students {
            assert!(!ev.data.to_string().contains("SECRET-TITLE"), "public event leaked report content: {}", ev.kind);
        }
        if ev.kind == "team_refresh" && ev.audience == Audience::Team(tb) {
            saw_team_refresh_for_b = true;
        }
    }
    assert!(saw_team_refresh_for_b, "defender notified");
    let public = views::arena_snapshot(&state, false).await.unwrap().to_string();
    assert!(!public.contains("SECRET-TITLE"));

    // identical resubmission is refused; a different finding is accepted; a near-duplicate is flagged
    let dup = engine::save_report(&state, ta, NIMS[0], None, &report("lagi", "Tabungan.saldo setter", "t.saldo = 10**9")).await.unwrap();
    assert!(engine::submit_report(&state, ta, NIMS[0], dup).await.is_err());
    engine::delete_report_draft(&state, ta, dup).await.unwrap();
    let r2 = engine::save_report(&state, ta, NIMS[1], None, &report("setor negatif", "Tabungan.setor", "t.setor(-5)")).await.unwrap();
    engine::submit_report(&state, ta, NIMS[1], r2).await.unwrap();
    let r3 = engine::save_report(&state, ta, NIMS[2], None, &report("saldo bisa diubah", "Tabungan.saldo setter", "t.saldo = -1")).await.unwrap();
    engine::submit_report(&state, ta, NIMS[2], r3).await.unwrap();
    let mut conn = state.db.acquire().await.unwrap();
    let r3_row = engine::report_by_id(&mut conn, r3).await.unwrap();
    assert_eq!(engine::duplicate_hint(&mut conn, &r3_row).await.unwrap(), Some(r1));
    drop(conn);

    // pause blocks gameplay writes
    engine::pause(&state, round_id).await.unwrap();
    assert!(engine::save_report(&state, ta, NIMS[0], None, &report("p", "q", "r")).await.is_err());
    engine::resume(&state, round_id).await.unwrap();

    // B defends: patch drafts never touch the locked snapshot
    engine::set_explicit_status(&state, tb, NIMS[3], "defense", "ANALYZING_BUG").await.unwrap();
    engine::update_report_defense(&state, tb, NIMS[3], r1, "analyzing", "setter membuka akses").await.unwrap();
    assert!(engine::update_report_defense(&state, ta, NIMS[0], r1, "analyzing", "bukan target").await.is_err(), "only the target team can respond");
    let mut fields = PatchFields { code: c03.reference_code.clone(), root_cause: "setter".into(), regression_tests: String::new(), explanation: String::new(), addressed: vec![r1, r2, 9999] };
    let v = engine::save_patch(&state, tb, NIMS[3], &fields).await.unwrap();
    assert_eq!(v, 2, "first patch after snapshot v1 is v2");
    assert_eq!(statuses(&state, round_id, tb).await.1, "FIXING_BUG");
    assert!(engine::submit_patch(&state, tb, NIMS[3]).await.is_err(), "regression test required");
    fields.regression_tests = c03.regression_tests.clone();
    engine::save_patch(&state, tb, NIMS[3], &fields).await.unwrap();
    assert_eq!(statuses(&state, round_id, tb).await.1, "WRITING_REGRESSION_TEST");
    let mut conn = state.db.acquire().await.unwrap();
    assert_eq!(engine::snapshot_of(&mut conn, round_id, tb).await.unwrap().unwrap().code, c03.starter_code, "snapshot unchanged by patch");
    drop(conn);
    engine::submit_rebuttal(&state, tb, NIMS[4], r2, "Pada snapshot kami setor() tetap memeriksa... (contoh rebuttal teknis)").await.unwrap();
    engine::submit_patch(&state, tb, NIMS[3]).await.unwrap();
    assert_eq!(statuses(&state, round_id, tb).await.1, "PATCH_SUBMITTED");
    let (addressed,): (String,) = sqlx::query_as("SELECT addressed_reports FROM patches WHERE team_id = ? AND version = 2").bind(tb).fetch_one(&state.db).await.unwrap();
    assert_eq!(addressed, format!("[{r1},{r2}]"), "foreign report ids are dropped");
    assert!(sqlx::query("UPDATE patches SET code = 'x' WHERE team_id = ? AND version = 2").bind(tb).execute(&state.db).await.is_err(), "submitted patch frozen");

    // attack window closes: reports locked & queued
    engine::advance_phase(&state, round_id, Phase::AttackWindow, "instructor").await.unwrap();
    assert_eq!(statuses(&state, round_id, ta).await.0, "AWAITING_VALIDATION");
    assert!(engine::save_report(&state, ta, NIMS[0], None, &report("late", "x", "y")).await.is_err());
    let (pending,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM attack_reports WHERE round_id = ? AND status = 'pending_review'")
        .bind(round_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(pending, 3);
    // teams without official reports are marked attack-completed
    assert_eq!(statuses(&state, round_id, tb).await.0, "ATTACK_COMPLETED");

    // instructor review (rubric)
    let rv = |id: i64, verdict: &str| ReportReview { report_id: id, verdict: verdict.into(), repro: 10, rootcause: 10, evidence: 5, note: String::new(), duplicate_of: None, rebuttal_bonus: 0 };
    engine::review_report(&state, &rv(r1, "valid")).await.unwrap();
    assert_eq!(statuses(&state, round_id, ta).await.0, "VALID_FINDING");
    let mut inv = rv(r2, "invalid");
    assert!(engine::review_report(&state, &inv).await.is_err(), "note required for invalid");
    inv.note = "setor() memang memvalidasi pada snapshot tim B".into();
    inv.rebuttal_bonus = 6;
    engine::review_report(&state, &inv).await.unwrap();
    let mut d3 = rv(r3, "duplicate");
    d3.note = "sama dengan #r1".into();
    assert!(engine::review_report(&state, &d3).await.is_err(), "duplicate needs original");
    d3.duplicate_of = Some(r1);
    engine::review_report(&state, &d3).await.unwrap();
    assert_eq!(statuses(&state, round_id, ta).await.0, "DUPLICATE_FINDING");

    engine::advance_phase(&state, round_id, Phase::DefenseResponse, "instructor").await.unwrap();
    assert_eq!(statuses(&state, round_id, tb).await.1, "AWAITING_REVIEW");
    let patch_id: (i64,) = sqlx::query_as("SELECT id FROM patches WHERE team_id = ? AND version = 2").bind(tb).fetch_one(&state.db).await.unwrap();
    let mut prv = PatchReview { patch_id: patch_id.0, verdict: "rejected".into(), rootfix: 20, regression: 15, edge: 10, explain: 5, note: String::new() };
    assert!(engine::review_patch(&state, &prv).await.is_err(), "rejection needs a reason");
    prv.verdict = "accepted".into();
    engine::review_patch(&state, &prv).await.unwrap();
    assert_eq!(statuses(&state, round_id, tb).await.1, "PATCH_ACCEPTED");

    // scores follow the rubric: duplicates and invalid reports earn nothing
    let live = scoring::compute_round(&state.db, round_id).await.unwrap();
    assert_eq!(live[&ta].attack, 25);
    assert_eq!(live[&ta].valid_findings, 1);
    assert_eq!(live[&tb].defense, 50);
    assert_eq!(live[&tb].rebuttal, 6);
    assert_eq!(live[&tb].total, 56);
    assert_eq!(live[&tc].total, 0);
    // recomputing (refresh / replay) never changes the result
    assert_eq!(scoring::compute_round(&state.db, round_id).await.unwrap(), live);

    engine::advance_phase(&state, round_id, Phase::Verification, "instructor").await.unwrap();
    assert!(engine::advance_phase(&state, round_id, Phase::Results, "instructor").await.is_err(), "no phase after results");
    let finals = engine::finalize_round(&state, round_id, false).await.unwrap();
    assert_eq!(finals[&tb].total, 56);
    assert!(engine::finalize_round(&state, round_id, false).await.is_err(), "finalize once");
    assert!(engine::review_report(&state, &rv(r1, "invalid")).await.is_err(), "final results immutable");
    assert!(sqlx::query("UPDATE round_scores SET total = 999").execute(&state.db).await.is_err());
    let (ra, rd) = statuses(&state, round_id, ta).await;
    assert_eq!((ra.as_str(), rd.as_str()), ("ATTACK_COMPLETED", "DEFENSE_COMPLETED"));

    let s = db::current_session(&state.db).await.unwrap();
    let board = scoring::session_scores(&state.db, s.id).await.unwrap();
    let b = board.iter().find(|x| x.team_id == tb).unwrap();
    assert_eq!((b.cumulative, b.rank, b.round_finalized), (56, 1, true));

    // explicit, reasoned correction is the only way to change totals
    assert!(engine::adjust_score(&state, ta, Some(round_id), 5, "").await.is_err());
    engine::adjust_score(&state, ta, Some(round_id), 5, "bonus presentasi pembahasan").await.unwrap();
    let board = scoring::session_scores(&state.db, s.id).await.unwrap();
    assert_eq!(board.iter().find(|x| x.team_id == ta).unwrap().cumulative, 30);
    let (audits,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE action IN ('score_adjusted', 'report_reviewed', 'patch_reviewed', 'round_finalized')")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert!(audits >= 6);

    // discussion opens the answer key to students
    engine::set_discussion(&state, round_id, true).await.unwrap();
}

#[tokio::test]
async fn timer_pause_and_restart_never_reset_a_round() {
    let cfg = config("timer");
    let state = codearena::build_state(cfg.clone()).await.unwrap();
    join_online(&state, &NIMS[..4]).await;
    manual_teams(&state, &[2, 2]).await;
    let d = Durations { briefing: 0, defense_prep: 60, attack_window: 60, defense_response: 60, verification: 0 };
    let round_id = engine::create_round(&state, "C11", d, true).await.unwrap();
    engine::start_round(&state, round_id).await.unwrap();
    engine::advance_phase(&state, round_id, Phase::Briefing, "instructor").await.unwrap();

    // not yet due: tick does nothing
    engine::tick(&state).await;
    assert_eq!(phase_of(&state, round_id).await, "DEFENSE_PREP");

    // deadline passes -> server advances exactly one phase
    sqlx::query("UPDATE rounds SET phase_deadline = ? WHERE id = ?").bind(codearena::util::now_ms() - 1).bind(round_id).execute(&state.db).await.unwrap();
    engine::tick(&state).await;
    assert_eq!(phase_of(&state, round_id).await, "ATTACK_WINDOW");
    engine::tick(&state).await;
    assert_eq!(phase_of(&state, round_id).await, "ATTACK_WINDOW", "new deadline is in the future");

    // paused rounds are frozen even past their deadline
    engine::pause(&state, round_id).await.unwrap();
    let (deadline_before,): (i64,) = sqlx::query_as("SELECT phase_deadline FROM rounds WHERE id = ?").bind(round_id).fetch_one(&state.db).await.unwrap();
    sqlx::query("UPDATE rounds SET paused_at = paused_at - 30000 WHERE id = ?").bind(round_id).execute(&state.db).await.unwrap();
    engine::tick(&state).await;
    assert_eq!(phase_of(&state, round_id).await, "ATTACK_WINDOW");
    engine::resume(&state, round_id).await.unwrap();
    let (deadline_after,): (i64,) = sqlx::query_as("SELECT phase_deadline FROM rounds WHERE id = ?").bind(round_id).fetch_one(&state.db).await.unwrap();
    assert!(deadline_after - deadline_before >= 30_000, "deadline shifted by paused time");

    // simulated server restart on the same database
    drop(state);
    let restarted = codearena::build_state(cfg).await.unwrap();
    assert_eq!(phase_of(&restarted, round_id).await, "ATTACK_WINDOW", "restart keeps the official phase");
    let (state_txt,): (String,) = sqlx::query_as("SELECT state FROM rounds WHERE id = ?").bind(round_id).fetch_one(&restarted.db).await.unwrap();
    assert_eq!(state_txt, "running");
    let (snaps,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM defense_snapshots WHERE round_id = ?").bind(round_id).fetch_one(&restarted.db).await.unwrap();
    assert_eq!(snaps, 2, "snapshots locked exactly once");
}

#[tokio::test]
async fn chat_rooms_are_authorised_and_rate_limited() {
    let state = setup("chat").await;
    join_online(&state, &NIMS[..4]).await;
    let teams = manual_teams(&state, &[2, 2]).await;
    let s = db::current_session(&state.db).await.unwrap();
    let alice_team = auth::confirmed_team_of(&state.db, s.id, NIMS[0]).await.unwrap().unwrap();
    let alice = Viewer::Student { nim: NIMS[0].into(), team_id: Some(alice_team) };
    let other_team = teams.iter().copied().find(|t| *t != alice_team).unwrap();

    let mut rx = state.events.subscribe();
    chat::send(&state, &alice, &Room::Team(alice_team), "strategi kita?").await.unwrap();
    let ev = rx.recv().await.unwrap();
    assert!(matches!(ev.audience, Audience::TeamAndInstructors(_)), "team chat is private");
    assert!(chat::send(&state, &alice, &Room::Team(other_team), "mengintip").await.is_err());
    assert!(chat::history(&state, &alice, &Room::Team(other_team)).await.is_err());
    assert!(chat::send(&state, &alice, &Room::Broadcast, "bukan instruktur").await.is_err());
    chat::send(&state, &Viewer::Instructor, &Room::Broadcast, "Attack window dibuka").await.unwrap();
    assert!(chat::send(&state, &alice, &Room::Lobby, &"x".repeat(501)).await.is_err(), "length limit");
    // rate limit: burst of messages is cut off
    let mut ok = 0;
    for i in 0..8 {
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        if chat::send(&state, &alice, &Room::Lobby, &format!("halo {i}")).await.is_ok() {
            ok += 1;
        }
    }
    assert!(ok < 8, "rate limiting kicks in");
    // moderation
    let msgs = chat::history(&state, &Viewer::Instructor, &Room::Lobby).await.unwrap();
    chat::delete(&state, msgs[0].id).await.unwrap();
    let seen = chat::history(&state, &alice, &Room::Lobby).await.unwrap();
    assert!(seen[0].body.contains("dihapus"));
    chat::set_lobby_locked(&state, true).await.unwrap();
    assert!(chat::send(&state, &Viewer::Student { nim: NIMS[1].into(), team_id: None }, &Room::Lobby, "boleh?").await.is_err());
}

#[tokio::test]
async fn public_snapshot_hides_unlocked_pairings() {
    let state = setup("visibility").await;
    join_online(&state, &NIMS[..4]).await;
    manual_teams(&state, &[2, 2]).await;
    let round_id = engine::create_round(&state, "C07", Durations::default(), false).await.unwrap();
    let public = views::arena_snapshot(&state, false).await.unwrap();
    assert_eq!(public["stats"]["edges"].as_array().unwrap().len(), 0, "students do not see proposed pairings");
    let full = views::arena_snapshot(&state, true).await.unwrap();
    assert_eq!(full["stats"]["edges"].as_array().unwrap().len(), 2);
    assert_eq!(full["stats"]["edges"][0]["proposed"], true);
    engine::start_round(&state, round_id).await.unwrap();
    let public = views::arena_snapshot(&state, false).await.unwrap();
    let edges = public["stats"]["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    assert!(edges.iter().all(|e| e["locked"] == true && e["proposed"] == false));
    // every team appears exactly once as attacker and once as target in the graph
    let attackers: HashSet<i64> = edges.iter().map(|e| e["attacker_id"].as_i64().unwrap()).collect();
    let targets: HashSet<i64> = edges.iter().map(|e| e["target_id"].as_i64().unwrap()).collect();
    assert_eq!(attackers.len(), 2);
    assert_eq!(attackers, targets);
}
