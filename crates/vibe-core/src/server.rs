use crate::api::{router, AppState};
use crate::install::sync_hook_health_from_disk;
use crate::lite::spawn_lite_watcher;
use crate::paths::{read_port, write_port};
use crate::store::SessionStore;
use anyhow::Result;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;

const DEFAULT_PORT: u16 = 17392;
const MAX_PORT_TRIES: u16 = 5;

pub struct RunningServer {
    pub port: u16,
    pub store: SessionStore,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

impl RunningServer {
    pub async fn stop(self) {
        let _ = self.shutdown.send(());
    }
}

pub async fn start(
    hook_source_path: Option<PathBuf>,
    hook_search_hints: Vec<PathBuf>,
    lite_enabled: bool,
) -> Result<RunningServer> {
    let port = bind_port().await?;
    write_port(port)?;
    let store = SessionStore::new(port);
    store.set_lite_mode(lite_enabled).await;
    sync_hook_health_from_disk(&store).await;
    spawn_lite_watcher(store.clone());

    let state = AppState {
        store: store.clone(),
        hook_source_path,
        hook_search_hints,
    };
    let app = router(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let store_tick = store.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            store_tick.tick_idle().await;
        }
    });

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .ok();
    });

    tracing::info!("vibe-core listening on http://127.0.0.1:{port}");

    Ok(RunningServer {
        port,
        store,
        shutdown: shutdown_tx,
    })
}

async fn bind_port() -> Result<u16> {
    let start = read_port().unwrap_or(DEFAULT_PORT);
    for offset in 0..MAX_PORT_TRIES {
        let port = start + offset;
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        if TcpListener::bind(addr).await.is_ok() {
            return Ok(port);
        }
    }
    anyhow::bail!("could not bind port {start}..{}", start + MAX_PORT_TRIES - 1);
}

pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}
