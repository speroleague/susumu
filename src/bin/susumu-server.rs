#[cfg(feature = "server")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    susumu::server::run().await
}

#[cfg(not(feature = "server"))]
fn main() {
    eprintln!("susumu-server requires the `server` feature");
    std::process::exit(2);
}
