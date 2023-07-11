use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use crate::{SendBundleRequest, SendBundleResponse, SimBundleOverrides, SimBundleResponse};

/// Mev rpc interface.
#[cfg_attr(not(feature = "client"), rpc(server, namespace = "mev"))]
#[cfg_attr(all(feature = "client", feature = "server"), rpc(server, client, namespace = "mev"))]
#[cfg_attr(not(feature = "server"), rpc(client, namespace = "mev"))]
#[async_trait::async_trait]
pub trait MevApi {
    /// Submitting bundles to the relay. It takes in a bundle and provides a bundle hash as a return
    /// value.
    #[method(name = "sendBundle")]
    async fn send_bundle(&self, request: SendBundleRequest) -> RpcResult<SendBundleResponse>;

    /// Similar to `mev_sendBundle` but instead of submitting a bundle to the relay, it returns a
    /// simulation result. Only fully matched bundles can be simulated.
    #[method(name = "simBundle")]
    async fn sim_bundle(&self, bundle: SendBundleRequest, sim_overrides: SimBundleOverrides) -> RpcResult<SimBundleResponse>;
}

#[cfg(test)]
mod tests {
    use crate::{BundleItem, FlashbotsSignerLayer, Inclusion};

    use super::*;
    use ethers_core::{types::{TransactionRequest, U64}, rand::thread_rng};
    use ethers_signers::{LocalWallet, Signer};
    use tower::ServiceBuilder;
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    use jsonrpsee::http_client::{HttpClientBuilder, transport::Error as HttpError};

    const RELAY_URL: &str = "https://relay.flashbots.net:443";


    fn init_tracing() {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::from_default_env())
            .init();
    }

    #[tokio::test]
    #[ignore]
    async fn sim_bundle() {
        init_tracing();

        let fb_signer = LocalWallet::new(&mut thread_rng());
        let tx_signer = LocalWallet::new(&mut thread_rng());

        // Set up flashbots-style auth middleware
        let signing_middleware = FlashbotsSignerLayer::new(fb_signer);
        let service_builder = ServiceBuilder::new()
        // map signer errors to http errors
        .map_err(|e| HttpError::Http(e))
        .layer(signing_middleware);

        let client = HttpClientBuilder::default()
            .set_middleware(service_builder)
            .build(RELAY_URL)
            .expect("Failed to create http client");

        let tx = TransactionRequest::pay("vitalik.eth", 100);
        let signature = tx_signer.sign_transaction(&tx.clone().into()).await.unwrap();
        let bytes = tx.rlp_signed(&signature);

        let bundle_body = vec![BundleItem::Tx { tx: bytes, can_revert: false }];
        let inclusion = Inclusion {
            max_block: None, 
            block: U64::from(17671777)
        };
        let inc = inclusion.clone();
        let sim_request = SendBundleRequest { bundle_body, inclusion, ..Default::default() };
        println!("Sending bundle: {:?}", sim_request);
        println!("inlcusion: {:?}", inc);
        let resp = client.sim_bundle(sim_request, Default::default()).await;
        println!("Got a bundle response: {:?}", resp);
    }
}
