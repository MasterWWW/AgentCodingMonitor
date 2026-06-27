use crate::action::ActionStore;
use crate::event::phase_for_event;
use crate::install::{doctor, install_hooks, sync_hook_health_from_disk};
use crate::lan::{self, extract_token, token_matches};
use crate::mobile::INDEX_HTML;
use crate::state::{
    ensure_lan_token, ensure_watch_token, load_lan_companion_enabled, load_lan_companion_token,
    load_watch_companion_enabled, load_watch_companion_token, load_watch_device_id,
    watch_pairing_payload, watch_service_name,
};
use crate::store::SessionStore;
use crate::types::{DoctorReport, InstallHooksResult, NormalizedEvent, StatusSnapshot, VibePhase};
use axum::{
    extract::{ConnectInfo, Path, State},
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
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use uuid::Uuid;
use vibe_protocol::{ResponseOrigin, WatchStreamEvent};

#[derive(Clone)]
pub struct AppState {
    pub store: SessionStore,
    pub action_store: ActionStore,
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
struct WatchInfoResponse {
    enabled: bool,
    port: u16,
    device_id: String,
    service: String,
    token: String,
    pairing: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct RespondBody {
    choice: String,
    #[serde(default)]
    from: Option<ResponseOrigin>,
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
        .route("/api/watch-info", get(watch_info))
        .route("/api/watch/stream", get(watch_stream))
        .route("/api/actions/pending", get(actions_pending))
        .route("/api/actions/:id/respond", post(action_respond))
        .layer(middleware::from_fn(lan_guard))
        .layer(CorsLayer::permissive())
        .with_state(Arc::new(state))
}

fn extract_auth_token(request: &axum::http::Request<axum::body::Body>) -> Option<String> {
    extract_token(
        request.uri().query(),
        request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
    )
}

fn token_valid_for_lan_read(provided: Option<&str>) -> bool {
    let lan = load_lan_companion_token().unwrap_or_default();
    let watch = load_watch_companion_token().unwrap_or_default();
    if !lan.is_empty() && token_matches(provided, &lan) {
        return true;
    }
    if !watch.is_empty() && token_matches(provided, &watch) {
        return true;
    }
    false
}

fn token_valid_for_watch(provided: Option<&str>) -> bool {
    let watch = load_watch_companion_token().unwrap_or_default();
    token_matches(provided, &watch)
}

/// 局域网鉴权：hook 写接口仅 loopback；看板只读；手表动作读写用 watch token。
async fn lan_guard(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let method = request.method();
    let is_local = lan::is_loopback(peer.ip());
    let token = extract_auth_token(&request);

    if !is_local {
        let is_action_respond =
            method == Method::POST && path.starts_with("/api/actions/") && path.ends_with("/respond");
        let is_watch_read = path == "/api/actions/pending"
            || path == "/api/watch/stream"
            || path == "/api/watch-info";
        let is_lan_read = path == "/api/status" || path == "/api/stream" || path == "/mobile";

        if method == Method::POST && !is_action_respond {
            return json_error(StatusCode::FORBIDDEN, "forbidden");
        }
        if path == "/api/doctor" || path == "/api/install-hooks" || path == "/api/lan-info" {
            return json_error(StatusCode::FORBIDDEN, "forbidden");
        }
        if path == "/api/watch-info" {
            return json_error(StatusCode::FORBIDDEN, "forbidden");
        }
        if is_action_respond || is_watch_read {
            if !token_valid_for_watch(token.as_deref()) {
                return json_error(StatusCode::UNAUTHORIZED, "unauthorized");
            }
        } else if is_lan_read {
            if !token_valid_for_lan_read(token.as_deref()) {
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
    state.store.apply_event(ev.clone()).await;
    if load_watch_companion_enabled() {
        if phase_for_event(&ev.event_name) == Some(VibePhase::WaitingUser) {
            state.action_store.maybe_create_from_event(&ev).await;
        }
    }
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

async fn watch_info(State(state): State<Arc<AppState>>) -> Result<Json<WatchInfoResponse>, StatusCode> {
    let port = state.store.port();
    let enabled = load_watch_companion_enabled();
    if !enabled {
        return Ok(Json(WatchInfoResponse {
            enabled: false,
            port,
            device_id: String::new(),
            service: String::new(),
            token: load_watch_companion_token().unwrap_or_default(),
            pairing: serde_json::json!({}),
        }));
    }
    let device_id = load_watch_device_id().unwrap_or_default();
    let token = ensure_watch_token().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let service = if device_id.is_empty() {
        String::new()
    } else {
        watch_service_name(&device_id)
    };
    let pairing = watch_pairing_payload(port).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(WatchInfoResponse {
        enabled,
        port,
        device_id,
        service,
        token,
        pairing,
    }))
}

async fn actions_pending(State(state): State<Arc<AppState>>) -> Json<Vec<vibe_protocol::PendingAction>> {
    Json(state.action_store.pending().await)
}

async fn action_respond(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<RespondBody>,
) -> Result<Json<vibe_protocol::ResolvedAction>, Response> {
    let from = body.from.unwrap_or(ResponseOrigin::Phone);
    match state
        .action_store
        .respond(id, body.choice, from)
        .await
    {
        Ok(resolved) => Ok(Json(resolved)),
        Err(err) => Err(json_error(err.status_code(), err.message())),
    }
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

async fn watch_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.action_store.subscribe();
    let pending = state.action_store.pending().await;
    let snap = state.store.snapshot().await;
    let stream = async_stream::stream! {
        let status = WatchStreamEvent::Status {
            data: serde_json::to_value(&snap).unwrap_or_default(),
        };
        if let Ok(data) = serde_json::to_string(&status) {
            yield Ok(Event::default().event("status").data(data));
        }
        for action in pending {
            let ev = WatchStreamEvent::ActionCreated { data: action };
            if let Ok(data) = serde_json::to_string(&ev) {
                yield Ok(Event::default().event("action_created").data(data));
            }
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let event_name = match &ev {
                        WatchStreamEvent::Status { .. } => "status",
                        WatchStreamEvent::ActionCreated { .. } => "action_created",
                        WatchStreamEvent::ActionResolved { .. } => "action_resolved",
                        WatchStreamEvent::ActionCancelled { .. } => "action_cancelled",
                    };
                    if let Ok(data) = serde_json::to_string(&ev) {
                        yield Ok(Event::default().event(event_name).data(data));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
