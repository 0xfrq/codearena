//! Explicit, structured game states. Every status has a stable backend code, a display label,
//! a visual tone (colour family) and an icon glyph so that colour is never the only cue.

use serde::Serialize;

/// Visual tone families used by the UI (see static/css/app.css `.tone-*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Idle,        // grey   — idle / waiting / not started
    Investigate, // amber  — investigation work
    Threat,      // red    — attack in progress / needs attention
    Analysis,    // cyan   — analysis / preparation work
    Review,      // purple — review & adjudication
    Success,     // green  — verified / accepted
    Fail,        // red outline — rejected / invalid
}

impl Tone {
    pub fn code(self) -> &'static str {
        match self {
            Tone::Idle => "idle",
            Tone::Investigate => "investigate",
            Tone::Threat => "threat",
            Tone::Analysis => "analysis",
            Tone::Review => "review",
            Tone::Success => "success",
            Tone::Fail => "fail",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Tone::Idle => "○",
            Tone::Investigate => "◎",
            Tone::Threat => "▲",
            Tone::Analysis => "◆",
            Tone::Review => "◷",
            Tone::Success => "✓",
            Tone::Fail => "✕",
        }
    }
}

macro_rules! status_enum {
    ($name:ident { $($variant:ident => ($code:literal, $label:literal, $tone:ident)),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];
            pub fn code(self) -> &'static str { match self { $($name::$variant => $code),+ } }
            pub fn label(self) -> &'static str { match self { $($name::$variant => $label),+ } }
            pub fn tone(self) -> Tone { match self { $($name::$variant => Tone::$tone),+ } }
            pub fn from_code(s: &str) -> Option<Self> {
                match s { $($code => Some($name::$variant),)+ _ => None }
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.code())
            }
        }
    };
}

status_enum!(Phase {
    Setup => ("SETUP", "Persiapan ronde", Idle),
    Briefing => ("BRIEFING", "Briefing", Analysis),
    DefensePrep => ("DEFENSE_PREP", "Defense Preparation", Analysis),
    AttackWindow => ("ATTACK_WINDOW", "Attack Window", Threat),
    DefenseResponse => ("DEFENSE_RESPONSE", "Defense Response", Investigate),
    Verification => ("VERIFICATION", "Verification", Review),
    Results => ("RESULTS", "Round Results", Success),
});

impl Phase {
    /// Official phase order shown on the round timeline (SETUP is not part of the timeline).
    pub const TIMELINE: [Phase; 6] = [
        Phase::Briefing,
        Phase::DefensePrep,
        Phase::AttackWindow,
        Phase::DefenseResponse,
        Phase::Verification,
        Phase::Results,
    ];

    pub fn next(self) -> Option<Phase> {
        match self {
            Phase::Setup => Some(Phase::Briefing),
            Phase::Briefing => Some(Phase::DefensePrep),
            Phase::DefensePrep => Some(Phase::AttackWindow),
            Phase::AttackWindow => Some(Phase::DefenseResponse),
            Phase::DefenseResponse => Some(Phase::Verification),
            Phase::Verification => Some(Phase::Results),
            Phase::Results => None,
        }
    }

    pub fn index(self) -> usize {
        Phase::TIMELINE.iter().position(|p| *p == self).map(|i| i + 1).unwrap_or(0)
    }

    /// Phases whose deadline is enforced and auto-advanced by the server timer.
    pub fn auto_advances(self) -> bool {
        matches!(self, Phase::DefensePrep | Phase::AttackWindow | Phase::DefenseResponse)
    }

    /// JSON key used in the round's planned-durations map.
    pub fn duration_key(self) -> &'static str {
        match self {
            Phase::Setup => "setup",
            Phase::Briefing => "briefing",
            Phase::DefensePrep => "defense_prep",
            Phase::AttackWindow => "attack_window",
            Phase::DefenseResponse => "defense_response",
            Phase::Verification => "verification",
            Phase::Results => "results",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Phase::Setup => "Instruktur menyiapkan challenge dan pairing.",
            Phase::Briefing => "Instruktur menjelaskan challenge dan aturan ronde.",
            Phase::DefensePrep => "Setiap tim memeriksa starter code dan menyiapkan implementasi pertahanan awal.",
            Phase::AttackWindow => "Snapshot pertahanan dikunci. Tim menyerang snapshot target dan mengirim attack report.",
            Phase::DefenseResponse => "Laporan dikunci. Defender menyelesaikan patch dan regression test.",
            Phase::Verification => "Instruktur memvalidasi laporan dan meninjau patch.",
            Phase::Results => "Skor ronde dihitung dan disahkan.",
        }
    }
}

status_enum!(AttackStatus {
    NotStarted => ("NOT_STARTED", "Belum dimulai", Idle),
    TargetAssigned => ("TARGET_ASSIGNED", "Target ditetapkan", Idle),
    InspectingTarget => ("INSPECTING_TARGET", "Menginspeksi target", Investigate),
    PreparingReport => ("PREPARING_REPORT", "Menyusun laporan", Investigate),
    ReportSubmitted => ("REPORT_SUBMITTED", "Laporan terkirim", Threat),
    AwaitingValidation => ("AWAITING_VALIDATION", "Menunggu validasi", Review),
    ValidFinding => ("VALID_FINDING", "Temuan valid", Success),
    InvalidFinding => ("INVALID_FINDING", "Temuan tidak valid", Fail),
    DuplicateFinding => ("DUPLICATE_FINDING", "Temuan duplikat", Fail),
    AttackCompleted => ("ATTACK_COMPLETED", "Attack selesai", Idle),
});

status_enum!(DefenseStatus {
    NotStarted => ("NOT_STARTED", "Belum dimulai", Idle),
    DefensePreparation => ("DEFENSE_PREPARATION", "Persiapan pertahanan", Analysis),
    AwaitingAttack => ("AWAITING_ATTACK", "Menunggu serangan", Idle),
    AttackReceived => ("ATTACK_RECEIVED", "Serangan diterima", Threat),
    AnalyzingBug => ("ANALYZING_BUG", "Menganalisis bug", Analysis),
    FixingBug => ("FIXING_BUG", "Memperbaiki bug", Investigate),
    WritingRegressionTest => ("WRITING_REGRESSION_TEST", "Menulis regression test", Analysis),
    PatchSubmitted => ("PATCH_SUBMITTED", "Patch terkirim", Review),
    AwaitingReview => ("AWAITING_REVIEW", "Menunggu review", Review),
    PatchAccepted => ("PATCH_ACCEPTED", "Patch diterima", Success),
    PatchRejected => ("PATCH_REJECTED", "Patch ditolak", Fail),
    DefenseCompleted => ("DEFENSE_COMPLETED", "Defense selesai", Idle),
});

status_enum!(ReportStatus {
    Draft => ("draft", "Draft", Idle),
    Submitted => ("submitted", "Submitted", Threat),
    PendingReview => ("pending_review", "Pending review", Review),
    Valid => ("valid", "Valid", Success),
    Invalid => ("invalid", "Invalid", Fail),
    Duplicate => ("duplicate", "Duplicate", Fail),
    NeedsRevision => ("needs_revision", "Needs revision", Investigate),
});

impl ReportStatus {
    /// Reports that are visible to the defender / counted as official submissions.
    pub fn is_official(self) -> bool {
        !matches!(self, ReportStatus::Draft)
    }
    pub fn awaiting_review(self) -> bool {
        matches!(self, ReportStatus::Submitted | ReportStatus::PendingReview)
    }
}

status_enum!(PatchStatus {
    Draft => ("draft", "Draft", Idle),
    Submitted => ("submitted", "Submitted — menunggu review", Review),
    Accepted => ("accepted", "Accepted", Success),
    Rejected => ("rejected", "Rejected", Fail),
});

// Per-report progress recorded explicitly by the defending team.
status_enum!(ReportDefenseState {
    Received => ("received", "Diterima", Threat),
    Analyzing => ("analyzing", "Dianalisis", Analysis),
    Fixing => ("fixing", "Sedang diperbaiki", Investigate),
    Testing => ("testing", "Menulis regression test", Analysis),
    Patched => ("patched", "Patch dikirim", Review),
    Disputed => ("disputed", "Dibantah (rebuttal)", Review),
});

/// Serializable status payload used in snapshots and events.
#[derive(Debug, Clone, Serialize)]
pub struct StatusView {
    pub code: String,
    pub label: String,
    pub tone: &'static str,
    pub icon: &'static str,
    pub reason: String,
    pub at: i64,
}

impl StatusView {
    pub fn attack(code: &str, reason: &str, at: i64) -> Self {
        let s = AttackStatus::from_code(code).unwrap_or(AttackStatus::NotStarted);
        StatusView {
            code: s.code().into(),
            label: s.label().into(),
            tone: s.tone().code(),
            icon: s.tone().icon(),
            reason: reason.into(),
            at,
        }
    }
    pub fn defense(code: &str, reason: &str, at: i64) -> Self {
        let s = DefenseStatus::from_code(code).unwrap_or(DefenseStatus::NotStarted);
        StatusView {
            code: s.code().into(),
            label: s.label().into(),
            tone: s.tone().code(),
            icon: s.tone().icon(),
            reason: reason.into(),
            at,
        }
    }
}

/// Default rubric (points) — see section 17 of the specification.
pub mod rubric {
    pub const ATTACK_REPRO: i64 = 10;
    pub const ATTACK_ROOTCAUSE: i64 = 10;
    pub const ATTACK_EVIDENCE: i64 = 5;
    pub const ATTACK_SCORING_FINDINGS: usize = 2;
    pub const DEFENSE_ROOTFIX: i64 = 20;
    pub const DEFENSE_REGRESSION: i64 = 15;
    pub const DEFENSE_EDGE: i64 = 10;
    pub const DEFENSE_EXPLAIN: i64 = 5;
    pub const DEFENSE_MAX: i64 = 50;
    pub const REBUTTAL_MAX: i64 = 10;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codes_roundtrip() {
        for s in AttackStatus::ALL {
            assert_eq!(AttackStatus::from_code(s.code()), Some(*s));
        }
        for s in DefenseStatus::ALL {
            assert_eq!(DefenseStatus::from_code(s.code()), Some(*s));
        }
        assert_eq!(AttackStatus::ALL.len(), 10);
        assert_eq!(DefenseStatus::ALL.len(), 12);
    }

    #[test]
    fn attack_and_defense_codes_are_distinct_types() {
        // Same code string can exist on both sides (NOT_STARTED) but they are separate fields/types.
        assert_eq!(AttackStatus::NotStarted.code(), DefenseStatus::NotStarted.code());
        assert!(DefenseStatus::from_code("REPORT_SUBMITTED").is_none());
        assert!(AttackStatus::from_code("FIXING_BUG").is_none());
    }

    #[test]
    fn phase_order() {
        let mut p = Phase::Setup;
        let mut seen = vec![];
        while let Some(n) = p.next() {
            seen.push(n);
            p = n;
        }
        assert_eq!(seen, Phase::TIMELINE.to_vec());
    }

    #[test]
    fn rubric_maxima_match_spec() {
        use rubric::*;
        assert_eq!(ATTACK_REPRO + ATTACK_ROOTCAUSE + ATTACK_EVIDENCE, 25);
        assert_eq!(DEFENSE_ROOTFIX + DEFENSE_REGRESSION + DEFENSE_EDGE + DEFENSE_EXPLAIN, DEFENSE_MAX);
    }
}
