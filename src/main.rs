mod agent;
mod claude;
mod codex;
mod ws;

use axum::{routing::get, Router};
use dashmap::DashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let sessions = Arc::new(DashMap::new());

    let app = Router::new()
        .route("/ws", get(ws::ws_handler))
        .with_state(sessions);

    let addr = "0.0.0.0:3000";
    tracing::info!("WebSocket server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
