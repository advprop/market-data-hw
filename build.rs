fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_client(true)
        .compile_protos(&["proto/market.proto"], &["proto"])?;
    println!("cargo:rerun-if-changed=proto/market.proto");
    Ok(())
}
