use clickhouse::{Client, Row};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use tonic::Status;

pub mod grpc;
pub mod http;
pub mod pb {
    tonic::include_proto!("market.v1");
}

pub const MAX_DEPTH: usize = 100;
pub const MAX_BATCH: usize = 100;
pub const MAX_RESULTS: u32 = 100;
pub const SCALE: u64 = 1_000_000;
const MAX_VALUE: u64 = 1_000_000_000_000_000;

#[derive(Clone, Debug, Row, Serialize, Deserialize)]
pub struct StoredSnapshot {
    pub symbol: String,
    pub timestamp_ms: u64,
    pub sequence: u64,
    pub bids: Vec<(u64, u64)>,
    pub asks: Vec<(u64, u64)>,
}

#[derive(Row, Deserialize)]
struct SymbolRow {
    symbol: String,
}

#[derive(Row, Deserialize)]
struct RangeRow {
    first_ms: u64,
    last_ms: u64,
    count: u64,
}

pub struct AppState {
    client: Client,
    token: String,
    capacity: Arc<Semaphore>,
}

fn database_error(error: clickhouse::error::Error) -> Status {
    tracing::error!(%error, "database request failed");
    Status::unavailable("database unavailable")
}

pub fn validate_symbol(symbol: &str) -> Result<(), Status> {
    if symbol.is_empty()
        || symbol.len() > 32
        || !symbol
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(Status::invalid_argument(
            "symbol must contain 1..32 uppercase ASCII letters, digits or hyphens",
        ));
    }
    Ok(())
}

fn normalize_levels(levels: Vec<pb::Level>, descending: bool) -> Result<Vec<(u64, u64)>, Status> {
    if levels.is_empty() || levels.len() > MAX_DEPTH {
        return Err(Status::invalid_argument(
            "each side must contain 1..100 levels",
        ));
    }
    let mut sorted = BTreeMap::new();
    for level in levels {
        if level.price == 0
            || level.price > MAX_VALUE
            || level.quantity == 0
            || level.quantity > MAX_VALUE
        {
            return Err(Status::invalid_argument(
                "price and quantity must be in 1..10^15",
            ));
        }
        if sorted.insert(level.price, level.quantity).is_some() {
            return Err(Status::invalid_argument("duplicate price on the same side"));
        }
    }
    if descending {
        Ok(sorted.into_iter().rev().collect())
    } else {
        Ok(sorted.into_iter().collect())
    }
}

impl TryFrom<pb::Snapshot> for StoredSnapshot {
    type Error = Status;

    fn try_from(snapshot: pb::Snapshot) -> Result<Self, Self::Error> {
        validate_symbol(&snapshot.symbol)?;
        if snapshot.timestamp_ms == 0 || snapshot.timestamp_ms > 253_402_300_799_999 {
            return Err(Status::invalid_argument(
                "timestamp_ms must be a positive Unix timestamp before year 10000",
            ));
        }
        let bids = normalize_levels(snapshot.bids, true)?;
        let asks = normalize_levels(snapshot.asks, false)?;
        let best_bid = bids
            .first()
            .ok_or_else(|| Status::invalid_argument("missing bids"))?
            .0;
        let best_ask = asks
            .first()
            .ok_or_else(|| Status::invalid_argument("missing asks"))?
            .0;
        if best_bid >= best_ask {
            return Err(Status::invalid_argument(
                "best bid must be less than best ask",
            ));
        }
        Ok(Self {
            symbol: snapshot.symbol,
            timestamp_ms: snapshot.timestamp_ms,
            sequence: snapshot.sequence,
            bids,
            asks,
        })
    }
}

impl From<StoredSnapshot> for pb::Snapshot {
    fn from(snapshot: StoredSnapshot) -> Self {
        let levels = |side: Vec<(u64, u64)>| {
            side.into_iter()
                .map(|(price, quantity)| pb::Level { price, quantity })
                .collect()
        };
        Self {
            symbol: snapshot.symbol,
            timestamp_ms: snapshot.timestamp_ms,
            sequence: snapshot.sequence,
            bids: levels(snapshot.bids),
            asks: levels(snapshot.asks),
        }
    }
}

pub fn summary(snapshot: &StoredSnapshot) -> Result<pb::SummaryReply, Status> {
    let bid = snapshot
        .bids
        .first()
        .ok_or_else(|| Status::data_loss("stored snapshot has no bids"))?
        .0;
    let ask = snapshot
        .asks
        .first()
        .ok_or_else(|| Status::data_loss("stored snapshot has no asks"))?
        .0;
    let sum = |side: &[(u64, u64)]| {
        side.iter().try_fold(0_u64, |total, &(_, quantity)| {
            total
                .checked_add(quantity)
                .ok_or_else(|| Status::data_loss("quantity overflow"))
        })
    };
    let bid_quantity = sum(&snapshot.bids)?;
    let ask_quantity = sum(&snapshot.asks)?;
    let total = bid_quantity
        .checked_add(ask_quantity)
        .ok_or_else(|| Status::data_loss("quantity overflow"))?;
    let spread = ask
        .checked_sub(bid)
        .ok_or_else(|| Status::data_loss("crossed stored book"))?;
    if total == 0 {
        return Err(Status::data_loss("empty stored book"));
    }
    Ok(pb::SummaryReply {
        timestamp_ms: snapshot.timestamp_ms,
        best_bid: bid,
        best_ask: ask,
        spread,
        mid_price: (bid as f64 + ask as f64) / (2.0 * SCALE as f64),
        bid_quantity,
        ask_quantity,
        imbalance: (bid_quantity as f64 - ask_quantity as f64) / total as f64,
    })
}

pub fn validate_range(range: &pb::RangeRequest) -> Result<u32, Status> {
    validate_symbol(&range.symbol)?;
    if range.from_ms >= range.to_ms || range.to_ms > 253_402_300_799_999 {
        return Err(Status::invalid_argument(
            "require 0 <= from_ms < to_ms before year 10000",
        ));
    }
    let limit = if range.limit == 0 {
        MAX_RESULTS
    } else {
        range.limit
    };
    if limit > MAX_RESULTS {
        return Err(Status::invalid_argument(
            "limit must be in 1..100, or 0 for default",
        ));
    }
    Ok(limit)
}

impl AppState {
    pub fn new(client: Client, token: String) -> Self {
        Self {
            client,
            token,
            capacity: Arc::new(Semaphore::new(32)),
        }
    }

    async fn permit(&self) -> Result<tokio::sync::OwnedSemaphorePermit, Status> {
        self.capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| Status::resource_exhausted("server is busy; retry later"))
    }

    pub fn authorize(&self, token: Option<&str>) -> Result<(), Status> {
        match token {
            Some(value) if value == self.token => Ok(()),
            _ => Err(Status::unauthenticated(
                "valid x-write-token metadata required",
            )),
        }
    }

    pub async fn initialize(&self) -> Result<(), Status> {
        self.client.query("CREATE TABLE IF NOT EXISTS snapshots (symbol String, timestamp_ms UInt64, sequence UInt64, bids Array(Tuple(UInt64, UInt64)), asks Array(Tuple(UInt64, UInt64))) ENGINE = ReplacingMergeTree ORDER BY (symbol, timestamp_ms, sequence)").execute().await.map_err(database_error)
    }

    pub async fn health(&self) -> Result<(), Status> {
        let _permit = self.permit().await?;
        self.client
            .query("SELECT 1")
            .fetch_one::<u8>()
            .await
            .map_err(database_error)?;
        Ok(())
    }

    pub async fn ingest(&self, snapshots: Vec<pb::Snapshot>) -> Result<u64, Status> {
        if snapshots.is_empty() || snapshots.len() > MAX_BATCH {
            return Err(Status::invalid_argument(
                "batch must contain 1..100 snapshots",
            ));
        }
        let mut normalized = BTreeMap::new();
        for snapshot in snapshots {
            let row = StoredSnapshot::try_from(snapshot)?;
            let key = (row.symbol.clone(), row.timestamp_ms, row.sequence);
            if normalized.insert(key, row).is_some() {
                return Err(Status::invalid_argument(
                    "duplicate snapshot identity within batch",
                ));
            }
        }
        let count = normalized.len() as u64;
        let _permit = self.permit().await?;
        let mut insert = self
            .client
            .insert::<StoredSnapshot>("snapshots")
            .await
            .map_err(database_error)?
            .with_timeouts(Some(Duration::from_secs(5)), Some(Duration::from_secs(5)));
        for row in normalized.values() {
            insert.write(row).await.map_err(database_error)?;
        }
        insert.end().await.map_err(database_error)?;
        Ok(count)
    }

    pub async fn symbols(&self) -> Result<Vec<String>, Status> {
        let _permit = self.permit().await?;
        let rows = self
            .client
            .query("SELECT DISTINCT symbol FROM snapshots ORDER BY symbol LIMIT 1001")
            .fetch_all::<SymbolRow>()
            .await
            .map_err(database_error)?;
        if rows.len() > 1000 {
            return Err(Status::resource_exhausted(
                "symbol directory exceeds 1000 entries",
            ));
        }
        Ok(rows.into_iter().map(|row| row.symbol).collect())
    }

    pub async fn latest(&self, symbol: &str) -> Result<StoredSnapshot, Status> {
        validate_symbol(symbol)?;
        let _permit = self.permit().await?;
        self.client.query("SELECT ?fields FROM snapshots FINAL WHERE symbol = ? ORDER BY timestamp_ms DESC, sequence DESC LIMIT 1").bind(symbol).fetch_optional::<StoredSnapshot>().await.map_err(database_error)?.ok_or_else(|| Status::not_found("symbol has no snapshots"))
    }

    pub async fn snapshots(
        &self,
        range: &pb::RangeRequest,
    ) -> Result<(Vec<StoredSnapshot>, bool), Status> {
        let limit = validate_range(range)?;
        let _permit = self.permit().await?;
        let mut rows = self.client.query("SELECT ?fields FROM snapshots FINAL WHERE symbol = ? AND timestamp_ms >= ? AND timestamp_ms < ? ORDER BY timestamp_ms, sequence LIMIT ?")
            .bind(&range.symbol).bind(range.from_ms).bind(range.to_ms).bind(u64::from(limit) + 1)
            .fetch_all::<StoredSnapshot>().await.map_err(database_error)?;
        let truncated = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        Ok((rows, truncated))
    }

    pub async fn range(&self, symbol: &str) -> Result<pb::RangeReply, Status> {
        validate_symbol(symbol)?;
        let _permit = self.permit().await?;
        let row = self.client.query("SELECT min(timestamp_ms) AS first_ms, max(timestamp_ms) AS last_ms, count() AS count FROM snapshots FINAL WHERE symbol = ?")
            .bind(symbol).fetch_one::<RangeRow>().await.map_err(database_error)?;
        if row.count == 0 {
            return Err(Status::not_found("symbol has no snapshots"));
        }
        Ok(pb::RangeReply {
            first_ms: row.first_ms,
            last_ms: row.last_ms,
            count: row.count,
        })
    }

    pub async fn mid_prices(&self, range: &pb::RangeRequest) -> Result<pb::MidPricesReply, Status> {
        let (rows, truncated) = self.snapshots(range).await?;
        let points = rows
            .iter()
            .map(|row| {
                Ok(pb::MidPricePoint {
                    timestamp_ms: row.timestamp_ms,
                    sequence: row.sequence,
                    price: summary(row)?.mid_price,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        Ok(pb::MidPricesReply { points, truncated })
    }
}
