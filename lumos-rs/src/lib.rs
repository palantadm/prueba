//! Lumos engine + web console as a library (used by the CLI binary and the desktop app).

pub mod api;
pub mod engine;
pub mod gdtf;
pub mod glb;
pub mod io;
pub mod library;
pub mod midi;
pub mod model;
pub mod persist;
pub mod qxf;

use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::sync::oneshot;

pub struct ServerConfig {
    /// Address to bind, e.g. `0.0.0.0` (LAN access) or `127.0.0.1`.
    pub bind: String,
    /// Preferred port. If busy, `fallback_random` picks a free one.
    pub port: u16,
    pub fallback_random: bool,
    pub data_dir: PathBuf,
}

pub struct RunningServer {
    pub port: u16,
    pub lan_ip: Option<String>,
    shutdown: Option<oneshot::Sender<()>>,
    done: Option<tokio::task::JoinHandle<()>>,
}

impl RunningServer {
    /// Stop accepting connections and let the engine flush its autosave.
    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.done.take() {
            // Open WebSocket connections would otherwise keep a graceful shutdown waiting forever.
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), h).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
}

/// Start the engine and HTTP/WebSocket server on the current tokio runtime.
pub async fn start(cfg: ServerConfig) -> anyhow::Result<RunningServer> {
    let store = persist::Store::new(cfg.data_dir.clone())?;
    let handle = engine::spawn(store);
    let app = api::router(handle);

    let preferred: SocketAddr = format!("{}:{}", cfg.bind, cfg.port).parse()?;
    let listener = match tokio::net::TcpListener::bind(preferred).await {
        Ok(l) => l,
        Err(e) if cfg.fallback_random => {
            tracing::warn!("port {} busy ({e}); using a free port", cfg.port);
            tokio::net::TcpListener::bind(format!("{}:0", cfg.bind).parse::<SocketAddr>()?).await?
        }
        Err(e) => return Err(e.into()),
    };
    let port = listener.local_addr()?.port();
    let lan_ip = local_ip_address::local_ip().ok().map(|i| i.to_string());

    let (tx, rx) = oneshot::channel::<()>();
    let done = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });
    Ok(RunningServer { port, lan_ip, shutdown: Some(tx), done: Some(done) })
}
