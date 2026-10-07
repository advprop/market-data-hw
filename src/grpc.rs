use crate::{AppState, MAX_DEPTH, pb, summary};
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub struct Service(pub Arc<AppState>);

#[tonic::async_trait]
impl pb::market_data_server::MarketData for Service {
    async fn health(&self, _: Request<pb::Empty>) -> Result<Response<pb::HealthReply>, Status> {
        self.0.health().await?;
        Ok(Response::new(pb::HealthReply {
            status: "ok".into(),
        }))
    }

    async fn ingest(
        &self,
        request: Request<pb::IngestRequest>,
    ) -> Result<Response<pb::IngestReply>, Status> {
        self.0.authorize(
            request
                .metadata()
                .get("x-write-token")
                .and_then(|v| v.to_str().ok()),
        )?;
        let accepted = self.0.ingest(request.into_inner().snapshots).await?;
        Ok(Response::new(pb::IngestReply { accepted }))
    }

    async fn list_symbols(
        &self,
        _: Request<pb::Empty>,
    ) -> Result<Response<pb::SymbolsReply>, Status> {
        Ok(Response::new(pb::SymbolsReply {
            symbols: self.0.symbols().await?,
        }))
    }

    async fn get_latest(
        &self,
        request: Request<pb::SymbolRequest>,
    ) -> Result<Response<pb::Snapshot>, Status> {
        Ok(Response::new(
            self.0.latest(&request.into_inner().symbol).await?.into(),
        ))
    }

    async fn get_snapshots(
        &self,
        request: Request<pb::RangeRequest>,
    ) -> Result<Response<pb::SnapshotsReply>, Status> {
        let (rows, truncated) = self.0.snapshots(&request.into_inner()).await?;
        Ok(Response::new(pb::SnapshotsReply {
            snapshots: rows.into_iter().map(Into::into).collect(),
            truncated,
        }))
    }

    async fn get_depth(
        &self,
        request: Request<pb::DepthRequest>,
    ) -> Result<Response<pb::Snapshot>, Status> {
        let request = request.into_inner();
        if request.levels == 0 || request.levels as usize > MAX_DEPTH {
            return Err(Status::invalid_argument("levels must be in 1..100"));
        }
        let mut snapshot = self.0.latest(&request.symbol).await?;
        snapshot.bids.truncate(request.levels as usize);
        snapshot.asks.truncate(request.levels as usize);
        Ok(Response::new(snapshot.into()))
    }

    async fn get_summary(
        &self,
        request: Request<pb::SymbolRequest>,
    ) -> Result<Response<pb::SummaryReply>, Status> {
        Ok(Response::new(summary(
            &self.0.latest(&request.into_inner().symbol).await?,
        )?))
    }

    async fn get_range(
        &self,
        request: Request<pb::SymbolRequest>,
    ) -> Result<Response<pb::RangeReply>, Status> {
        Ok(Response::new(
            self.0.range(&request.into_inner().symbol).await?,
        ))
    }

    async fn get_mid_prices(
        &self,
        request: Request<pb::RangeRequest>,
    ) -> Result<Response<pb::MidPricesReply>, Status> {
        Ok(Response::new(
            self.0.mid_prices(&request.into_inner()).await?,
        ))
    }
}
