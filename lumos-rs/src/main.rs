mod api;
mod engine;
mod gdtf;
mod glb;
mod io;
mod library;
mod model;
mod persist;
mod qxf;

use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lumos", version, about = "Lumos lighting control — engine + web console")]
struct Args {
    /// TCP port for the web console (0 = pick a free port).
    #[arg(long, default_value_t = 8080)]
    port: u16,
    /// Address to bind.
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,
    /// Data directory (shows, autosave, media).
    #[arg(long, default_value = "data")]
    data: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "lumos=info".into()))
        .init();
    let args = Args::parse();
    let store = persist::Store::new(args.data.clone())?;
    let handle = engine::spawn(store);
    let app = api::router(handle);

    let addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let port = listener.local_addr()?.port();
    println!("Lumos listening on http://127.0.0.1:{port}");
    if let Ok(ip) = local_ip_address::local_ip() {
        println!("  LAN: http://{ip}:{port}");
    }
    println!("  data dir: {}", args.data.display());

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    // Give the engine a moment to flush its autosave once all handles are dropped.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Ok(())
}
