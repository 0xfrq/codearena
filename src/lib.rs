//! Code Arena — live attack-defense recap platform for Python OOP (PBO) practicum.

pub mod assignment;
pub mod auth;
pub mod challenges;
pub mod chat;
pub mod db;
pub mod engine;
pub mod error;
pub mod events;
pub mod models;
pub mod scoring;
pub mod state;
pub mod util;
pub mod views;
pub mod web;
pub mod ws;

use std::time::Duration;

use state::{AppState, Config};

/// Connect, migrate, seed and build the shared application state.
pub async fn build_state(config: Config) -> error::AppResult<AppState> {
    let pool = db::connect(&config.db_path).await?;
    db::migrate(&pool).await?;
    db::seed(&pool, &config).await?;
    if let Some(pw) = &config.admin_password_env {
        tracing::info!("instructor password taken from CODEARENA_ADMIN_PASSWORD ({} chars)", pw.len());
    }
    Ok(AppState::new(pool, config))
}

/// Background timer enforcing server-side phase deadlines (auto-advance).
pub fn spawn_timer(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(1000));
        loop {
            tick.tick().await;
            engine::tick(&state).await;
        }
    });
}

pub fn app(state: AppState) -> axum::Router {
    web::router(state)
}
