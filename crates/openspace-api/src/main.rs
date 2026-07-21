//! OpenSpace HTTP API server.

mod db;
mod routes;
mod state;
mod worker;

use axum::Router;
use clap::Parser;
use state::AppState;
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "openspace-api",
    about = "OpenSpace floorplan reconstruction API",
    version
)]
struct Args {
    /// HTTP listen port (env: PORT)
    #[arg(short = 'p', long = "port", env = "PORT", default_value_t = 8080)]
    port: u16,

    /// Bind address (env: OPENSPACE_HOST)
    #[arg(long = "host", env = "OPENSPACE_HOST", default_value = "0.0.0.0")]
    host: String,

    /// Data directory for SQLite + uploads (env: OPENSPACE_DATA)
    #[arg(long = "data", env = "OPENSPACE_DATA", default_value = "data")]
    data: PathBuf,

    /// Static web assets directory (env: OPENSPACE_WEB)
    #[arg(long = "web", env = "OPENSPACE_WEB", default_value = "web/dist")]
    web: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args = Args::parse();
    std::fs::create_dir_all(args.data.join("projects"))?;

    let state = AppState::new(args.data.clone()).await?;
    worker::spawn_worker(state.clone());

    let index = args.web.join("index.html");

    let app = Router::new()
        .merge(routes::api_router())
        .fallback_service(
            ServeDir::new(&args.web)
                .append_index_html_on_directories(true)
                .not_found_service(ServeFile::new(index)),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let host: std::net::IpAddr = args
        .host
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid --host '{}': {e}", args.host))?;
    let addr = SocketAddr::from((host, args.port));
    tracing::info!("OpenSpace listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        anyhow::anyhow!(
            "failed to bind {addr}: {e}\n\
             Hint: port may be in use. Try another port, e.g.\n\
               cargo run -p openspace-api --release -- --port 8081\n\
             PowerShell:\n\
               $env:PORT=8081; cargo run -p openspace-api --release"
        )
    })?;
    axum::serve(listener, app).await?;
    Ok(())
}
