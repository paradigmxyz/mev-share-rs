//! Basic SSE example
use futures_util::StreamExt;
use mev_share_rs::EventClient;

#[tokio::main]
async fn main() {
    let mainnet_sse = "https://mev-share.flashbots.net";
    let client = EventClient::default();
    let mut stream = client.events(mainnet_sse).await.unwrap();
    println!("Subscribed to {}", stream.endpoint());

    while let Some(event) = stream.next().await {
        dbg!(&event);
    }
}
