//! Basic SSE example

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();

    let url = "http://localhost:8545";
    let _client = jsonrpsee::http_client::HttpClientBuilder::default()
        .build(url)
        .expect("Failed to create http client");
}
