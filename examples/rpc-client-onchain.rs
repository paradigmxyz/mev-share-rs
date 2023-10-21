//! Basic RPC api example

use alloy_primitives::{Bytes, B256, U64};
use anyhow::Result;
use ethers_core::{
    rand::thread_rng,
    types::{transaction::eip2718::TypedTransaction, Chain, Eip1559TransactionRequest},
};
use ethers_middleware::MiddlewareBuilder;
use ethers_providers::{Middleware, Provider};
use ethers_signers::{LocalWallet, Signer};
use jsonrpsee::{
    core::async_trait,
    http_client::{transport::Error as HttpError, HttpClientBuilder},
};
use mev_share_rpc_api::{
    BundleItem, FlashbotsApiClient, FlashbotsSignerLayer, Inclusion, MevApiClient, Privacy,
    PrivacyHint, PrivateTransactionPreferences, PrivateTransactionRequest, SendBundleRequest,
};
use std::{env, str::FromStr};
use tokio::time::Duration;
use tower::ServiceBuilder;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Sends a transaction to the Goerli mempool and send a request to the Goerli MEV-wShare deployment
/// to backrun it.
///
/// # Usage
///
/// Run:
///
/// ```sh
/// export ETH_RPC_URL="..."
/// export SENDER_PRIVATE_KEY="..."
/// cargo run --example rpc-client-onchain
/// ```
///
/// where:
///
/// - `ETH_RPC_URL` is a Goerli RPC provider (that needs to answer to `eth_getBlockNumber`
/// requests in order to get the target block for our bundle)
/// - `SENDER_PRIVATE_KEY` is the private key of a wallet with some ETH on Goerli to send
///   transactions.
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();

    // The signer used to authenticate bundles
    let fb_signer = LocalWallet::new(&mut thread_rng());

    // The signer used to sign our transactions
    let tx_signer =
        LocalWallet::from_str(&env::var("SENDER_PRIVATE_KEY")?)?.with_chain_id(Chain::Goerli);

    // Set up flashbots-style auth middleware
    let signing_middleware = FlashbotsSignerLayer::new(fb_signer);
    let service_builder = ServiceBuilder::new()
        // map signer errors to http errors
        .map_err(HttpError::Http)
        .layer(signing_middleware);

    // Set up the rpc client
    let url = "https://relay-goerli.flashbots.net:443";
    let client = HttpClientBuilder::default().set_middleware(service_builder).build(url)?;

    // Set up the eth client
    let eth_rpc_url = std::env::var("ETH_RPC_URL").expect("ETH_RPC_URL must be set");
    let eth_client = Provider::try_from(eth_rpc_url)?
        .with_signer(tx_signer.clone())
        .nonce_manager(tx_signer.address());

    // Set up a private tx we will try to backrun
    let tx_to_backrun = Eip1559TransactionRequest::new()
        .to(tx_signer.address())
        .value(100)
        .data(b"im backrunniiiiiing")
        .fill(&eth_client)
        .await?
        .sign(&tx_signer)
        .await?;

    // Build private tx request
    let private_tx = PrivateTransactionRequest {
        tx: tx_to_backrun,
        max_block_number: None,
        preferences: PrivateTransactionPreferences {
            privacy: Some(Privacy {
                hints: Some(
                    PrivacyHint::default()
                        .with_calldata()
                        .with_contract_address()
                        .with_function_selector(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        },
    };

    // Send a private tx
    let tx_to_backrun_hash =
        mev_share_rpc_api::EthBundleApiClient::send_private_transaction(&client, private_tx)
            .await?;

    println!("Sent tx to backrun, hash: {tx_to_backrun_hash:?}");

    // Our own tx that we want to include in the bundle
    let backrun_tx = Eip1559TransactionRequest::new()
        .to(tx_signer.address())
        .value(100)
        .data(b"im backrunniiiiiing")
        .fill(&eth_client)
        .await?
        .sign(&tx_signer)
        .await?;

    // Build bundle
    let current_block = eth_client.get_block_number().await?;
    let target_block = current_block + 2;
    let bundle_body = vec![
        BundleItem::Hash { hash: tx_to_backrun_hash },
        BundleItem::Tx { tx: backrun_tx, can_revert: false },
    ];
    let bundle_request = SendBundleRequest {
        protocol_version: mev_share_rpc_api::ProtocolVersion::V0_1,
        bundle_body,
        inclusion: Inclusion { block: target_block, ..Default::default() },
        ..Default::default()
    };

    // Send bundle
    let bundle_send_res = client.send_bundle(bundle_request.clone()).await?;
    println!("`mev_sendBundle`: {bundle_send_res:?}");

    let wait_duration = Duration::from_secs(10);
    println!("checking bundle stats in {wait_duration:?}...");
    tokio::time::sleep(wait_duration).await;

    // Get bundle stats
    let bundle_stats = client.get_bundle_stats(B256::random(), target_block).await;
    println!("`flashbots_getBundleStatsV2`: {:?}", bundle_stats);

    // Get user stats
    let user_stats = client.get_user_stats(U64::from(current_block.as_u64())).await;
    println!("`flashbots_getUserStatsV2`: {:?}", user_stats);

    Ok(())
}

/* Helpers for building and signing transactions. */

#[async_trait]
trait TxFill {
    async fn fill<P: Middleware>(mut self, eth_client: &P) -> Result<TypedTransaction>;
}

#[async_trait]
impl TxFill for Eip1559TransactionRequest {
    async fn fill<P: Middleware>(mut self, eth_client: &P) -> Result<TypedTransaction> {
        let mut tx: TypedTransaction = self.into();
        eth_client
            .fill_transaction(&mut tx, None)
            .await
            .expect("Failed to fill backrun transaction");
        Ok(tx)
    }
}

#[async_trait]
trait TxSign {
    async fn sign<S: Signer>(&self, signer: &S) -> Result<Bytes>
    where
        S::Error: 'static;
}

#[async_trait]
impl TxSign for TypedTransaction {
    async fn sign<S: Signer>(&self, signer: &S) -> Result<Bytes>
    where
        S::Error: 'static,
    {
        let signature = signer.sign_transaction(self).await?;
        Ok(self.rlp_signed(&signature).0.into())
    }
}
