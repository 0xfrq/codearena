use std::str::FromStr;
use std::time::Duration;

use rand::distributions::{Alphanumeric, DistString};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{SqliteConnection, SqlitePool};

use crate::error::{AppError, AppResult};
use crate::state::Config;
use crate::util::{now_ms, sha256_hex};

pub const INITIAL_ROSTER: &[&str] = &[
    "25032014005", "25032014006", "25032014007", "25032014018", "25032014021",
    "25032014025", "25032014029", "25032014031", "25032014032", "25032014039",
    "25032014043", "25032014045", "25032014050", "25032014051", "25032014053",
    "25032014058", "25032014062", "25032014071", "25032014075", "25032014090",
];

pub async fn connect(path: &std::path::Path) -> AppResult<SqlitePool> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Internal(format!("data dir: {e}")))?;
        }
    }
    let url = format!("sqlite://{}", path.to_string_lossy().replace('\\', "/"));
    let opts = SqliteConnectOptions::from_str(&url)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10));
    let pool = SqlitePoolOptions::new().max_connections(8).connect_with(opts).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> AppResult<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| AppError::Internal(format!("migration: {e}")))
}

/// Idempotent startup seeding: roster, challenge bank, current session, instructor password.
pub async fn seed(pool: &SqlitePool, config: &Config) -> AppResult<()> {
    let now = now_ms();
    let (roster_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM roster").fetch_one(pool).await?;
    if roster_count == 0 {
        for nim in INITIAL_ROSTER {
            sqlx::query("INSERT INTO roster (nim, added_at) VALUES (?, ?)").bind(nim).bind(now).execute(pool).await?;
        }
    }
    crate::challenges::seed(pool).await?;
    let (sessions,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM game_sessions WHERE status <> 'archived'").fetch_one(pool).await?;
    if sessions == 0 {
        sqlx::query("INSERT INTO game_sessions (name, created_at) VALUES (?, ?)")
            .bind("Recap Praktikum PBO")
            .bind(now)
            .execute(pool)
            .await?;
    }
    if config.admin_password_env.is_none() && get_setting(pool, "admin_hash").await?.is_none() {
        let password = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);
        set_admin_password(pool, &password).await?;
        let file = config.data_dir.join("admin-password.txt");
        let _ = std::fs::create_dir_all(&config.data_dir);
        let _ = std::fs::write(&file, format!("{password}\n"));
        tracing::warn!(
            "instructor password generated and written to {} (set CODEARENA_ADMIN_PASSWORD to override)",
            file.display()
        );
    }
    Ok(())
}

pub async fn set_admin_password(pool: &SqlitePool, password: &str) -> AppResult<()> {
    let salt = crate::util::random_token();
    let hash = sha256_hex(&format!("{salt}:{password}"));
    set_setting(pool, "admin_salt", &salt).await?;
    set_setting(pool, "admin_hash", &hash).await?;
    Ok(())
}

pub async fn get_setting(pool: &SqlitePool, key: &str) -> AppResult<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM settings WHERE key = ?").bind(key).fetch_optional(pool).await?;
    Ok(row.map(|r| r.0))
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionRow {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub teams_locked: i64,
    pub lobby_locked: i64,
    pub created_at: i64,
}

pub async fn current_session(pool: &SqlitePool) -> AppResult<SessionRow> {
    sqlx::query_as::<_, SessionRow>(
        "SELECT id, name, status, teams_locked, lobby_locked, created_at FROM game_sessions
         WHERE status <> 'archived' ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::Internal("no active game session".into()))
}

pub async fn current_session_conn(conn: &mut SqliteConnection) -> AppResult<SessionRow> {
    sqlx::query_as::<_, SessionRow>(
        "SELECT id, name, status, teams_locked, lobby_locked, created_at FROM game_sessions
         WHERE status <> 'archived' ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| AppError::Internal("no active game session".into()))
}

pub async fn audit(
    conn: &mut SqliteConnection,
    session_id: Option<i64>,
    actor: &str,
    action: &str,
    entity: &str,
    entity_id: impl ToString,
    detail: Value,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO audit_log (session_id, actor, action, entity, entity_id, detail, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(session_id)
    .bind(actor)
    .bind(action)
    .bind(entity)
    .bind(entity_id.to_string())
    .bind(detail.to_string())
    .bind(now_ms())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn activity(
    conn: &mut SqliteConnection,
    session_id: i64,
    round_id: Option<i64>,
    team_id: Option<i64>,
    kind: &str,
    side: Option<&str>,
    status: Option<&str>,
    message: &str,
    actor: &str,
) -> AppResult<i64> {
    let res = sqlx::query(
        "INSERT INTO activity (session_id, round_id, team_id, kind, side, status, message, actor, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(session_id)
    .bind(round_id)
    .bind(team_id)
    .bind(kind)
    .bind(side)
    .bind(status)
    .bind(message)
    .bind(actor)
    .bind(now_ms())
    .execute(&mut *conn)
    .await?;
    Ok(res.last_insert_rowid())
}

/// Team name lookup used in human-readable activity messages.
pub async fn team_name(conn: &mut SqliteConnection, team_id: i64) -> AppResult<String> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT name FROM teams WHERE id = ?").bind(team_id).fetch_optional(&mut *conn).await?;
    Ok(row.map(|r| r.0).unwrap_or_else(|| format!("Tim #{team_id}")))
}
