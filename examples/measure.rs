use market_data::pb::{self, market_data_client::MarketDataClient};
use prost::Message;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fs,
    future::Future,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tonic::{Request, Response, Status, transport::Channel};

const SAMPLES: usize = 50;
const WARMUP: usize = 10;
const FIRST_MS: u64 = 1_791_540_100_000;
const SNAPSHOTS: u64 = 20;
const DEPTH: u64 = 100;

#[derive(Serialize)]
struct Metric {
    method: String,
    requests: usize,
    successes: usize,
    errors: BTreeMap<String, usize>,
    latency_min_ms: f64,
    latency_p50_ms: f64,
    latency_p95_ms: f64,
    latency_max_ms: f64,
    request_protobuf_bytes: usize,
    request_message_bytes: usize,
    response_protobuf_bytes_min: usize,
    response_protobuf_bytes_max: usize,
    response_message_bytes_max: usize,
}

#[derive(Serialize)]
struct Report {
    measured_at_unix_ms: u128,
    endpoint: String,
    client_profile: String,
    samples_per_method: usize,
    warmup_per_method: usize,
    concurrency: usize,
    snapshots: u64,
    levels_per_side: u64,
    size_definition: String,
    metrics: Vec<Metric>,
}

async fn measure<Q, R, F, Fut>(name: &str, query: Q, mut call: F) -> Result<Metric, Box<dyn Error>>
where
    Q: Message + Clone,
    R: Message,
    F: FnMut(Q) -> Fut,
    Fut: Future<Output = Result<Response<R>, Status>>,
{
    for _ in 0..WARMUP {
        call(query.clone()).await?;
    }
    let mut latency = Vec::with_capacity(SAMPLES);
    let mut sizes = Vec::with_capacity(SAMPLES);
    let mut errors = BTreeMap::new();
    for _ in 0..SAMPLES {
        let request = query.clone();
        let started = Instant::now();
        let result = call(request).await;
        latency.push(started.elapsed().as_secs_f64() * 1000.0);
        match result {
            Ok(response) => sizes.push(response.into_inner().encoded_len()),
            Err(error) => *errors.entry(format!("{:?}", error.code())).or_insert(0) += 1,
        }
    }
    latency.sort_by(f64::total_cmp);
    let at = |rank: usize| latency.get(rank).copied().ok_or("missing latency sample");
    let request_bytes = query.encoded_len();
    let min_size = sizes.iter().min().copied().ok_or("all requests failed")?;
    let max_size = sizes.iter().max().copied().ok_or("all requests failed")?;
    Ok(Metric {
        method: name.into(),
        requests: SAMPLES,
        successes: sizes.len(),
        errors,
        latency_min_ms: at(0)?,
        latency_p50_ms: at(SAMPLES.div_ceil(2) - 1)?,
        latency_p95_ms: at((SAMPLES * 95).div_ceil(100) - 1)?,
        latency_max_ms: at(SAMPLES - 1)?,
        request_protobuf_bytes: request_bytes,
        request_message_bytes: request_bytes + 5,
        response_protobuf_bytes_min: min_size,
        response_protobuf_bytes_max: max_size,
        response_message_bytes_max: max_size + 5,
    })
}

fn book(index: u64) -> pb::Snapshot {
    pb::Snapshot {
        symbol: "BENCH-L2".into(),
        timestamp_ms: FIRST_MS + index * 1000,
        sequence: index + 1,
        bids: (0..DEPTH)
            .map(|level| pb::Level {
                price: 62_000_000_000 - level * 1_000_000,
                quantity: (level + 1) * 1_000_000,
            })
            .collect(),
        asks: (0..DEPTH)
            .map(|level| pb::Level {
                price: 62_001_000_000 + level * 1_000_000,
                quantity: (level + 1) * 1_000_000,
            })
            .collect(),
    }
}

fn authorized(query: pb::IngestRequest, token: &str) -> Result<Request<pb::IngestRequest>, Status> {
    let mut request = Request::new(query);
    request.metadata_mut().insert(
        "x-write-token",
        token
            .parse()
            .map_err(|_| Status::invalid_argument("invalid WRITE_TOKEN metadata"))?,
    );
    Ok(request)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let endpoint = match env::var("GRPC_ENDPOINT") {
        Ok(value) => value,
        Err(_) => "http://127.0.0.1:50051".into(),
    };
    let output = env::args()
        .nth(1)
        .map_or("docs/grpc-metrics.json".into(), |v| v);
    let token = env::var("WRITE_TOKEN")?;
    let client = MarketDataClient::connect(endpoint.clone()).await?;
    let mut seed_client = client.clone();
    seed_client
        .ingest(authorized(
            pb::IngestRequest {
                snapshots: (0..SNAPSHOTS).map(book).collect(),
            },
            &token,
        )?)
        .await?;
    let symbol = pb::SymbolRequest {
        symbol: "BENCH-L2".into(),
    };
    let range = pb::RangeRequest {
        symbol: "BENCH-L2".into(),
        from_ms: FIRST_MS,
        to_ms: FIRST_MS + SNAPSHOTS * 1000,
        limit: 100,
    };
    let mut metrics = Vec::new();
    macro_rules! read {
        ($name:literal, $method:ident, $query:expr) => {
            metrics.push(
                measure($name, $query, |query| {
                    let mut client: MarketDataClient<Channel> = client.clone();
                    async move { client.$method(query).await }
                })
                .await?,
            );
        };
    }
    read!("Health", health, pb::Empty {});
    read!("ListSymbols", list_symbols, pb::Empty {});
    read!("GetLatest", get_latest, symbol.clone());
    read!("GetSnapshots", get_snapshots, range.clone());
    read!(
        "GetDepth",
        get_depth,
        pb::DepthRequest {
            symbol: "BENCH-L2".into(),
            levels: 10
        }
    );
    read!("GetSummary", get_summary, symbol.clone());
    read!("GetRange", get_range, symbol);
    read!("GetMidPrices", get_mid_prices, range);
    metrics.push(
        measure(
            "Ingest",
            pb::IngestRequest {
                snapshots: vec![book(SNAPSHOTS - 1)],
            },
            |query| {
                let mut client = client.clone();
                let request = authorized(query, &token);
                async move { client.ingest(request?).await }
            },
        )
        .await?,
    );
    let report = Report {
        measured_at_unix_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
        endpoint, client_profile: if cfg!(debug_assertions) { "debug" } else { "release" }.into(),
        samples_per_method: SAMPLES, warmup_per_method: WARMUP, concurrency: 1,
        snapshots: SNAPSHOTS, levels_per_side: DEPTH,
        size_definition: "protobuf encoded_len; message bytes include 5-byte gRPC prefix; HTTP/2 headers, trailers and TCP excluded; no compression".into(),
        metrics,
    };
    fs::write(&output, serde_json::to_string_pretty(&report)? + "\n")?;
    println!("| Метод | p50, мс | p95, мс | Запрос, байт | Ответ, байт | Успешно |");
    println!("|---|---:|---:|---:|---:|---:|");
    for metric in &report.metrics {
        println!(
            "| {} | {:.3} | {:.3} | {} | {} | {}/{} |",
            metric.method,
            metric.latency_p50_ms,
            metric.latency_p95_ms,
            metric.request_message_bytes,
            metric.response_message_bytes_max,
            metric.successes,
            metric.requests
        );
    }
    println!("Report: {output}");
    if report
        .metrics
        .iter()
        .any(|metric| !metric.errors.is_empty())
    {
        return Err("some benchmark requests failed; see report".into());
    }
    Ok(())
}
