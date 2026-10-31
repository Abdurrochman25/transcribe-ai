//! TranscribeAI API — entrypoint.
//! Urutan bootstrap: .env → tracing → config (fail-fast) → DB + migrasi → serve.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use transcribe_api::{app, config::Config, state::AppState};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    init_tracing();

    let cfg = Config::from_env()?;
    let port = cfg.port;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        port,
        "menjalankan TranscribeAI API"
    );

    let db = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&cfg.database_url)
        .await
        .context(
            "gagal koneksi ke PostgreSQL — pastikan `docker compose up -d postgres` berjalan \
             atau DATABASE_URL benar",
        )?;

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .context("gagal menjalankan migrasi database")?;
    tracing::info!("migrasi database selesai");

    let state = AppState {
        db,
        cfg: Arc::new(cfg),
    };
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .with_context(|| format!("gagal bind port {port}"))?;

    tracing::info!("listening on http://0.0.0.0:{port}");
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server berhenti dengan error")?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,transcribe_api=debug"));
    let json = std::env::var("LOG_FORMAT").is_ok_and(|v| v.eq_ignore_ascii_case("json"));

    if json {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer())
            .init();
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("sinyal shutdown diterima — berhenti dengan anggun");
}
