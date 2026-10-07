use market_data::{AppState, grpc, http, pb::market_data_server::MarketDataServer};
use std::{env, error::Error, net::SocketAddr, sync::Arc, time::Duration};
use tracing_subscriber::EnvFilter;

fn setting(name: &str, fallback: &str) -> String {
    match env::var(name) {
        Ok(value) => value,
        Err(_) => fallback.to_owned(),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info"))?)
        .init();
    let token = env::var("WRITE_TOKEN")?;
    if token.len() < 16 {
        return Err("WRITE_TOKEN must contain at least 16 bytes".into());
    }
    let client = clickhouse::Client::default()
        .with_url(setting("CLICKHOUSE_URL", "http://127.0.0.1:8123"))
        .with_user(setting("CLICKHOUSE_USER", "market"))
        .with_password(setting("CLICKHOUSE_PASSWORD", "local-market-password"))
        .with_database(setting("CLICKHOUSE_DATABASE", "market"))
        .with_setting("max_execution_time", "5")
        .with_setting("max_memory_usage", "134217728");
    let state = Arc::new(AppState::new(client, token));
    tokio::time::timeout(Duration::from_secs(10), state.initialize()).await??;
    let grpc_addr: SocketAddr = setting("GRPC_ADDR", "127.0.0.1:50051").parse()?;
    let http_addr: SocketAddr = setting("HTTP_ADDR", "127.0.0.1:3000").parse()?;
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    let origin = setting("CLIENT_ORIGIN", "http://localhost:5173").parse()?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let mut grpc_shutdown = shutdown_rx.clone();
    let mut http_shutdown = shutdown_rx;
    let grpc_server = tonic::transport::Server::builder()
        .concurrency_limit_per_connection(32)
        .timeout(Duration::from_secs(10))
        .add_service(
            MarketDataServer::new(grpc::Service(state.clone()))
                .max_decoding_message_size(1_048_576),
        )
        .serve_with_shutdown(grpc_addr, async move {
            let _ = grpc_shutdown.changed().await;
        });
    let http_server =
        axum::serve(listener, http::router(state, origin)).with_graceful_shutdown(async move {
            let _ = http_shutdown.changed().await;
        });
    tracing::info!(%grpc_addr, %http_addr, "server started");
    let signal = async move {
        tokio::signal::ctrl_c().await?;
        shutdown_tx.send(true)?;
        Ok::<(), Box<dyn Error>>(())
    };
    tokio::try_join!(
        async { grpc_server.await.map_err(|e| Box::new(e) as Box<dyn Error>) },
        async { http_server.await.map_err(|e| Box::new(e) as Box<dyn Error>) },
        signal
    )?;
    Ok(())
}
