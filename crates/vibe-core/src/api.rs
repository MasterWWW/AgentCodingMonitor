use crate::install::{doctor, install_hooks, sync_hook_health_from_disk};
use crate::store::SessionStore;
use crate::types::{DoctorReport, InstallHooksResult, NormalizedEvent, StatusSnapshot};
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use futures_util::stream::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
    pub store: SessionStore,
    pub hook_source_path: Option<std::path::PathBuf>,
    pub hook_search_hints: Vec<std::path::PathBuf>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/status", get(status))
        .route("/api/doctor", get(doctor_handler))
        .route("/api/events", post(events))
        .route("/api/install-hooks", post(install_hooks_handler))
        .route("/api/stream", get(stream))
        .layer(CorsLayer::permissive())
        .with_state(Arc::new(state))
}

async fn status(State(state): State<Arc<AppState>>) -> Json<StatusSnapshot> {
    Json(state.store.snapshot().await)
}

async fn doctor_handler(State(state): State<Arc<AppState>>) -> Json<DoctorReport> {
    Json(doctor(state.hook_source_path.as_deref()).await)
}

async fn events(
    State(state): State<Arc<AppState>>,
    Json(ev): Json<NormalizedEvent>,
) -> Json<StatusSnapshot> {
    state.store.apply_event(ev).await;
    Json(state.store.snapshot().await)
}

async fn install_hooks_handler(
    State(state): State<Arc<AppState>>,
) -> Json<InstallHooksResult> {
    let result = install_hooks(
        state.hook_source_path.as_deref(),
        &state.hook_search_hints,
    );
    if result.ok {
        sync_hook_health_from_disk(&state.store).await;
    }
    Json(result)
}

async fn stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.store.subscribe();
    let initial = state.store.snapshot().await;
    let stream = async_stream::stream! {
        if let Ok(data) = serde_json::to_string(&initial) {
            yield Ok(Event::default().data(data));
        }
        loop {
            match rx.recv().await {
                Ok(snap) => {
                    if let Ok(data) = serde_json::to_string(&snap) {
                        yield Ok(Event::default().data(data));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

