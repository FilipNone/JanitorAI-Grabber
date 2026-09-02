use super::capture::capture;
use super::handler::{forward, AppState};
use crate::config::Mode;
use axum::extract::State;
use axum::routing::{any, post};
use axum::Router;

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

    chat.merge(Router::new().route("/v1/{*rest}", any(forward)))
        .fallback(any(forward))
        .with_state(state)
}
