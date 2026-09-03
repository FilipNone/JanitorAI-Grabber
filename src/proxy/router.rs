use super::capture::capture;
use super::handler::{forward, AppState};
use crate::config::Mode;
use axum::extract::State;
use axum::http::header::HeaderName;
use axum::response::IntoResponse;
use axum::routing::{any, get, post};
use axum::Router;
use tower_http::cors::CorsLayer;

/// uBlock Origin's "Block LAN" filter list blocks requests from public sites
/// (janitorai.com) to loopback addresses like ours. Serving an exception list
/// lets the user subscribe once in uBlock's dashboard; uBlock re-fetches it
/// automatically afterwards, so no further browser interaction is needed.
const UBLOCK_FILTER: &str = concat!(
    "! JanitorAI Grabber - allow janitorai.com to reach the local proxy\n",
    "@@||127.0.0.1:8817^$domain=janitorai.com\n",
);

async fn ublock_filter() -> impl IntoResponse {
    ([("content-type", "text/plain")], UBLOCK_FILTER)
}

pub fn build_router(state: AppState) -> Router {
    // The OpenAI-compatible chat-completions path JanitorAI's proxy presets hit.
    let chat = Router::new().route(
        "/v1/chat/completions",
        post(|State(st): State<AppState>, m, u, h, b| async move {
            match st.mode {
                Mode::Capture => capture(State(st), m, u, h, b).await,
                Mode::Forward => forward(State(st), m, u, h, b).await,
            }
        }),
    );

    // JanitorAI's page calls this endpoint from the browser, so the browser
    // demands CORS headers on the response (and preflight OPTIONS replies).
    // `authorization` must be listed explicitly: the `*` wildcard no longer
    // covers it in Chrome (deprecated per fetch spec, enforced from milestone
    // 97). The private-network header answers Chrome's Local Network Access
    // preflight probe for requests from a public site to loopback.
    let cors = CorsLayer::permissive()
        .allow_headers([
            HeaderName::from_static("authorization"),
            HeaderName::from_static("content-type"),
        ])
        .allow_private_network(true);

    chat.merge(Router::new().route("/v1/{*rest}", any(forward)))
        .route("/ublock.txt", get(ublock_filter))
        .fallback(any(forward))
        .layer(cors)
        .with_state(state)
}
