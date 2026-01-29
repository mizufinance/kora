//! HTTP RPC server implementation.

use std::{net::SocketAddr, sync::Arc};

use axum::{Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use tower_http::cors::CorsLayer;
use tracing::{error, info};

use crate::state::NodeState;

/// RPC server for exposing node status via HTTP.
#[derive(Debug)]
pub struct RpcServer {
    state: NodeState,
    addr: SocketAddr,
}

impl RpcServer {
    /// Create a new RPC server.
    pub const fn new(state: NodeState, addr: SocketAddr) -> Self {
        Self { state, addr }
    }

    /// Start the RPC server.
    ///
    /// This spawns a background task and returns immediately.
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        let addr = self.addr;
        let state = Arc::new(self.state);

        tokio::spawn(async move {
            let app = Router::new()
                .route("/status", get(status_handler))
                .route("/health", get(health_handler))
                .layer(CorsLayer::permissive())
                .with_state(state);

            info!(addr = %addr, "Starting RPC server");

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!(error = %e, "Failed to bind RPC server");
                    return;
                }
            };

            if let Err(e) = axum::serve(listener, app).await {
                error!(error = %e, "RPC server error");
            }
        })
    }
}

async fn status_handler(State(state): State<Arc<NodeState>>) -> impl IntoResponse {
    let status = state.status();
    (StatusCode::OK, axum::Json(status))
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}
