-- Code Arena schema. All timestamps are unix epoch milliseconds (INTEGER).

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Roster of students allowed to join (NIM stored as TEXT).
CREATE TABLE roster (
    nim      TEXT PRIMARY KEY,
    name     TEXT NOT NULL DEFAULT '',
    active   INTEGER NOT NULL DEFAULT 1,
    added_at INTEGER NOT NULL
);

-- A game session. Exactly one session has status <> 'archived' at a time.
CREATE TABLE game_sessions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'lobby',     -- lobby | running | archived
    teams_locked INTEGER NOT NULL DEFAULT 0,
    lobby_locked INTEGER NOT NULL DEFAULT 0,        -- cross-team chat closed by instructor
    created_at   INTEGER NOT NULL,
    archived_at  INTEGER
);

-- Login sessions (token stored as sha256 hash only).
CREATE TABLE auth_sessions (
    token_hash      TEXT PRIMARY KEY,
    role            TEXT NOT NULL,                  -- student | instructor
    nim             TEXT,
    game_session_id INTEGER,
    created_at      INTEGER NOT NULL,
    expires_at      INTEGER NOT NULL
);

CREATE TABLE participants (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  INTEGER NOT NULL REFERENCES game_sessions(id),
    nim         TEXT NOT NULL,
    joined_at   INTEGER,                            -- NULL = pre-assigned by instructor, not yet joined
    last_seen   INTEGER,
    muted_until INTEGER,
    UNIQUE (session_id, nim)
);

CREATE TABLE teams (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL REFERENCES game_sessions(id),
    name       TEXT NOT NULL,
    confirmed  INTEGER NOT NULL DEFAULT 0,          -- 0 = draft/preview, 1 = confirmed (visible to students)
    created_at INTEGER NOT NULL,
    UNIQUE (session_id, name)
);

-- PRIMARY KEY (session_id, nim) guarantees a student is in at most one team per session.
CREATE TABLE team_members (
    session_id INTEGER NOT NULL,
    team_id    INTEGER NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    nim        TEXT NOT NULL,
    added_at   INTEGER NOT NULL,
    PRIMARY KEY (session_id, nim)
);

CREATE TABLE team_history (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id   INTEGER NOT NULL,
    action       TEXT NOT NULL,
    team_id      INTEGER,
    nim          TEXT,
    from_team_id INTEGER,
    detail       TEXT NOT NULL DEFAULT '',
    actor        TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);

CREATE TABLE challenges (
    id               TEXT PRIMARY KEY,
    title            TEXT NOT NULL,
    materi           TEXT NOT NULL,                 -- JSON array
    topics           TEXT NOT NULL,                 -- JSON array
    meta_json        TEXT NOT NULL,
    starter_code     TEXT NOT NULL,
    reference_code   TEXT NOT NULL,
    regression_tests TEXT NOT NULL,
    is_capstone      INTEGER NOT NULL DEFAULT 0,
    updated_at       INTEGER NOT NULL
);

CREATE TABLE rounds (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id       INTEGER NOT NULL REFERENCES game_sessions(id),
    number           INTEGER NOT NULL,
    challenge_id     TEXT NOT NULL REFERENCES challenges(id),
    state            TEXT NOT NULL,                 -- setup | running | finalized | cancelled
    phase            TEXT NOT NULL,                 -- SETUP | BRIEFING | DEFENSE_PREP | ATTACK_WINDOW | DEFENSE_RESPONSE | VERIFICATION | RESULTS
    phase_started_at INTEGER,
    phase_deadline   INTEGER,                       -- NULL = untimed phase
    paused_at        INTEGER,                       -- non-NULL while paused
    durations_json   TEXT NOT NULL,                 -- planned seconds per phase
    auto_advance     INTEGER NOT NULL DEFAULT 1,
    discussion_open  INTEGER NOT NULL DEFAULT 0,
    created_at       INTEGER NOT NULL,
    started_at       INTEGER,
    finalized_at     INTEGER,
    UNIQUE (session_id, number)
);

CREATE TABLE round_phases (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    round_id     INTEGER NOT NULL REFERENCES rounds(id),
    phase        TEXT NOT NULL,
    started_at   INTEGER NOT NULL,
    ended_at     INTEGER,
    planned_secs INTEGER,
    paused_ms    INTEGER NOT NULL DEFAULT 0,
    UNIQUE (round_id, phase)
);

CREATE TABLE pairings (
    round_id         INTEGER NOT NULL REFERENCES rounds(id),
    attacker_team_id INTEGER NOT NULL,
    target_team_id   INTEGER NOT NULL,
    locked           INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (round_id, attacker_team_id),
    UNIQUE (round_id, target_team_id),
    CHECK (attacker_team_id <> target_team_id)
);

CREATE TRIGGER pairings_locked_no_update BEFORE UPDATE ON pairings
WHEN OLD.locked = 1
BEGIN
    SELECT RAISE(ABORT, 'pairing is locked');
END;

CREATE TRIGGER pairings_locked_no_delete BEFORE DELETE ON pairings
WHEN OLD.locked = 1
BEGIN
    SELECT RAISE(ABORT, 'pairing is locked');
END;

-- Defense implementation being prepared before the attack window (becomes the locked snapshot).
CREATE TABLE prep_drafts (
    round_id   INTEGER NOT NULL,
    team_id    INTEGER NOT NULL,
    code       TEXT NOT NULL,
    notes      TEXT NOT NULL DEFAULT '',
    saves      INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    updated_by TEXT NOT NULL,
    ready_at   INTEGER,
    PRIMARY KEY (round_id, team_id)
);

-- Locked defense snapshot: what attackers test. Immutable once written.
CREATE TABLE defense_snapshots (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    round_id    INTEGER NOT NULL,
    team_id     INTEGER NOT NULL,
    version     INTEGER NOT NULL DEFAULT 1,
    code        TEXT NOT NULL,
    code_sha256 TEXT NOT NULL,
    source      TEXT NOT NULL,                      -- team_draft | starter_default
    locked_at   INTEGER NOT NULL,
    UNIQUE (round_id, team_id)
);

CREATE TRIGGER defense_snapshots_immutable BEFORE UPDATE ON defense_snapshots
BEGIN
    SELECT RAISE(ABORT, 'defense snapshot is locked and immutable');
END;

CREATE TRIGGER defense_snapshots_no_delete BEFORE DELETE ON defense_snapshots
BEGIN
    SELECT RAISE(ABORT, 'defense snapshot is locked and immutable');
END;

CREATE TABLE attack_reports (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    round_id         INTEGER NOT NULL,
    attacker_team_id INTEGER NOT NULL,
    target_team_id   INTEGER NOT NULL,
    snapshot_id      INTEGER NOT NULL REFERENCES defense_snapshots(id),
    status           TEXT NOT NULL,                 -- draft | submitted | pending_review | valid | invalid | duplicate | needs_revision
    title            TEXT NOT NULL DEFAULT '',
    code_location    TEXT NOT NULL DEFAULT '',
    trigger_input    TEXT NOT NULL DEFAULT '',
    expected         TEXT NOT NULL DEFAULT '',
    actual           TEXT NOT NULL DEFAULT '',
    repro_steps      TEXT NOT NULL DEFAULT '',
    root_cause       TEXT NOT NULL DEFAULT '',
    concept          TEXT NOT NULL DEFAULT '',
    evidence         TEXT NOT NULL DEFAULT '',
    revision         INTEGER NOT NULL DEFAULT 0,
    created_by       TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    submitted_at     INTEGER,
    submitted_by     TEXT,
    reviewer         TEXT,
    reviewed_at      INTEGER,
    review_note      TEXT NOT NULL DEFAULT '',
    score_repro      INTEGER NOT NULL DEFAULT 0,
    score_rootcause  INTEGER NOT NULL DEFAULT 0,
    score_evidence   INTEGER NOT NULL DEFAULT 0,
    duplicate_of     INTEGER,
    rebuttal         TEXT NOT NULL DEFAULT '',
    rebuttal_at      INTEGER,
    rebuttal_by      TEXT,
    rebuttal_bonus   INTEGER NOT NULL DEFAULT 0,
    defense_state    TEXT NOT NULL DEFAULT 'received',  -- defender's per-report progress
    defense_analysis TEXT NOT NULL DEFAULT '',
    defense_updated_at INTEGER,
    CHECK (score_repro BETWEEN 0 AND 10),
    CHECK (score_rootcause BETWEEN 0 AND 10),
    CHECK (score_evidence BETWEEN 0 AND 5),
    CHECK (rebuttal_bonus BETWEEN 0 AND 10)
);

-- Submitted reports can never be deleted (only drafts by their authors).
CREATE TRIGGER attack_reports_no_delete BEFORE DELETE ON attack_reports
WHEN OLD.status <> 'draft'
BEGIN
    SELECT RAISE(ABORT, 'submitted attack reports cannot be deleted');
END;

-- Every content version of a report after submission is preserved here.
CREATE TABLE report_revisions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    report_id    INTEGER NOT NULL REFERENCES attack_reports(id),
    revision     INTEGER NOT NULL,
    content_json TEXT NOT NULL,
    reason       TEXT NOT NULL,
    created_by   TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);

CREATE TABLE patches (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    round_id          INTEGER NOT NULL,
    team_id           INTEGER NOT NULL,
    version           INTEGER NOT NULL,             -- >= 2 (v1 is the locked snapshot)
    status            TEXT NOT NULL,                -- draft | submitted | accepted | rejected
    code              TEXT NOT NULL,
    root_cause        TEXT NOT NULL DEFAULT '',
    regression_tests  TEXT NOT NULL DEFAULT '',
    explanation       TEXT NOT NULL DEFAULT '',
    addressed_reports TEXT NOT NULL DEFAULT '[]',   -- JSON array of report ids
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    updated_by        TEXT NOT NULL,
    submitted_at      INTEGER,
    submitted_by      TEXT,
    reviewer          TEXT,
    reviewed_at       INTEGER,
    review_note       TEXT NOT NULL DEFAULT '',
    score_rootfix     INTEGER NOT NULL DEFAULT 0,
    score_regression  INTEGER NOT NULL DEFAULT 0,
    score_edge        INTEGER NOT NULL DEFAULT 0,
    score_explain     INTEGER NOT NULL DEFAULT 0,
    UNIQUE (round_id, team_id, version),
    CHECK (score_rootfix BETWEEN 0 AND 20),
    CHECK (score_regression BETWEEN 0 AND 15),
    CHECK (score_edge BETWEEN 0 AND 10),
    CHECK (score_explain BETWEEN 0 AND 5)
);

-- Submitted patch content is frozen.
CREATE TRIGGER patches_content_frozen BEFORE UPDATE OF code, root_cause, regression_tests, explanation, addressed_reports ON patches
WHEN OLD.status <> 'draft'
BEGIN
    SELECT RAISE(ABORT, 'submitted patch content is frozen');
END;

CREATE TRIGGER patches_no_delete BEFORE DELETE ON patches
WHEN OLD.status <> 'draft'
BEGIN
    SELECT RAISE(ABORT, 'submitted patches cannot be deleted');
END;

-- Current attack/defense status per team per round (explicit, event-driven).
CREATE TABLE team_status (
    round_id       INTEGER NOT NULL,
    team_id        INTEGER NOT NULL,
    attack_status  TEXT NOT NULL,
    attack_reason  TEXT NOT NULL,
    attack_at      INTEGER NOT NULL,
    defense_status TEXT NOT NULL,
    defense_reason TEXT NOT NULL,
    defense_at     INTEGER NOT NULL,
    PRIMARY KEY (round_id, team_id)
);

-- Activity timeline (status changes + real user actions).
CREATE TABLE activity (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL,
    round_id   INTEGER,
    team_id    INTEGER,
    kind       TEXT NOT NULL,
    side       TEXT,                                -- attack | defense | NULL
    status     TEXT,
    message    TEXT NOT NULL,
    actor      TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX activity_team_idx ON activity (session_id, team_id, id);

-- Finalized per-round results: immutable once written.
CREATE TABLE round_scores (
    round_id         INTEGER NOT NULL,
    team_id          INTEGER NOT NULL,
    attack           INTEGER NOT NULL,
    defense          INTEGER NOT NULL,
    rebuttal         INTEGER NOT NULL,
    total            INTEGER NOT NULL,
    valid_findings   INTEGER NOT NULL,
    accepted_patches INTEGER NOT NULL,
    finalized_at     INTEGER NOT NULL,
    PRIMARY KEY (round_id, team_id)
);

CREATE TRIGGER round_scores_immutable BEFORE UPDATE ON round_scores
BEGIN
    SELECT RAISE(ABORT, 'finalized round scores are immutable; use a score adjustment');
END;

CREATE TRIGGER round_scores_no_delete BEFORE DELETE ON round_scores
BEGIN
    SELECT RAISE(ABORT, 'finalized round scores are immutable; use a score adjustment');
END;

-- Explicit, reasoned admin corrections (the only way to change a finalized total).
CREATE TABLE score_adjustments (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL,
    round_id   INTEGER,
    team_id    INTEGER NOT NULL,
    points     INTEGER NOT NULL,
    reason     TEXT NOT NULL CHECK (length(trim(reason)) > 0),
    actor      TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE audit_log (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER,
    actor      TEXT NOT NULL,
    action     TEXT NOT NULL,
    entity     TEXT NOT NULL,
    entity_id  TEXT,
    detail     TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL
);
CREATE INDEX audit_session_idx ON audit_log (session_id, id);

CREATE TABLE chat_messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  INTEGER NOT NULL,
    room        TEXT NOT NULL,                      -- lobby | broadcast | team:<id>
    sender_kind TEXT NOT NULL,                      -- student | instructor
    sender_nim  TEXT,
    sender_label TEXT NOT NULL,
    body        TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    deleted     INTEGER NOT NULL DEFAULT 0,
    deleted_by  TEXT
);
CREATE INDEX chat_room_idx ON chat_messages (session_id, room, id);
