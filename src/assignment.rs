//! Waiting room and team assignment (manual + random). Draft (preview) teams are invisible to
//! students until the instructor confirms them. After the first round starts the session's teams
//! are locked: membership changes then require an explicit, reasoned instructor action.

use rand::seq::SliceRandom;
use serde::Serialize;
use serde_json::json;
use sqlx::SqliteConnection;

use crate::db::{self, current_session_conn};
use crate::error::{AppError, AppResult};
use crate::events::Outbox;
use crate::state::AppState;
use crate::util::{normalize_nim, now_ms};

pub const TEAM_NAMES: &[&str] = &[
    "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel", "India", "Juliett",
    "Kilo", "Lima", "Mike", "November", "Oscar", "Papa",
];

/// Balanced group sizes for `n` participants with a target size.
/// groups = max(2, ceil(n / target)) (at least two teams are needed to compete), sizes differ by ≤ 1.
/// 5→[3,2], 9→[3,3,3], 13→[4,3,3,3], 18→[4,4,4,3,3], 20→[4,4,4,4,4].
pub fn group_sizes(n: usize, target: usize) -> Vec<usize> {
    if n == 0 {
        return vec![];
    }
    let target = target.max(1);
    let groups = n.div_ceil(target).max(2).min(n);
    let base = n / groups;
    let extra = n % groups;
    (0..groups).map(|i| base + usize::from(i < extra)).collect()
}

/// Shuffle and split items into balanced groups (no item lost or duplicated).
pub fn distribute<T: Clone>(items: &[T], target: usize, rng: &mut impl rand::Rng) -> Vec<Vec<T>> {
    let mut pool = items.to_vec();
    pool.shuffle(rng);
    let mut out = Vec::new();
    let mut it = pool.into_iter();
    for size in group_sizes(items.len(), target) {
        out.push(it.by_ref().take(size).collect());
    }
    out
}

#[derive(Debug, Serialize)]
pub struct PreviewTeam {
    pub id: i64,
    pub name: String,
    pub members: Vec<String>,
}

async fn teams_locked(conn: &mut SqliteConnection) -> AppResult<(i64, bool)> {
    let s = current_session_conn(conn).await?;
    Ok((s.id, s.teams_locked == 1))
}

async fn active_round_exists(conn: &mut SqliteConnection, session_id: i64) -> AppResult<bool> {
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM rounds WHERE session_id = ? AND state IN ('setup', 'running')")
            .bind(session_id)
            .fetch_one(&mut *conn)
            .await?;
    Ok(n > 0)
}

async fn next_team_name(conn: &mut SqliteConnection, session_id: i64) -> AppResult<String> {
    let used: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM teams WHERE session_id = ?").bind(session_id).fetch_all(&mut *conn).await?;
    let used: std::collections::HashSet<String> = used.into_iter().map(|r| r.0).collect();
    for n in TEAM_NAMES {
        if !used.contains(*n) {
            return Ok((*n).to_string());
        }
    }
    Ok(format!("Team {}", used.len() + 1))
}

async fn history(
    conn: &mut SqliteConnection,
    session_id: i64,
    action: &str,
    team_id: Option<i64>,
    nim: Option<&str>,
    from_team: Option<i64>,
    detail: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO team_history (session_id, action, team_id, nim, from_team_id, detail, actor, created_at)
         VALUES (?, ?, ?, ?, ?, ?, 'instructor', ?)",
    )
    .bind(session_id)
    .bind(action)
    .bind(team_id)
    .bind(nim)
    .bind(from_team)
    .bind(detail)
    .bind(now_ms())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Snapshot eligible waiting participants, shuffle, split into balanced draft teams (preview).
/// Existing draft teams are discarded first; confirmed teams are never touched.
pub async fn random_preview(state: &AppState, target: usize, include_offline: bool) -> AppResult<Vec<PreviewTeam>> {
    if !(2..=8).contains(&target) {
        return Err(AppError::bad("Target ukuran kelompok harus 2–8."));
    }
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let (session_id, locked) = teams_locked(&mut tx).await?;
    if locked {
        return Err(AppError::conflict("Kelompok sudah dikunci karena pertandingan telah dimulai."));
    }
    discard_drafts(&mut tx, session_id).await?;
    let waiting: Vec<(String,)> = sqlx::query_as(
        "SELECT p.nim FROM participants p
         WHERE p.session_id = ? AND p.joined_at IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM team_members tm WHERE tm.session_id = p.session_id AND tm.nim = p.nim)
         ORDER BY p.nim",
    )
    .bind(session_id)
    .fetch_all(&mut *tx)
    .await?;
    let eligible: Vec<String> = waiting
        .into_iter()
        .map(|r| r.0)
        .filter(|nim| include_offline || state.presence.is_online(nim))
        .collect();
    if eligible.len() < 2 {
        return Err(AppError::conflict(format!(
            "Minimal 2 peserta eligible diperlukan (saat ini {}).",
            eligible.len()
        )));
    }
    let groups = distribute(&eligible, target, &mut rand::thread_rng());
    let now = now_ms();
    let mut preview = Vec::new();
    for members in groups {
        let name = next_team_name(&mut tx, session_id).await?;
        let id = sqlx::query("INSERT INTO teams (session_id, name, confirmed, created_at) VALUES (?, ?, 0, ?)")
            .bind(session_id)
            .bind(&name)
            .bind(now)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
        for nim in &members {
            sqlx::query("INSERT INTO team_members (session_id, team_id, nim, added_at) VALUES (?, ?, ?, ?)")
                .bind(session_id)
                .bind(id)
                .bind(nim)
                .bind(now)
                .execute(&mut *tx)
                .await?;
        }
        history(&mut tx, session_id, "random_preview", Some(id), None, None, &members.join(",")).await?;
        preview.push(PreviewTeam { id, name, members });
    }
    db::audit(
        &mut tx,
        Some(session_id),
        "instructor",
        "random_preview",
        "teams",
        "",
        json!({ "target": target, "snapshot": eligible, "teams": preview.len() }),
    )
    .await?;
    tx.commit().await?;
    state.events.emit("admin_teams", crate::events::Audience::Instructors, json!({ "reason": "random_preview" }));
    Ok(preview)
}

async fn discard_drafts(conn: &mut SqliteConnection, session_id: i64) -> AppResult<u64> {
    sqlx::query("DELETE FROM team_members WHERE team_id IN (SELECT id FROM teams WHERE session_id = ? AND confirmed = 0)")
        .bind(session_id)
        .execute(&mut *conn)
        .await?;
    let n = sqlx::query("DELETE FROM teams WHERE session_id = ? AND confirmed = 0")
        .bind(session_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(n)
}

pub async fn discard_preview(state: &AppState) -> AppResult<u64> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let s = current_session_conn(&mut tx).await?;
    let n = discard_drafts(&mut tx, s.id).await?;
    history(&mut tx, s.id, "discard_preview", None, None, None, &format!("{n} draft team(s)")).await?;
    tx.commit().await?;
    state.events.emit("admin_teams", crate::events::Audience::Instructors, json!({}));
    Ok(n)
}

pub async fn create_team(state: &AppState, name: &str) -> AppResult<i64> {
    let name = name.trim();
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let (session_id, locked) = teams_locked(&mut tx).await?;
    if locked && active_round_exists(&mut tx, session_id).await? {
        return Err(AppError::conflict("Tidak dapat membuat kelompok baru ketika ronde sedang berjalan."));
    }
    let name = if name.is_empty() { next_team_name(&mut tx, session_id).await? } else { name.chars().take(40).collect() };
    // A team created after teams are locked is confirmed immediately (explicit instructor action).
    let confirmed = i64::from(locked);
    let id = sqlx::query("INSERT INTO teams (session_id, name, confirmed, created_at) VALUES (?, ?, ?, ?)")
        .bind(session_id)
        .bind(&name)
        .bind(confirmed)
        .bind(now_ms())
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::conflict("Nama kelompok sudah dipakai."))?
        .last_insert_rowid();
    history(&mut tx, session_id, "created", Some(id), None, None, &name).await?;
    tx.commit().await?;
    state.events.emit("admin_teams", crate::events::Audience::Instructors, json!({}));
    Ok(id)
}

pub async fn rename_team(state: &AppState, team_id: i64, name: &str) -> AppResult<()> {
    let name: String = name.trim().chars().take(40).collect();
    if name.is_empty() {
        return Err(AppError::bad("Nama kelompok kosong."));
    }
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let (session_id, _) = teams_locked(&mut tx).await?;
    let old = db::team_name(&mut tx, team_id).await?;
    let n = sqlx::query("UPDATE teams SET name = ? WHERE id = ? AND session_id = ?")
        .bind(&name)
        .bind(team_id)
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::conflict("Nama kelompok sudah dipakai."))?
        .rows_affected();
    if n == 0 {
        return Err(AppError::not_found("Kelompok tidak ditemukan."));
    }
    history(&mut tx, session_id, "renamed", Some(team_id), None, None, &format!("{old} -> {name}")).await?;
    tx.commit().await?;
    let mut out = Outbox::default();
    out.structure("team_renamed");
    out.flush(&state.events);
    Ok(())
}

pub async fn delete_team(state: &AppState, team_id: i64) -> AppResult<()> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let (session_id, locked) = teams_locked(&mut tx).await?;
    let (used,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM pairings p JOIN rounds r ON r.id = p.round_id
         WHERE r.state <> 'cancelled' AND (p.attacker_team_id = ? OR p.target_team_id = ?)",
    )
    .bind(team_id)
    .bind(team_id)
    .fetch_one(&mut *tx)
    .await?;
    if used > 0 || (locked && active_round_exists(&mut tx, session_id).await?) {
        return Err(AppError::conflict("Kelompok sudah ikut bertanding dan tidak dapat dihapus."));
    }
    let members: Vec<(String,)> =
        sqlx::query_as("SELECT nim FROM team_members WHERE team_id = ?").bind(team_id).fetch_all(&mut *tx).await?;
    sqlx::query("DELETE FROM team_members WHERE team_id = ?").bind(team_id).execute(&mut *tx).await?;
    let n = sqlx::query("DELETE FROM teams WHERE id = ? AND session_id = ?")
        .bind(team_id)
        .bind(session_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::not_found("Kelompok tidak ditemukan."));
    }
    let list = members.into_iter().map(|m| m.0).collect::<Vec<_>>().join(",");
    history(&mut tx, session_id, "deleted", Some(team_id), None, None, &format!("members returned to waiting: {list}"))
        .await?;
    tx.commit().await?;
    let mut out = Outbox::default();
    out.structure("team_deleted");
    out.flush(&state.events);
    Ok(())
}

/// Add (or move) a roster NIM into a team. After teams are locked a reason is mandatory.
pub async fn set_member(state: &AppState, team_id: i64, raw_nim: &str, reason: &str) -> AppResult<()> {
    let nim = normalize_nim(raw_nim).ok_or_else(|| AppError::bad("Format NIM tidak valid."))?;
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let (session_id, locked) = teams_locked(&mut tx).await?;
    if locked && reason.trim().is_empty() {
        return Err(AppError::bad("Kelompok sudah dikunci: perubahan anggota wajib disertai alasan."));
    }
    let in_roster: Option<(i64,)> =
        sqlx::query_as("SELECT active FROM roster WHERE nim = ?").bind(&nim).fetch_optional(&mut *tx).await?;
    if in_roster.is_none() {
        return Err(AppError::bad(format!("NIM {nim} tidak ada di roster.")));
    }
    let team: Option<(i64,)> = sqlx::query_as("SELECT confirmed FROM teams WHERE id = ? AND session_id = ?")
        .bind(team_id)
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some((confirmed,)) = team else { return Err(AppError::not_found("Kelompok tidak ditemukan.")) };
    // Pre-register the participant (joined_at stays NULL until the student actually joins).
    sqlx::query("INSERT INTO participants (session_id, nim) VALUES (?, ?) ON CONFLICT(session_id, nim) DO NOTHING")
        .bind(session_id)
        .bind(&nim)
        .execute(&mut *tx)
        .await?;
    let current: Option<(i64,)> = sqlx::query_as("SELECT team_id FROM team_members WHERE session_id = ? AND nim = ?")
        .bind(session_id)
        .bind(&nim)
        .fetch_optional(&mut *tx)
        .await?;
    match current {
        Some((from,)) if from == team_id => return Ok(()),
        Some((from,)) => {
            sqlx::query("UPDATE team_members SET team_id = ?, added_at = ? WHERE session_id = ? AND nim = ?")
                .bind(team_id)
                .bind(now_ms())
                .bind(session_id)
                .bind(&nim)
                .execute(&mut *tx)
                .await?;
            history(&mut tx, session_id, "member_moved", Some(team_id), Some(&nim), Some(from), reason).await?;
        }
        None => {
            sqlx::query("INSERT INTO team_members (session_id, team_id, nim, added_at) VALUES (?, ?, ?, ?)")
                .bind(session_id)
                .bind(team_id)
                .bind(&nim)
                .bind(now_ms())
                .execute(&mut *tx)
                .await?;
            history(&mut tx, session_id, "member_added", Some(team_id), Some(&nim), None, reason).await?;
        }
    }
    if locked {
        db::audit(&mut tx, Some(session_id), "instructor", "locked_member_change", "team", team_id, json!({ "nim": nim, "reason": reason }))
            .await?;
    }
    tx.commit().await?;
    let mut out = Outbox::default();
    if confirmed == 1 {
        out.structure("member_changed");
    } else {
        out.push("admin_teams", crate::events::Audience::Instructors, json!({}));
    }
    out.flush(&state.events);
    Ok(())
}

pub async fn remove_member(state: &AppState, raw_nim: &str, reason: &str) -> AppResult<()> {
    let nim = normalize_nim(raw_nim).ok_or_else(|| AppError::bad("Format NIM tidak valid."))?;
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let (session_id, locked) = teams_locked(&mut tx).await?;
    if locked && reason.trim().is_empty() {
        return Err(AppError::bad("Kelompok sudah dikunci: perubahan anggota wajib disertai alasan."));
    }
    let current: Option<(i64,)> = sqlx::query_as("SELECT team_id FROM team_members WHERE session_id = ? AND nim = ?")
        .bind(session_id)
        .bind(&nim)
        .fetch_optional(&mut *tx)
        .await?;
    let Some((from,)) = current else { return Err(AppError::not_found("Mahasiswa belum berada di kelompok mana pun.")) };
    sqlx::query("DELETE FROM team_members WHERE session_id = ? AND nim = ?").bind(session_id).bind(&nim).execute(&mut *tx).await?;
    history(&mut tx, session_id, "member_removed", None, Some(&nim), Some(from), reason).await?;
    tx.commit().await?;
    let mut out = Outbox::default();
    out.structure("member_removed");
    out.flush(&state.events);
    Ok(())
}

/// Confirm all draft teams: they become visible to students. Empty drafts are rejected.
pub async fn confirm(state: &AppState) -> AppResult<usize> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let s = current_session_conn(&mut tx).await?;
    let drafts: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT t.id, t.name, (SELECT COUNT(*) FROM team_members tm WHERE tm.team_id = t.id)
         FROM teams t WHERE t.session_id = ? AND t.confirmed = 0",
    )
    .bind(s.id)
    .fetch_all(&mut *tx)
    .await?;
    if drafts.is_empty() {
        return Err(AppError::conflict("Tidak ada kelompok draft untuk dikonfirmasi."));
    }
    if let Some((_, name, _)) = drafts.iter().find(|d| d.2 == 0) {
        return Err(AppError::conflict(format!("Kelompok {name} masih kosong.")));
    }
    sqlx::query("UPDATE teams SET confirmed = 1 WHERE session_id = ? AND confirmed = 0").bind(s.id).execute(&mut *tx).await?;
    for (id, name, n) in &drafts {
        history(&mut tx, s.id, "confirmed", Some(*id), None, None, &format!("{name} ({n} anggota)")).await?;
    }
    db::audit(&mut tx, Some(s.id), "instructor", "confirm_teams", "teams", "", json!({ "teams": drafts.len() })).await?;
    tx.commit().await?;
    let mut out = Outbox::default();
    out.structure("teams_assigned");
    out.flush(&state.events);
    Ok(drafts.len())
}

/// Place waiting (joined, unassigned) participants into the currently smallest confirmed teams.
pub async fn place_late_arrivals(state: &AppState, reason: &str) -> AppResult<Vec<(String, String)>> {
    let _w = state.write.lock().await;
    let mut tx = state.db.begin().await?;
    let (session_id, locked) = teams_locked(&mut tx).await?;
    if locked && reason.trim().is_empty() {
        return Err(AppError::bad("Kelompok sudah dikunci: penempatan wajib disertai alasan."));
    }
    let waiting: Vec<(String,)> = sqlx::query_as(
        "SELECT p.nim FROM participants p WHERE p.session_id = ? AND p.joined_at IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM team_members tm WHERE tm.session_id = p.session_id AND tm.nim = p.nim)
         ORDER BY p.joined_at",
    )
    .bind(session_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut placed = Vec::new();
    for (nim,) in waiting {
        let smallest: Option<(i64, String)> = sqlx::query_as(
            "SELECT t.id, t.name FROM teams t WHERE t.session_id = ? AND t.confirmed = 1
             ORDER BY (SELECT COUNT(*) FROM team_members tm WHERE tm.team_id = t.id), t.id LIMIT 1",
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((team_id, name)) = smallest else {
            return Err(AppError::conflict("Belum ada kelompok terkonfirmasi."));
        };
        sqlx::query("INSERT INTO team_members (session_id, team_id, nim, added_at) VALUES (?, ?, ?, ?)")
            .bind(session_id)
            .bind(team_id)
            .bind(&nim)
            .bind(now_ms())
            .execute(&mut *tx)
            .await?;
        history(&mut tx, session_id, "late_placed", Some(team_id), Some(&nim), None, reason).await?;
        placed.push((nim, name));
    }
    tx.commit().await?;
    if !placed.is_empty() {
        let mut out = Outbox::default();
        out.structure("late_arrivals_placed");
        out.flush(&state.events);
    }
    Ok(placed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn distribution_table_from_spec() {
        assert_eq!(group_sizes(5, 4), vec![3, 2]);
        assert_eq!(group_sizes(9, 4), vec![3, 3, 3]);
        assert_eq!(group_sizes(13, 4), vec![4, 3, 3, 3]);
        assert_eq!(group_sizes(18, 4), vec![4, 4, 4, 3, 3]);
        assert_eq!(group_sizes(20, 4), vec![4, 4, 4, 4, 4]);
    }

    #[test]
    fn small_counts_still_make_two_teams() {
        assert_eq!(group_sizes(2, 4), vec![1, 1]);
        assert_eq!(group_sizes(4, 4), vec![2, 2]);
        assert_eq!(group_sizes(1, 4), vec![1]);
        assert!(group_sizes(0, 4).is_empty());
    }

    #[test]
    fn sizes_are_balanced_for_all_counts() {
        for n in 2..=60 {
            for target in 2..=6 {
                let s = group_sizes(n, target);
                assert_eq!(s.iter().sum::<usize>(), n);
                assert!(s.iter().max().unwrap() - s.iter().min().unwrap() <= 1, "n={n} t={target} {s:?}");
                assert!(s.len() >= 2);
            }
        }
    }

    #[test]
    fn distribute_never_loses_or_duplicates() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let people: Vec<String> = (0..17).map(|i| format!("2503201{i:04}")).collect();
        let groups = distribute(&people, 4, &mut rng);
        let mut flat: Vec<String> = groups.concat();
        flat.sort();
        let mut expected = people.clone();
        expected.sort();
        assert_eq!(flat, expected);
        assert_eq!(groups.iter().map(|g| g.len()).collect::<Vec<_>>(), vec![4, 4, 3, 3, 3]);
    }
}
