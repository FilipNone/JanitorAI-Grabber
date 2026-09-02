use super::handler::{forward, AppState};
use axum::routing::any;
use axum::Router;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/{*rest}", any(forward))
        .fallback(any(forward))
        .with_state(state)
}
