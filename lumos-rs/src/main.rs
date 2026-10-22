use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lumos", version, about = "Lumos lighting control — engine + web console")]
struct Args {
    /// TCP port for the web console.
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
    let server = lumos::start(lumos::ServerConfig {
        bind: args.bind,
        port: args.port,
        fallback_random: false,
        data_dir: args.data.clone(),
    })
    .await?;
    println!("Lumos listening on http://127.0.0.1:{}", server.port);
    if let Some(ip) = &server.lan_ip {
        println!("  LAN: http://{ip}:{}", server.port);
    }
    println!("  data dir: {}", args.data.display());
    let _ = tokio::signal::ctrl_c().await;
    server.stop().await;
    Ok(())
}
