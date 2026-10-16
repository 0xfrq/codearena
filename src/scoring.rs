//! Scoring. Scores are always *derived* from persisted review data (rubric fields on reports and
//! patches), finalized round rows and explicit adjustments — never incremented by events. This
//! makes recomputation idempotent: refreshing, reconnecting or replaying events cannot double count.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::models::rubric::*;

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct RoundScore {
    pub attack: i64,
    pub defense: i64,
    pub rebuttal: i64,
    pub total: i64,
    pub valid_findings: i64,
    pub accepted_patches: i64,
}

/// Points of one attack report from its rubric fields (each component clamped to its maximum).
pub fn report_points(repro: i64, rootcause: i64, evidence: i64) -> i64 {
    repro.clamp(0, ATTACK_REPRO) + rootcause.clamp(0, ATTACK_ROOTCAUSE) + evidence.clamp(0, ATTACK_EVIDENCE)
}

/// Attack score for a round: only the best `ATTACK_SCORING_FINDINGS` valid findings count.
pub fn attack_score(mut valid_points: Vec<i64>) -> i64 {
    valid_points.sort_unstable_by(|a, b| b.cmp(a));
    valid_points.into_iter().take(ATTACK_SCORING_FINDINGS).sum()
}

pub fn patch_points(rootfix: i64, regression: i64, edge: i64, explain: i64) -> i64 {
    rootfix.clamp(0, DEFENSE_ROOTFIX)
        + regression.clamp(0, DEFENSE_REGRESSION)
        + edge.clamp(0, DEFENSE_EDGE)
        + explain.clamp(0, DEFENSE_EXPLAIN)
}

/// Defense score: patch versions are cumulative, so the best reviewed patch of the round counts (max 50).
pub fn defense_score(reviewed_patch_points: &[i64]) -> i64 {
    reviewed_patch_points.iter().copied().max().unwrap_or(0).min(DEFENSE_MAX)
}

pub fn rebuttal_score(bonuses: &[i64]) -> i64 {
    bonuses.iter().map(|b| b.clamp(&0, &REBUTTAL_MAX)).sum::<i64>().min(REBUTTAL_MAX)
}

/// Live computation of a round's scores from review data.
pub async fn compute_round(pool: &SqlitePool, round_id: i64) -> AppResult<HashMap<i64, RoundScore>> {
    let teams: Vec<(i64,)> = sqlx::query_as("SELECT attacker_team_id FROM pairings WHERE round_id = ?")
        .bind(round_id)
        .fetch_all(pool)
        .await?;
    let mut out: HashMap<i64, RoundScore> = teams.into_iter().map(|(t,)| (t, RoundScore::default())).collect();

    let valid: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT attacker_team_id, score_repro, score_rootcause, score_evidence FROM attack_reports
         WHERE round_id = ? AND status = 'valid'",
    )
    .bind(round_id)
    .fetch_all(pool)
    .await?;
    let mut by_attacker: HashMap<i64, Vec<i64>> = HashMap::new();
    for (team, r, c, e) in valid {
        by_attacker.entry(team).or_default().push(report_points(r, c, e));
    }
    for (team, pts) in by_attacker {
        let e = out.entry(team).or_default();
        e.valid_findings = pts.len() as i64;
        e.attack = attack_score(pts);
    }

    let bonuses: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT target_team_id, rebuttal_bonus FROM attack_reports WHERE round_id = ? AND rebuttal_bonus > 0",
    )
    .bind(round_id)
    .fetch_all(pool)
    .await?;
    let mut by_target: HashMap<i64, Vec<i64>> = HashMap::new();
    for (team, b) in bonuses {
        by_target.entry(team).or_default().push(b);
    }
    for (team, b) in by_target {
        out.entry(team).or_default().rebuttal = rebuttal_score(&b);
    }

    let patches: Vec<(i64, String, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT team_id, status, score_rootfix, score_regression, score_edge, score_explain FROM patches
         WHERE round_id = ? AND status IN ('accepted', 'rejected')",
    )
    .bind(round_id)
    .fetch_all(pool)
    .await?;
    let mut by_team: HashMap<i64, Vec<i64>> = HashMap::new();
    for (team, status, a, b, c, d) in patches {
        by_team.entry(team).or_default().push(patch_points(a, b, c, d));
        if status == "accepted" {
            out.entry(team).or_default().accepted_patches += 1;
        }
    }
    for (team, pts) in by_team {
        out.entry(team).or_default().defense = defense_score(&pts);
    }
    for s in out.values_mut() {
        s.total = s.attack + s.defense + s.rebuttal;
    }
    Ok(out)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TeamScore {
    pub team_id: i64,
    /// Scores of the current (latest) round — live if not finalized, official if finalized.
    pub round: RoundScore,
    pub round_finalized: bool,
    /// Sum of finalized rounds + current live round + adjustments.
    pub cumulative: i64,
    pub adjustments: i64,
    pub valid_findings_total: i64,
    pub accepted_patches_total: i64,
    pub rank: usize,
}

/// Official/live scoreboard for every confirmed team of a session.
pub async fn session_scores(pool: &SqlitePool, session_id: i64) -> AppResult<Vec<TeamScore>> {
    let teams: Vec<(i64,)> = sqlx::query_as("SELECT id FROM teams WHERE session_id = ? AND confirmed = 1 ORDER BY id")
        .bind(session_id)
        .fetch_all(pool)
        .await?;
    let mut map: HashMap<i64, TeamScore> =
        teams.iter().map(|(id,)| (*id, TeamScore { team_id: *id, ..Default::default() })).collect();

    let finalized: Vec<(i64, i64, i64, i64, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT rs.round_id, rs.team_id, rs.attack, rs.defense, rs.rebuttal, rs.total, rs.valid_findings, rs.accepted_patches
         FROM round_scores rs JOIN rounds r ON r.id = rs.round_id WHERE r.session_id = ?",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let latest: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, state FROM rounds WHERE session_id = ? AND state IN ('running', 'finalized') ORDER BY number DESC LIMIT 1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    for (round_id, team, attack, defense, rebuttal, total, valid, accepted) in finalized {
        if let Some(e) = map.get_mut(&team) {
            e.cumulative += total;
            e.valid_findings_total += valid;
            e.accepted_patches_total += accepted;
            if latest.as_ref().is_some_and(|(id, _)| *id == round_id) {
                e.round = RoundScore { attack, defense, rebuttal, total, valid_findings: valid, accepted_patches: accepted };
                e.round_finalized = true;
            }
        }
    }
    if let Some((round_id, state)) = &latest {
        if state == "running" {
            for (team, s) in compute_round(pool, *round_id).await? {
                if let Some(e) = map.get_mut(&team) {
                    e.round = s;
                    e.cumulative += s.total;
                    e.valid_findings_total += s.valid_findings;
                    e.accepted_patches_total += s.accepted_patches;
                }
            }
        }
    }
    let adj: Vec<(i64, i64)> =
        sqlx::query_as("SELECT team_id, COALESCE(SUM(points), 0) FROM score_adjustments WHERE session_id = ? GROUP BY team_id")
            .bind(session_id)
            .fetch_all(pool)
            .await?;
    for (team, pts) in adj {
        if let Some(e) = map.get_mut(&team) {
            e.adjustments = pts;
            e.cumulative += pts;
        }
    }
    let mut list: Vec<TeamScore> = map.into_values().collect();
    list.sort_by(|a, b| b.cumulative.cmp(&a.cumulative).then(a.team_id.cmp(&b.team_id)));
    // Competition ranking: equal scores share a rank.
    let mut prev: Option<i64> = None;
    let mut rank = 0;
    for (i, s) in list.iter_mut().enumerate() {
        if prev != Some(s.cumulative) {
            rank = i + 1;
            prev = Some(s.cumulative);
        }
        s.rank = rank;
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_rubric_caps() {
        assert_eq!(report_points(10, 10, 5), 25);
        assert_eq!(report_points(99, 99, 99), 25);
        assert_eq!(report_points(-5, 3, 2), 5);
    }

    #[test]
    fn only_two_best_findings_score() {
        assert_eq!(attack_score(vec![10, 25, 20]), 45);
        assert_eq!(attack_score(vec![25, 25, 25]), 50);
        assert_eq!(attack_score(vec![]), 0);
    }

    #[test]
    fn defense_rubric() {
        assert_eq!(patch_points(20, 15, 10, 5), 50);
        assert_eq!(patch_points(30, 30, 30, 30), 50);
        assert_eq!(defense_score(&[30, 45, 12]), 45);
        assert_eq!(defense_score(&[]), 0);
    }

    #[test]
    fn rebuttal_capped_at_ten() {
        assert_eq!(rebuttal_score(&[6, 7]), 10);
        assert_eq!(rebuttal_score(&[4]), 4);
    }
}
