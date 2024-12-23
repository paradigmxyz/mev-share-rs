//! Simulation queue example

use futures_util::StreamExt;

use alloy::{
    providers::{Provider, ProviderBuilder},
    rpc::types::mev::{Inclusion, SendBundleRequest},
};
use jsonrpsee::http_client::HttpClientBuilder;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use mev_share_backend::{BundleSimulatorService, RpcSimulator};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();

    let eth_rpc_url = std::env::var("ETH_RPC_URL").expect("ETH_RPC_URL must be set");
    // Set up the rpc client
    let url = "https://relay.flashbots.net:443";
    let client = HttpClientBuilder::default().build(url).expect("Failed to create http client");
    let sim_client = RpcSimulator::new(client);

    let eth_client = ProviderBuilder::new().on_http(eth_rpc_url.parse().unwrap());

    let current_block = eth_client.get_block_number().await.expect("could not get block number");

    let sim = BundleSimulatorService::new(current_block, sim_client, Default::default());

    let handle = sim.handle();

    // subscribe to all bundle simulation results
    let mut sim_results = handle.events().results();

    // spawn the simulation service
    let sim = tokio::task::spawn(sim);

    handle
        .add_bundle_simulation_with_prio(
            SendBundleRequest {
                protocol_version: Default::default(),
                inclusion: Inclusion::at_block(current_block - 1),
                bundle_body: vec![],
                validity: None,
                privacy: None,
            },
            Default::default(),
            Default::default(),
        )
        .await
        .unwrap();

    let result = sim_results.next().await.unwrap();

    dbg!(&result);

    sim.await.unwrap();
}
