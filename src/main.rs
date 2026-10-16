use codearena::state::Config;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")))
        .init();

    let config = Config::from_env();
    let bind = config.bind.clone();
    let state = match codearena::build_state(config).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("startup failed: {e}");
            std::process::exit(1);
        }
    };
    if state.config.run_timer {
        codearena::spawn_timer(state.clone());
    }
    let listener = match tokio::net::TcpListener::bind(&bind).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot bind {bind}: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!("Code Arena listening on http://{bind}  (students: /  ·  instructor: /admin  ·  arena: /arena)");
    if let Err(e) = axum::serve(listener, codearena::app(state)).with_graceful_shutdown(shutdown()).await {
        eprintln!("server error: {e}");
    }
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
