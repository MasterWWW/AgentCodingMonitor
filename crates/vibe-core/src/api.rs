use crate::install::{doctor, install_hooks, sync_hook_health_from_disk};
use crate::lan::{self, extract_token, token_matches};
use crate::mobile::INDEX_HTML;
use crate::state::{ensure_lan_token, load_lan_companion_enabled, load_lan_companion_token};
use crate::store::SessionStore;
use crate::types::{DoctorReport, InstallHooksResult, NormalizedEvent, StatusSnapshot};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, Method, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::stream::Stream;
use serde::Serialize;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
    pub store: SessionStore,
    pub hook_source_path: Option<std::path::PathBuf>,
    pub hook_search_hints: Vec<std::path::PathBuf>,
}

#[derive(Debug, Serialize)]
struct LanInfoResponse {
    enabled: bool,
    port: u16,
    token: String,
    urls: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: &'static str,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/mobile", get(mobile_page))
        .route("/api/status", get(status))
        .route("/api/doctor", get(doctor_handler))
        .route("/api/events", post(events))
        .route("/api/install-hooks", post(install_hooks_handler))
        .route("/api/stream", get(stream))
        .route("/api/lan-info", get(lan_info))
        .layer(middleware::from_fn(lan_guard))
        .layer(CorsLayer::permissive())
        .with_state(Arc::new(state))
}

/// 局域网鉴权：写接口仅 loopback；LAN 读接口需 token。
async fn lan_guard(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let method = request.method();
    let is_local = lan::is_loopback(peer.ip());

    if !is_local {
        if method == Method::POST || path == "/api/doctor" || path == "/api/install-hooks" {
            return json_error(StatusCode::FORBIDDEN, "forbidden");
        }
        if path == "/api/lan-info" {
            return json_error(StatusCode::FORBIDDEN, "forbidden");
        }
        if path == "/api/status" || path == "/api/stream" {
            let expected = load_lan_companion_token().unwrap_or_default();
            let provided = extract_token(
                request.uri().query(),
                request
                    .headers()
                    .get(header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok()),
            );
            if !token_matches(provided.as_deref(), &expected) {
                return json_error(StatusCode::UNAUTHORIZED, "unauthorized");
            }
        }
    }

    next.run(request).await
}

fn json_error(status: StatusCode, error: &'static str) -> Response {
    (status, Json(ErrorBody { error })).into_response()
}

async fn mobile_page() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        INDEX_HTML,
    )
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

async fn lan_info(State(state): State<Arc<AppState>>) -> Result<Json<LanInfoResponse>, StatusCode> {
    let port = state.store.port();
    let enabled = load_lan_companion_enabled();
    let token = if enabled {
        ensure_lan_token().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        load_lan_companion_token().unwrap_or_default()
    };
    let urls = if enabled && !token.is_empty() {
        lan::companion_urls(port, &token)
    } else {
        vec![]
    };
    Ok(Json(LanInfoResponse {
        enabled,
        port,
        token,
        urls,
    }))
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
