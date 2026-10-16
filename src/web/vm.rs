//! View models: plain, display-ready structs consumed by Askama templates.

use std::collections::HashMap;

use serde_json::json;

use crate::challenges::Challenge;
use crate::engine::{PatchRow, ReportRow, SnapshotRow};
use crate::models::*;
use crate::util::{fmt_opt_time, fmt_time};

#[derive(Debug, Clone)]
pub struct ReportView {
    pub id: i64,
    pub status: String,
    pub status_label: String,
    pub tone: &'static str,
    pub icon: &'static str,
    pub title: String,
    pub code_location: String,
    pub trigger_input: String,
    pub expected: String,
    pub actual: String,
    pub repro_steps: String,
    pub root_cause: String,
    pub concept: String,
    pub evidence: String,
    pub attacker_id: i64,
    pub target_id: i64,
    pub attacker_name: String,
    pub target_name: String,
    pub created_by: String,
    pub created_at: String,
    pub submitted_at: String,
    pub submitted_by: String,
    pub revision: i64,
    pub reviewed: bool,
    pub reviewed_at: String,
    pub review_note: String,
    pub score_repro: i64,
    pub score_rootcause: i64,
    pub score_evidence: i64,
    pub points: i64,
    pub duplicate_of: String,
    pub rebuttal: String,
    pub rebuttal_at: String,
    pub rebuttal_by: String,
    pub rebuttal_bonus: i64,
    pub defense_state: String,
    pub defense_state_label: String,
    pub defense_tone: &'static str,
    pub defense_analysis: String,
    pub snapshot_label: String,
    pub possible_duplicate: String,
    pub is_draft: bool,
    pub needs_revision: bool,
    pub awaiting_review: bool,
    pub filter: &'static str,
    pub json: String,
}

impl ReportView {
    pub fn from_row(r: &ReportRow, names: &HashMap<i64, String>, snapshot_label: String, possible_duplicate: Option<i64>) -> Self {
        let st = r.status();
        let ds = ReportDefenseState::from_code(&r.defense_state).unwrap_or(ReportDefenseState::Received);
        let filter = match st {
            ReportStatus::Valid => "valid",
            ReportStatus::Invalid => "invalid",
            ReportStatus::Duplicate => "duplicate",
            ReportStatus::Draft => "draft",
            _ => "pending",
        };
        ReportView {
            id: r.id,
            status: st.code().into(),
            status_label: st.label().into(),
            tone: st.tone().code(),
            icon: st.tone().icon(),
            title: r.title.clone(),
            code_location: r.code_location.clone(),
            trigger_input: r.trigger_input.clone(),
            expected: r.expected.clone(),
            actual: r.actual.clone(),
            repro_steps: r.repro_steps.clone(),
            root_cause: r.root_cause.clone(),
            concept: r.concept.clone(),
            evidence: r.evidence.clone(),
            attacker_id: r.attacker_team_id,
            target_id: r.target_team_id,
            attacker_name: names.get(&r.attacker_team_id).cloned().unwrap_or_default(),
            target_name: names.get(&r.target_team_id).cloned().unwrap_or_default(),
            created_by: r.created_by.clone(),
            created_at: fmt_time(r.created_at),
            submitted_at: fmt_opt_time(r.submitted_at),
            submitted_by: r.submitted_by.clone().unwrap_or_default(),
            revision: r.revision,
            reviewed: r.reviewed_at.is_some(),
            reviewed_at: fmt_opt_time(r.reviewed_at),
            review_note: r.review_note.clone(),
            score_repro: r.score_repro,
            score_rootcause: r.score_rootcause,
            score_evidence: r.score_evidence,
            points: r.points(),
            duplicate_of: r.duplicate_of.map(|d| format!("#{d}")).unwrap_or_default(),
            rebuttal: r.rebuttal.clone(),
            rebuttal_at: fmt_opt_time(r.rebuttal_at),
            rebuttal_by: r.rebuttal_by.clone().unwrap_or_default(),
            rebuttal_bonus: r.rebuttal_bonus,
            defense_state: ds.code().into(),
            defense_state_label: ds.label().into(),
            defense_tone: ds.tone().code(),
            defense_analysis: r.defense_analysis.clone(),
            snapshot_label,
            possible_duplicate: possible_duplicate.map(|d| format!("#{d}")).unwrap_or_default(),
            is_draft: st == ReportStatus::Draft,
            needs_revision: st == ReportStatus::NeedsRevision,
            awaiting_review: st.awaiting_review(),
            filter,
            json: json!({
                "id": r.id, "title": r.title, "code_location": r.code_location, "trigger_input": r.trigger_input,
                "expected": r.expected, "actual": r.actual, "repro_steps": r.repro_steps, "root_cause": r.root_cause,
                "concept": r.concept, "evidence": r.evidence, "status": r.status, "defense_state": r.defense_state,
                "defense_analysis": r.defense_analysis, "rebuttal": r.rebuttal,
                "repro": r.score_repro, "rootcause": r.score_rootcause, "evidence_score": r.score_evidence,
                "review_note": r.review_note, "duplicate_of": r.duplicate_of, "rebuttal_bonus": r.rebuttal_bonus,
                "possible_duplicate": possible_duplicate,
            })
            .to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PatchView {
    pub id: i64,
    pub team_id: i64,
    pub team_name: String,
    pub version: i64,
    pub status: String,
    pub status_label: String,
    pub tone: &'static str,
    pub icon: &'static str,
    pub code: String,
    pub root_cause: String,
    pub regression_tests: String,
    pub explanation: String,
    pub addressed: String,
    pub tests: usize,
    pub updated_at: String,
    pub updated_by: String,
    pub submitted_at: String,
    pub submitted_by: String,
    pub reviewed: bool,
    pub reviewed_at: String,
    pub review_note: String,
    pub score_rootfix: i64,
    pub score_regression: i64,
    pub score_edge: i64,
    pub score_explain: i64,
    pub points: i64,
    pub is_draft: bool,
    pub awaiting_review: bool,
    pub json: String,
}

impl PatchView {
    pub fn from_row(p: &PatchRow, names: &HashMap<i64, String>) -> Self {
        let st = p.status();
        let addressed = p.addressed();
        PatchView {
            id: p.id,
            team_id: p.team_id,
            team_name: names.get(&p.team_id).cloned().unwrap_or_default(),
            version: p.version,
            status: st.code().into(),
            status_label: st.label().into(),
            tone: st.tone().code(),
            icon: st.tone().icon(),
            code: p.code.clone(),
            root_cause: p.root_cause.clone(),
            regression_tests: p.regression_tests.clone(),
            explanation: p.explanation.clone(),
            addressed: if addressed.is_empty() {
                "-".into()
            } else {
                addressed.iter().map(|a| format!("#{a}")).collect::<Vec<_>>().join(", ")
            },
            tests: p.test_count(),
            updated_at: fmt_time(p.updated_at),
            updated_by: p.updated_by.clone(),
            submitted_at: fmt_opt_time(p.submitted_at),
            submitted_by: p.submitted_by.clone().unwrap_or_default(),
            reviewed: p.reviewed_at.is_some(),
            reviewed_at: fmt_opt_time(p.reviewed_at),
            review_note: p.review_note.clone(),
            score_rootfix: p.score_rootfix,
            score_regression: p.score_regression,
            score_edge: p.score_edge,
            score_explain: p.score_explain,
            points: p.points(),
            is_draft: st == PatchStatus::Draft,
            awaiting_review: st == PatchStatus::Submitted,
            json: json!({
                "id": p.id, "version": p.version, "status": p.status, "rootfix": p.score_rootfix, "regression": p.score_regression,
                "edge": p.score_edge, "explain": p.score_explain, "review_note": p.review_note,
            })
            .to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotView {
    pub team_name: String,
    pub version: i64,
    pub sha: String,
    pub sha_short: String,
    pub source: String,
    pub locked_at: String,
    pub code: String,
}

impl SnapshotView {
    pub fn from_row(s: &SnapshotRow, team_name: &str) -> Self {
        SnapshotView {
            team_name: team_name.to_string(),
            version: s.version,
            sha: s.code_sha256.clone(),
            sha_short: s.code_sha256[..12].to_string(),
            source: if s.source == "team_draft" { "implementasi tim".into() } else { "starter code (tim tidak menyimpan draft)".into() },
            locked_at: fmt_time(s.locked_at),
            code: s.code.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BugView {
    pub id: String,
    pub title: String,
    pub location: String,
    pub concept: String,
    pub scenario: String,
    pub attack: String,
    pub fix: String,
}

#[derive(Debug, Clone)]
pub struct ChallengeView {
    pub id: String,
    pub title: String,
    pub label: String,
    pub materi: String,
    pub case: String,
    pub capstone: bool,
    pub summary: String,
    pub topics: Vec<String>,
    pub objectives: Vec<String>,
    pub spec: Vec<String>,
    pub attack_procedure: Vec<String>,
    pub rubric_attack: String,
    pub rubric_defense: String,
    pub concept_map: Vec<(String, String, String)>,
    pub starter_code: String,
    /// Answer key — only populated for instructors or after discussion is opened.
    pub answer_key: bool,
    pub bugs: Vec<BugView>,
    pub reference_code: String,
    pub regression_tests: String,
}

impl ChallengeView {
    pub fn new(c: &Challenge, answer_key: bool) -> Self {
        let m = &c.meta;
        ChallengeView {
            id: m.id.clone(),
            title: m.title.clone(),
            label: c.label(),
            materi: c.materi_label(),
            case: m.case.clone(),
            capstone: m.capstone,
            summary: m.summary.clone(),
            topics: m.topics.clone(),
            objectives: m.objectives.clone(),
            spec: m.spec.clone(),
            attack_procedure: m.attack_procedure.clone(),
            rubric_attack: m.rubric_notes.attack.clone(),
            rubric_defense: m.rubric_notes.defense.clone(),
            concept_map: m.concept_map.iter().map(|l| (l.topic.clone(), l.bug.clone(), l.objective.clone())).collect(),
            starter_code: c.starter_code.clone(),
            answer_key,
            bugs: if answer_key {
                m.bugs
                    .iter()
                    .map(|b| BugView {
                        id: b.id.clone(),
                        title: b.title.clone(),
                        location: b.location.clone(),
                        concept: b.concept.clone(),
                        scenario: b.scenario.clone(),
                        attack: b.attack.clone(),
                        fix: b.fix.clone(),
                    })
                    .collect()
            } else {
                vec![]
            },
            reference_code: if answer_key { c.reference_code.clone() } else { String::new() },
            regression_tests: if answer_key { c.regression_tests.clone() } else { String::new() },
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActivityView {
    pub at: String,
    pub kind: String,
    pub side: String,
    pub status: String,
    pub message: String,
    pub actor: String,
    pub tone: &'static str,
    pub icon: &'static str,
}

impl ActivityView {
    pub fn new(kind: String, side: Option<String>, status: Option<String>, message: String, actor: String, at: i64) -> Self {
        let tone = match (side.as_deref(), status.as_deref()) {
            (Some("attack"), Some(s)) => AttackStatus::from_code(s).map(|x| x.tone()).unwrap_or(Tone::Idle),
            (Some("defense"), Some(s)) => DefenseStatus::from_code(s).map(|x| x.tone()).unwrap_or(Tone::Idle),
            _ => match kind.as_str() {
                "report_submitted" | "report_received" => Tone::Threat,
                "rebuttal" | "rebuttal_bonus" => Tone::Review,
                "patch_draft" | "prep_saved" => Tone::Analysis,
                _ => Tone::Idle,
            },
        };
        ActivityView {
            at: fmt_time(at),
            kind,
            side: side.unwrap_or_default(),
            status: status.unwrap_or_default(),
            message,
            actor,
            tone: tone.code(),
            icon: tone.icon(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemberView {
    pub nim: String,
    pub online: bool,
    pub joined: bool,
}

#[derive(Debug, Clone)]
pub struct ScoreRowView {
    pub rank: usize,
    pub team_id: i64,
    pub team_name: String,
    pub attack: i64,
    pub defense: i64,
    pub rebuttal: i64,
    pub round_total: i64,
    pub adjustments: i64,
    pub cumulative: i64,
    pub valid_findings: i64,
    pub accepted_patches: i64,
    pub finalized: bool,
}
