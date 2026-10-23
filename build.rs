fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = tonic_prost_build::configure()
        .build_client(true)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(
            ".",
            "#[serde(default, rename_all = \"camelCase\", deny_unknown_fields)]",
        );
    for field in [
        "Level.price",
        "Level.quantity",
        "Snapshot.timestamp_ms",
        "Snapshot.sequence",
        "IngestReply.accepted",
        "RangeRequest.from_ms",
        "RangeRequest.to_ms",
        "SummaryReply.timestamp_ms",
        "SummaryReply.best_bid",
        "SummaryReply.best_ask",
        "SummaryReply.spread",
        "SummaryReply.bid_quantity",
        "SummaryReply.ask_quantity",
        "RangeReply.first_ms",
        "RangeReply.last_ms",
        "RangeReply.count",
        "MidPricePoint.timestamp_ms",
        "MidPricePoint.sequence",
    ] {
        builder = builder.field_attribute(
            format!("market.v1.{field}"),
            "#[serde(with = \"crate::wire::uint64\")]",
        );
    }
    builder.compile_protos(&["proto/market.proto"], &["proto"])?;
    println!("cargo:rerun-if-changed=proto/market.proto");
    Ok(())
}
