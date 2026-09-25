use anyhow::Result;
use external_dns_provider_mikrotik::{app::build_routers, config::Config};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();
    let cfg = Config::from_env()?;
    let addr = cfg.addr;
    let health_addr = cfg.health_addr;
    let (app, health) = build_routers(cfg)?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let health_listener = tokio::net::TcpListener::bind(health_addr).await?;
    tokio::select! {
        result = axum::serve(listener, app).with_graceful_shutdown(shutdown()) => result?,
        result = axum::serve(health_listener, health).with_graceful_shutdown(shutdown()) => result?,
    }
    Ok(())
}
async fn shutdown() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
