//! Basic SSE example
use futures_util::StreamExt;
use mev_share_rs::EventClient;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();

    let mainnet_sse = "https://mev-share.flashbots.net";
    let client = EventClient::default();
    let mut stream = client.events(mainnet_sse).await.unwrap();
    println!("Subscribed to {}", stream.endpoint());

    while let Some(event) = stream.next().await {
        dbg!(&event);
    }
}
