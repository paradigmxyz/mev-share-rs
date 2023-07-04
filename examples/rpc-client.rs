//! Basic RPC api example

use std::sync::Arc;

use jsonrpsee::http_client::HttpClientBuilder;
use mev_share_rpc_api::{FlashbotsSignerLayer, BundleItem, SendBundleRequest, MevApiClient};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use tower::ServiceBuilder;

use ethers_core::{rand::thread_rng, types::{H256, TransactionRequest}};
use ethers_signers::{LocalWallet, Signer};




#[tokio::main]
async fn main() {
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();

    // The signer used to authenticate bundles
    let fb_signer = LocalWallet::new(&mut thread_rng());

    // The signer used to sign our transactions
    let tx_signer = LocalWallet::new(&mut thread_rng());

    // Set up flashbots-style auth middleware
    let signing_middleware = FlashbotsSignerLayer::new(Arc::new(fb_signer));
    let service_builder = ServiceBuilder::new().layer(signing_middleware);

    // Set up the rpc client 
    let url = "https://relay.flashbots.net:443";
    let client = HttpClientBuilder::default()
        .set_middleware(service_builder)
        .build(url)
        .expect("Failed to create http client");

    // Hash of the transaction we are trying to backrun
    let tx_hash = H256::random();

    // Our own tx that we want to include in the bundle
    let tx = TransactionRequest::pay("vitalik.eth", 100);
    let signature = tx_signer.sign_transaction(&tx.clone().into()).await.unwrap();
    let bytes = tx.rlp_signed(&signature);

    // Build bundle
    let mut bundle_body = Vec::new();
    bundle_body.push(BundleItem::TxHash { hash: tx_hash });
    bundle_body.push(BundleItem::Tx {
        tx: bytes,
        can_revert: false,
    });

    let bundle = SendBundleRequest {
        protocol_version: Default::default(),
        inclusion_predicate: Default::default(),
        bundle_body,
    };

    // Send bundle
    let resp = client.send_bundle(bundle).await;
    println!("Got a bundle response: {:?}", resp);

}
