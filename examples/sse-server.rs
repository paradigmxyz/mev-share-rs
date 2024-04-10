//! SSE server example
use alloy_primitives::B256;
use futures_util::StreamExt;
use hyper::{service::make_service_fn, Server};
use mev_share_sse::{Event, EventClient};
use std::net::SocketAddr;
use tokio::sync::mpsc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use mev_share_sse::server::{SseBroadcastService, SseBroadcaster};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let (tx, _rx) = tokio::sync::broadcast::channel(1000);
    let broadcaster = SseBroadcaster::new(tx);

    let b = broadcaster.clone();

    let svc = SseBroadcastService::new(move || b.ready_stream());

    let make_svc = make_service_fn(move |_| {
        let svc = svc.clone();
        async { Ok::<_, hyper::Error>(svc) }
    });

    let server = Server::bind(&addr).serve(make_svc);

    tracing::debug!("listening on {}", addr);

    let server = tokio::spawn(server);

    let (tx, mut rx) = mpsc::unbounded_channel();
    let client = EventClient::default();
    let mut stream = client.events(&format!("http://{addr}")).await.unwrap();
    tokio::spawn(async move {
        println!("Subscribed to {}", stream.endpoint());
        while let Some(event) = stream.next().await {
            let event = event.unwrap();
            println!("Received event from server: {:?}", event);
            let _ = tx.send(event);
        }
    });

    let event = Event { hash: B256::ZERO, transactions: vec![], logs: vec![] };

    broadcaster.send(&event).unwrap();

    let received = rx.recv().await.unwrap();
    assert_eq!(event, received);
    println!("Matching response");

    server.await??;
    Ok(())
}
