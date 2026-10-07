use market_data::{
    AppState, StoredSnapshot, http::render_svg, pb, summary, validate_range, validate_symbol,
};
use tonic::Code;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn snapshot() -> pb::Snapshot {
    pb::Snapshot {
        symbol: "BTC-USDT".into(),
        timestamp_ms: 1000,
        sequence: 1,
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

#[test]
fn sorts_levels_and_computes_l2_metrics() -> TestResult {
    let row = StoredSnapshot::try_from(snapshot())?;
    assert_eq!(
        row.bids,
        vec![(100_000_000, 1_000_000), (99_000_000, 2_000_000)]
    );
    assert_eq!(
        row.asks,
        vec![(101_000_000, 1_000_000), (102_000_000, 1_000_000)]
    );
    let metrics = summary(&row)?;
    assert_eq!(metrics.spread, 1_000_000);
    assert_eq!(metrics.mid_price, 100.5);
    assert_eq!(metrics.bid_quantity, 3_000_000);
    assert!((metrics.imbalance - 0.2).abs() < 1e-12);
    let restored: pb::Snapshot = row.into();
    assert_eq!(restored.bids.first().map(|v| v.price), Some(100_000_000));
    Ok(())
}

#[test]
fn rejects_invalid_books() {
    let mut books = Vec::new();
    let mut book = snapshot();
    book.bids.clear();
    books.push(book);
    let mut book = snapshot();
    book.asks = vec![pb::Level {
        price: 99_000_000,
        quantity: 1,
    }];
    books.push(book);
    let mut book = snapshot();
    book.bids.push(pb::Level {
        price: 100_000_000,
        quantity: 2,
    });
    books.push(book);
    let mut book = snapshot();
    book.asks = vec![pb::Level {
        price: 101_000_000,
        quantity: 0,
    }];
    books.push(book);
    let mut book = snapshot();
    book.bids = vec![
        pb::Level {
            price: 1,
            quantity: 1
        };
        101
    ];
    books.push(book);
    let mut book = snapshot();
    book.timestamp_ms = 0;
    books.push(book);
    let mut book = snapshot();
    book.timestamp_ms = u64::MAX;
    books.push(book);
    let mut book = snapshot();
    book.asks = vec![pb::Level {
        price: u64::MAX,
        quantity: 1,
    }];
    books.push(book);
    for book in books {
        assert!(
            matches!(StoredSnapshot::try_from(book), Err(e) if e.code() == Code::InvalidArgument)
        );
    }
}

#[test]
fn validates_symbols_and_ranges() -> TestResult {
    for symbol in ["", "btc", "BTC/USDT", "<script>", "БТК", &"A".repeat(33)] {
        assert!(validate_symbol(symbol).is_err());
    }
    validate_symbol("BTC-USDT")?;
    let mut range = pb::RangeRequest {
        symbol: "BTC-USDT".into(),
        from_ms: 0,
        to_ms: 2000,
        limit: 0,
    };
    assert_eq!(validate_range(&range)?, 100);
    range.limit = 101;
    assert!(validate_range(&range).is_err());
    range.limit = 10;
    range.to_ms = 0;
    assert!(validate_range(&range).is_err());
    Ok(())
}

#[test]
fn svg_handles_constant_single_and_irregular_series() -> TestResult {
    let points = vec![
        pb::MidPricePoint {
            timestamp_ms: 1000,
            sequence: 1,
            price: 100.5,
        },
        pb::MidPricePoint {
            timestamp_ms: 2000,
            sequence: 2,
            price: 100.5,
        },
        pb::MidPricePoint {
            timestamp_ms: 11_000,
            sequence: 3,
            price: 100.5,
        },
    ];
    let svg = render_svg("BTC-USDT", &points)?;
    assert!(svg.contains("158.00,225.00"));
    assert!(!svg.contains("NaN"));
    assert!(svg.contains("BTC-USDT L2 mid-price"));
    assert!(render_svg("BTC-USDT", &points[..1])?.contains("470.00,225.00"));
    assert!(render_svg("BTC-USDT", &[]).is_err());
    assert!(render_svg("<script>", &points).is_err());
    Ok(())
}

#[test]
fn rejects_corrupt_stored_books_without_panicking() {
    let row = StoredSnapshot {
        symbol: "BTC-USDT".into(),
        timestamp_ms: 1,
        sequence: 1,
        bids: vec![],
        asks: vec![],
    };
    assert!(matches!(summary(&row), Err(e) if e.code() == Code::DataLoss));
    let row = StoredSnapshot {
        bids: vec![(10, u64::MAX), (9, 1)],
        asks: vec![(11, 1)],
        ..row
    };
    assert!(matches!(summary(&row), Err(e) if e.code() == Code::DataLoss));
}

#[tokio::test]
async fn validates_entire_batch_before_database_access() {
    let state = AppState::new(
        clickhouse::Client::default().with_url("http://127.0.0.1:1"),
        "test-token".into(),
    );
    let mut invalid = snapshot();
    invalid.bids.clear();
    assert!(
        matches!(state.ingest(vec![snapshot(), invalid]).await, Err(e) if e.code() == Code::InvalidArgument)
    );
    assert!(
        matches!(state.ingest(vec![snapshot(), snapshot()]).await, Err(e) if e.code() == Code::InvalidArgument)
    );
    assert!(state.authorize(None).is_err());
    assert!(state.authorize(Some("wrong-token")).is_err());
    assert!(state.authorize(Some("test-token")).is_ok());
}
