use crate::{AppState, pb, validate_range};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use std::{fmt::Write, sync::Arc, time::Duration};
use tonic::{Code, Status};
use tower::limit::ConcurrencyLimitLayer;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

#[derive(Deserialize)]
struct PlotQuery {
    from_ms: u64,
    to_ms: u64,
    limit: Option<u32>,
}

struct HttpError(Status);

impl From<Status> for HttpError {
    fn from(status: Status) -> Self {
        Self(status)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status = match self.0.code() {
            Code::InvalidArgument => StatusCode::BAD_REQUEST,
            Code::NotFound => StatusCode::NOT_FOUND,
            Code::ResourceExhausted => StatusCode::TOO_MANY_REQUESTS,
            Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(serde_json::json!({"error": self.0.message()}))).into_response()
    }
}

pub fn router(state: Arc<AppState>, origin: HeaderValue) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/plot/{symbol}", get(plot))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(origin)
                .allow_methods([Method::GET]),
        )
        .layer(ConcurrencyLimitLayer::new(32))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(10),
        ))
        .layer(TraceLayer::new_for_http())
}

async fn health(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, HttpError> {
    state.health().await?;
    Ok(Json(serde_json::json!({"status": "ok"})))
}

async fn plot(
    State(state): State<Arc<AppState>>,
    Path(symbol): Path<String>,
    Query(query): Query<PlotQuery>,
) -> Result<Response, HttpError> {
    let range = pb::RangeRequest {
        symbol,
        from_ms: query.from_ms,
        to_ms: query.to_ms,
        limit: query.limit.map_or(0, |v| v),
    };
    validate_range(&range)?;
    let series = state.mid_prices(&range).await?;
    if series.truncated {
        return Err(
            Status::resource_exhausted("more than limit snapshots; narrow the time range").into(),
        );
    }
    let svg = render_svg(&range.symbol, &series.points)?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/svg+xml; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        svg,
    )
        .into_response())
}

pub fn render_svg(symbol: &str, points: &[pb::MidPricePoint]) -> Result<String, Status> {
    crate::validate_symbol(symbol)?;
    let first = points
        .first()
        .ok_or_else(|| Status::not_found("no snapshots in this range"))?;
    let last = points
        .last()
        .ok_or_else(|| Status::not_found("no snapshots in this range"))?;
    if points
        .iter()
        .any(|p| !p.price.is_finite() || p.price <= 0.0)
        || points
            .windows(2)
            .any(|pair| pair[0].timestamp_ms > pair[1].timestamp_ms)
    {
        return Err(Status::data_loss("invalid mid-price series"));
    }
    let minimum = points.iter().map(|p| p.price).fold(f64::INFINITY, f64::min);
    let maximum = points
        .iter()
        .map(|p| p.price)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = maximum - minimum;
    let time_span = last.timestamp_ms.saturating_sub(first.timestamp_ms);
    let mut coordinates = String::with_capacity(points.len() * 24);
    for point in points {
        let x = if time_span == 0 {
            470.0
        } else {
            80.0 + 780.0 * (point.timestamp_ms - first.timestamp_ms) as f64 / time_span as f64
        };
        let y = if span == 0.0 {
            225.0
        } else {
            360.0 - 270.0 * (point.price - minimum) / span
        };
        write!(&mut coordinates, "{x:.2},{y:.2} ")
            .map_err(|_| Status::internal("SVG formatting failed"))?;
    }
    Ok(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="960" height="440" viewBox="0 0 960 440" role="img" aria-labelledby="title desc"><title id="title">{symbol} L2 mid-price</title><desc id="desc">Mid-price from full order book snapshots. Unix milliseconds on the horizontal axis; quote currency units on the vertical axis.</desc><rect width="960" height="440" fill="#101820"/><g fill="#d9e6ee" font-family="sans-serif" font-size="14"><text x="80" y="40" font-size="24">{symbol} · L2 mid-price</text><text x="80" y="70">max {maximum:.6} · min {minimum:.6}</text><text x="80" y="395">{start} ms</text><text x="860" y="395" text-anchor="end">{end} ms</text><text x="80" y="420">price in quote units · time in Unix ms · {count} snapshots</text></g><path d="M80 90V360H860" fill="none" stroke="#557080"/><polyline points="{coordinates}" fill="none" stroke="#4fe0b0" stroke-width="2" stroke-linejoin="round"/><g fill="#4fe0b0">{dots}</g></svg>"##,
        start = first.timestamp_ms,
        end = last.timestamp_ms,
        count = points.len(),
        dots = coordinates
            .split_whitespace()
            .map(|coordinate| match coordinate.split_once(',') {
                Some((x, y)) => format!(r#"<circle cx="{x}" cy="{y}" r="2"/>"#),
                None => String::new(),
            })
            .collect::<String>()
    ))
}
