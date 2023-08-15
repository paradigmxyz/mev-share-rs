use crate::{BundleStats, UserStats};
use ethers_core::types::{H256, U64};

// re-export the rpc server trait
#[cfg(feature = "server")]
pub use rpc::FlashbotsApiServer;

/// Generates a client using [jsonrpsee_proc_macros].
///
/// This hides the generated client trait which is replaced by the [`super::FlashbotsApiClient`]
/// trait.
///
/// [jsonrpsee_proc_macros]: https://docs.rs/jsonrpsee-proc-macros/0.20.0/jsonrpsee_proc_macros/attr.rpc.html
mod rpc {
    use crate::{BundleStats, UserStats};
    use ethers_core::types::{H256, U64};
    use jsonrpsee::proc_macros::rpc;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct GetUserStatsRequest {
        pub block_number: U64,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]

    pub struct GetBundleStatsRequest {
        pub bundle_hash: H256,
        pub block_number: U64,
    }

    /// Flashbots RPC interface.
    #[cfg_attr(not(feature = "client"), rpc(server, namespace = "flashbots"))]
    #[cfg_attr(
        all(feature = "client", feature = "server"),
        rpc(server, client, namespace = "flashbots")
    )]
    #[cfg_attr(not(feature = "server"), rpc(client, namespace = "flashbots"))]
    #[async_trait::async_trait]
    pub trait FlashbotsApi {
        /// See [`super::FlashbotsApiClient::get_user_stats`]
        #[method(name = "getUserStatsV2")]
        async fn get_user_stats(
            &self,
            request: GetUserStatsRequest,
        ) -> jsonrpsee::core::RpcResult<UserStats>;

        /// See [`super::FlashbotsApiClient::get_user_stats`]
        #[method(name = "getBundleStatsV2")]
        async fn get_bundle_stats(
            &self,
            request: GetBundleStatsRequest,
        ) -> jsonrpsee::core::RpcResult<BundleStats>;
    }
}

/// An object safe version of the `FlashbotsApiClient` trait.
#[cfg(feature = "client")]
#[async_trait::async_trait]
pub trait FlashbotsApiClient {
    /// Returns a quick summary of how a searcher is performing in the Flashbots ecosystem,
    /// including their reputation-based priority.
    ///
    /// Note: It is currently updated once every hour.
    ///
    /// # Arguments
    ///
    /// * `block_number` - A recent block number, in order to prevent replay attacks. Must be within
    ///   20 blocks of the current chain tip.
    async fn get_user_stats(&self, block_number: U64) -> Result<UserStats, jsonrpsee::core::Error>;

    /// Returns stats for a single bundle.
    ///
    /// You must provide a blockNumber and the bundleHash, and the signing address must be the
    /// same as the one who submitted the bundle.
    ///
    /// # Arguments
    ///
    /// * `block_hash` - Returned by the Flashbots API when calling
    ///   `eth_sendBundle`/`mev_sendBundle`.  [`crate::SendBundleResponse`].
    /// * `block_number` - The block number the bundle was targeting. See [`crate::Inclusion`].
    async fn get_bundle_stats(
        &self,
        bundle_hash: H256,
        block_number: U64,
    ) -> Result<BundleStats, jsonrpsee::core::Error>;
}

#[cfg(feature = "client")]
#[async_trait::async_trait]
impl<T> FlashbotsApiClient for T
where
    T: rpc::FlashbotsApiClient + Sync,
{
    /// See [`FlashbotsApiClient::get_user_stats`]
    async fn get_user_stats(&self, block_number: U64) -> Result<UserStats, jsonrpsee::core::Error> {
        self.get_user_stats(rpc::GetUserStatsRequest { block_number }).await
    }

    /// See [`FlashbotsApiClient::get_user_stats`]
    async fn get_bundle_stats(
        &self,
        bundle_hash: H256,
        block_number: U64,
    ) -> Result<BundleStats, jsonrpsee::core::Error> {
        self.get_bundle_stats(rpc::GetBundleStatsRequest { bundle_hash, block_number }).await
    }
}

#[cfg(all(test, feature = "client"))]
mod tests {
    use super::*;
    use crate::FlashbotsSignerLayer;
    use ethers_core::rand::thread_rng;
    use ethers_signers::LocalWallet;
    use jsonrpsee::http_client::{transport, HttpClientBuilder};

    struct Client {
        inner: Box<dyn FlashbotsApiClient>,
    }

    #[allow(dead_code)]
    async fn assert_flashbots_api_box() {
        let fb_signer = LocalWallet::new(&mut thread_rng());
        let http = HttpClientBuilder::default()
            .set_middleware(
                tower::ServiceBuilder::new()
                    .map_err(transport::Error::Http)
                    .layer(FlashbotsSignerLayer::new(fb_signer.clone())),
            )
            .build("http://localhost:3030")
            .unwrap();
        let client = Client { inner: Box::new(http) };
        client.inner.get_user_stats(Default::default()).await.unwrap();
    }
}
