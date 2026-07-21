//! OpenSpace HTTP API server.

mod db;
mod routes;
mod state;
mod worker;

use axum::Router;
use state::AppState;
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let data_dir = PathBuf::from(
        std::env::var("OPENSPACE_DATA").unwrap_or_else(|_| "data".into()),
    );
    std::fs::create_dir_all(data_dir.join("projects"))?;

    let state = AppState::new(data_dir.clone()).await?;
    worker::spawn_worker(state.clone());

    let web_dir = PathBuf::from(std::env::var("OPENSPACE_WEB").unwrap_or_else(|_| "web/dist".into()));
    let index = web_dir.join("index.html");

    let app = Router::new()
        .merge(routes::api_router())
        .fallback_service(
            ServeDir::new(&web_dir)
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

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("OpenSpace listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
