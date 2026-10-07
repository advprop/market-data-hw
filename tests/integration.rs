use axum::{
    body::{Body, to_bytes},
    http::{Request as HttpRequest, StatusCode, header},
};
use market_data::{
    AppState,
    grpc::Service,
    http,
    pb::{self, market_data_client::MarketDataClient, market_data_server::MarketDataServer},
};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Code, Request};
use tower::ServiceExt;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const TOKEN: &str = "integration-test-token";

fn database() -> clickhouse::Client {
    clickhouse::Client::default()
        .with_url(match std::env::var("CLICKHOUSE_URL") {
            Ok(value) => value,
            Err(_) => "http://127.0.0.1:8123".into(),
        })
        .with_user("market")
        .with_password("local-market-password")
        .with_database("market")
        .with_setting("max_execution_time", "5")
}

fn book(symbol: &str, timestamp_ms: u64, sequence: u64) -> pb::Snapshot {
    pb::Snapshot {
        symbol: symbol.into(),
        timestamp_ms,
        sequence,
        bids: vec![
            pb::Level {
                price: 99_000_000,
                quantity: 2_000_000,
            },
            pb::Level {
                price: 100_000_000,
                quantity: 1_000_000,
            },
        ],
        asks: vec![
            pb::Level {
                price: 102_000_000,
                quantity: 1_000_000,
            },
            pb::Level {
                price: 101_000_000,
                quantity: 1_000_000,
            },
        ],
    }
}

fn authorized(
    snapshots: Vec<pb::Snapshot>,
) -> Result<Request<pb::IngestRequest>, Box<dyn std::error::Error>> {
    let mut request = Request::new(pb::IngestRequest { snapshots });
    request
        .metadata_mut()
        .insert("x-write-token", TOKEN.parse()?);
    Ok(request)
}

#[tokio::test]
#[ignore = "requires docker compose up -d --wait"]
async fn all_requirements_over_grpc_and_http() -> TestResult {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("market_data=error")
        .try_init();
    let state = Arc::new(AppState::new(database(), TOKEN.into()));
    state.initialize().await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(
                MarketDataServer::new(Service(state.clone())).max_decoding_message_size(1_048_576),
            )
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                let _ = shutdown_rx.await;
            }),
    );
    let mut client = MarketDataClient::connect(format!("http://{address}")).await?;
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let symbol = format!("T-{suffix}");
    let unknown = pb::SymbolRequest {
        symbol: "UNKNOWN-TEST".into(),
    };
    let first = book(&symbol, 1000, 1);
    let second = book(&symbol, 2000, 2);
    assert_eq!(client.health(pb::Empty {}).await?.into_inner().status, "ok");
    assert!(matches!(client.get_latest(unknown).await, Err(e) if e.code() == Code::NotFound));
    assert!(
        matches!(client.ingest(pb::IngestRequest { snapshots: vec![first.clone()] }).await, Err(e) if e.code() == Code::Unauthenticated)
    );
    let mut bad = first.clone();
    bad.asks.clear();
    assert!(
        matches!(client.ingest(authorized(vec![second.clone(), bad])?).await, Err(e) if e.code() == Code::InvalidArgument)
    );
    assert!(state.latest(&symbol).await.is_err());
    assert_eq!(
        client
            .ingest(authorized(vec![second.clone(), first.clone()])?)
            .await?
            .into_inner()
            .accepted,
        2
    );
    client.ingest(authorized(vec![first])?).await?;
    let lookup = pb::SymbolRequest {
        symbol: symbol.clone(),
    };
    assert!(
        client
            .list_symbols(pb::Empty {})
            .await?
            .into_inner()
            .symbols
            .contains(&symbol)
    );
    let latest = client.get_latest(lookup.clone()).await?.into_inner();
    assert_eq!(latest.timestamp_ms, 2000);
    assert_eq!(latest.bids.first().map(|v| v.price), Some(100_000_000));
    let range = pb::RangeRequest {
        symbol: symbol.clone(),
        from_ms: 1000,
        to_ms: 2001,
        limit: 100,
    };
    let history = client.get_snapshots(range.clone()).await?.into_inner();
    assert_eq!(history.snapshots.len(), 2);
    assert!(!history.truncated);
    assert_eq!(
        history.snapshots.first().map(|v| v.timestamp_ms),
        Some(1000)
    );
    let exclusive = pb::RangeRequest {
        to_ms: 2000,
        ..range.clone()
    };
    assert_eq!(
        client
            .get_snapshots(exclusive)
            .await?
            .into_inner()
            .snapshots
            .len(),
        1
    );
    let limited = pb::RangeRequest {
        limit: 1,
        ..range.clone()
    };
    assert!(
        client
            .get_snapshots(limited.clone())
            .await?
            .into_inner()
            .truncated
    );
    assert!(client.get_mid_prices(limited).await?.into_inner().truncated);
    let depth = client
        .get_depth(pb::DepthRequest {
            symbol: symbol.clone(),
            levels: 1,
        })
        .await?
        .into_inner();
    assert_eq!(depth.bids.len(), 1);
    assert_eq!(depth.asks.len(), 1);
    let metrics = client.get_summary(lookup.clone()).await?.into_inner();
    assert_eq!(metrics.spread, 1_000_000);
    assert_eq!(metrics.mid_price, 100.5);
    assert!((metrics.imbalance - 0.2).abs() < 1e-12);
    let coverage = client.get_range(lookup).await?.into_inner();
    assert_eq!(
        (coverage.first_ms, coverage.last_ms, coverage.count),
        (1000, 2000, 2)
    );
    let series = client.get_mid_prices(range).await?.into_inner();
    assert_eq!(series.points.len(), 2);
    assert_eq!(series.points.first().map(|p| p.price), Some(100.5));
    let reopened = AppState::new(database(), TOKEN.into());
    assert_eq!(reopened.latest(&symbol).await?.timestamp_ms, 2000);
    let app = http::router(state, "http://localhost:5173".parse()?);
    let request = HttpRequest::builder()
        .uri(format!("/v1/plot/{symbol}?from_ms=1000&to_ms=2001"))
        .body(Body::empty())?;
    let response = app.clone().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("image/svg+xml; charset=utf-8")
    );
    let svg = String::from_utf8(to_bytes(response.into_body(), 32_768).await?.to_vec())?;
    assert!(svg.contains("<svg"));
    assert!(svg.contains(&symbol));
    let request = HttpRequest::builder()
        .uri(format!("/v1/plot/{symbol}?from_ms=1000&to_ms=2001&limit=1"))
        .body(Body::empty())?;
    assert_eq!(
        app.clone().oneshot(request).await?.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    let request = HttpRequest::builder()
        .uri("/health")
        .header(header::ORIGIN, "http://localhost:5173")
        .body(Body::empty())?;
    let response = app.oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|v| v.to_str().ok()),
        Some("http://localhost:5173")
    );
    shutdown_tx
        .send(())
        .map_err(|_| "server stopped unexpectedly")?;
    server.await??;
    Ok(())
}
