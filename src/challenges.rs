//! Challenge bank. Sources live in `challenges/Cxx/` and are embedded into the binary at compile
//! time, then upserted into the `challenges` table on startup.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::util::now_ms;

pub struct ChallengeSource {
    pub id: &'static str,
    pub meta: &'static str,
    pub starter: &'static str,
    pub reference: &'static str,
    pub tests: &'static str,
}

macro_rules! bank {
    ($($id:literal),+ $(,)?) => {
        &[$(ChallengeSource {
            id: $id,
            meta: include_str!(concat!("../challenges/", $id, "/meta.json")),
            starter: include_str!(concat!("../challenges/", $id, "/starter.py")),
            reference: include_str!(concat!("../challenges/", $id, "/reference.py")),
            tests: include_str!(concat!("../challenges/", $id, "/test_regression.py")),
        }),+]
    };
}

pub static BANK: &[ChallengeSource] =
    bank!["C01", "C02", "C03", "C04", "C05", "C06", "C07", "C08", "C09", "C10", "C11", "C12", "C13"];

/// Official sub-topics of the PBO syllabus (section 3). Every one must be covered by the bank.
pub const REQUIRED_TOPICS: &[(u8, &str)] = &[
    (1, "konsep class"),
    (1, "object/instance"),
    (1, "struktur class"),
    (1, "attribute"),
    (1, "instance attribute"),
    (1, "class attribute"),
    (1, "method"),
    (1, "instance method"),
    (1, "membuat object"),
    (1, "akses attribute & method"),
    (1, "pemodelan objek"),
    (2, "visibility property"),
    (2, "encapsulation"),
    (2, "public/protected/private"),
    (2, "name mangling"),
    (2, "pengendalian akses attribute"),
    (2, "validasi via property"),
    (3, "class method"),
    (3, "static method"),
    (3, "decorator"),
    (3, "custom decorator"),
    (3, "chaining decorator"),
    (3, "*args & **kwargs"),
    (4, "inheritance"),
    (4, "base class"),
    (4, "derived class"),
    (4, "overriding"),
    (4, "nested inheritance"),
    (4, "multiple inheritance"),
    (5, "abstract method"),
    (5, "data class"),
    (5, "attribute mutability"),
    (5, "duck typing"),
    (5, "structural typing"),
];

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BugInfo {
    pub id: String,
    pub title: String,
    pub location: String,
    pub concept: String,
    pub scenario: String,
    pub attack: String,
    pub fix: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RubricNotes {
    pub attack: String,
    pub defense: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConceptLink {
    pub topic: String,
    pub bug: String,
    pub objective: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChallengeMeta {
    pub id: String,
    pub title: String,
    pub materi: Vec<u8>,
    #[serde(default)]
    pub capstone: bool,
    pub case: String,
    pub topics: Vec<String>,
    pub summary: String,
    pub objectives: Vec<String>,
    pub spec: Vec<String>,
    pub bugs: Vec<BugInfo>,
    pub attack_procedure: Vec<String>,
    pub rubric_notes: RubricNotes,
    pub concept_map: Vec<ConceptLink>,
}

#[derive(Debug, Clone)]
pub struct Challenge {
    pub meta: ChallengeMeta,
    pub starter_code: String,
    pub reference_code: String,
    pub regression_tests: String,
}

impl Challenge {
    pub fn id(&self) -> &str {
        &self.meta.id
    }
    pub fn title(&self) -> &str {
        &self.meta.title
    }
    pub fn label(&self) -> String {
        format!("{} — {}", self.meta.id, self.meta.title)
    }
    pub fn materi_label(&self) -> String {
        self.meta.materi.iter().map(|m| format!("M{m}")).collect::<Vec<_>>().join(" · ")
    }
}

pub fn parse_meta(src: &str) -> Result<ChallengeMeta, serde_json::Error> {
    serde_json::from_str(src)
}

pub async fn seed(pool: &SqlitePool) -> AppResult<()> {
    let now = now_ms();
    for c in BANK {
        let meta = parse_meta(c.meta).map_err(|e| AppError::Internal(format!("challenge {} meta: {e}", c.id)))?;
        sqlx::query(
            "INSERT INTO challenges (id, title, materi, topics, meta_json, starter_code, reference_code, regression_tests, is_capstone, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET title=excluded.title, materi=excluded.materi, topics=excluded.topics,
               meta_json=excluded.meta_json, starter_code=excluded.starter_code, reference_code=excluded.reference_code,
               regression_tests=excluded.regression_tests, is_capstone=excluded.is_capstone, updated_at=excluded.updated_at",
        )
        .bind(c.id)
        .bind(&meta.title)
        .bind(serde_json::to_string(&meta.materi)?)
        .bind(serde_json::to_string(&meta.topics)?)
        .bind(c.meta)
        .bind(c.starter)
        .bind(c.reference)
        .bind(c.tests)
        .bind(meta.capstone as i64)
        .bind(now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn load(pool: &SqlitePool, id: &str) -> AppResult<Challenge> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT meta_json, starter_code, reference_code, regression_tests FROM challenges WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let (meta, starter_code, reference_code, regression_tests) =
        row.ok_or_else(|| AppError::not_found(format!("challenge {id} tidak ditemukan")))?;
    Ok(Challenge { meta: parse_meta(&meta)?, starter_code, reference_code, regression_tests })
}

pub async fn load_all(pool: &SqlitePool) -> AppResult<Vec<Challenge>> {
    let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM challenges ORDER BY id").fetch_all(pool).await?;
    let mut out = Vec::with_capacity(ids.len());
    for (id,) in ids {
        out.push(load(pool, &id).await?);
    }
    Ok(out)
}

/// Template shown to defenders for writing regression tests.
pub fn regression_template(challenge: &Challenge) -> String {
    format!(
        "import unittest\n\nfrom solution import *  # modul {} versi patch tim\n\n\nclass TestRegression(unittest.TestCase):\n    def test_bug_tidak_terulang(self):\n        # TODO: reproduksi input pemicu dari attack report, lalu assert perilaku yang benar\n        ...\n\n\nif __name__ == \"__main__\":\n    unittest.main()\n",
        challenge.meta.id
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn bank_has_12_materials_plus_capstone() {
        assert!(BANK.len() >= 13);
        let metas: Vec<_> = BANK.iter().map(|c| parse_meta(c.meta).expect("meta parses")).collect();
        assert_eq!(metas.iter().filter(|m| m.capstone).count(), 1, "exactly one capstone");
        assert!(metas.iter().filter(|m| !m.capstone).count() >= 12);
    }

    #[test]
    fn every_required_topic_is_covered() {
        let covered: HashSet<String> =
            BANK.iter().flat_map(|c| parse_meta(c.meta).unwrap().topics).collect();
        let missing: Vec<_> = REQUIRED_TOPICS.iter().filter(|(_, t)| !covered.contains(*t)).collect();
        assert!(missing.is_empty(), "missing topics: {missing:?}");
    }

    #[test]
    fn every_challenge_is_complete() {
        let known: HashSet<&str> = REQUIRED_TOPICS.iter().map(|(_, t)| *t).collect();
        for c in BANK {
            let m = parse_meta(c.meta).unwrap();
            assert_eq!(m.id, c.id);
            assert!(!m.objectives.is_empty(), "{} objectives", c.id);
            assert!(!m.spec.is_empty(), "{} spec", c.id);
            assert!(m.bugs.len() >= 2, "{} needs >= 2 bug scenarios", c.id);
            assert!(!m.attack_procedure.is_empty(), "{} attack procedure", c.id);
            assert!(!m.rubric_notes.attack.is_empty() && !m.rubric_notes.defense.is_empty());
            assert!(!c.starter.trim().is_empty() && !c.reference.trim().is_empty());
            assert_ne!(c.starter, c.reference, "{} starter must differ from reference", c.id);
            for t in &m.topics {
                assert!(known.contains(t.as_str()), "{} unknown topic {t}", c.id);
            }
            // Each bug must have at least one regression test named test_b<N>_...
            for b in &m.bugs {
                let tag = format!("def test_{}_", b.id.to_lowercase());
                assert!(c.tests.contains(&tag), "{} bug {} has no regression test", c.id, b.id);
            }
        }
    }

    #[test]
    fn capstone_spans_all_five_materi() {
        let cap = BANK.iter().map(|c| parse_meta(c.meta).unwrap()).find(|m| m.capstone).unwrap();
        for m in 1..=5u8 {
            assert!(cap.materi.contains(&m));
        }
        assert!(cap.bugs.len() >= 4);
    }
}
