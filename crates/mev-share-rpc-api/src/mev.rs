use crate::{SendBundleRequest, SendBundleResponse, SimBundleOverrides, SimBundleResponse};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

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
    async fn sim_bundle(
        &self,
        bundle: SendBundleRequest,
        sim_overrides: SimBundleOverrides,
    ) -> RpcResult<SimBundleResponse>;
}

/// An object safe version of the `MevApiClient` trait.
#[async_trait::async_trait]
pub(crate) trait MevApiExt {
    /// Submitting bundles to the relay. It takes in a bundle and provides a bundle hash as a return
    /// value.
    async fn send_bundle(
        &self,
        request: SendBundleRequest,
    ) -> Result<SendBundleResponse, jsonrpsee::core::Error>;

    /// Similar to `mev_sendBundle` but instead of submitting a bundle to the relay, it returns a
    /// simulation result. Only fully matched bundles can be simulated.
    async fn sim_bundle(
        &self,
        bundle: SendBundleRequest,
        sim_overrides: SimBundleOverrides,
    ) -> Result<SimBundleResponse, jsonrpsee::core::Error>;
}

#[async_trait::async_trait]
impl<T> MevApiExt for T
where
    T: MevApiClient + Sync,
{
    async fn send_bundle(
        &self,
        request: SendBundleRequest,
    ) -> Result<SendBundleResponse, jsonrpsee::core::Error> {
        MevApiClient::send_bundle(self, request).await
    }

    async fn sim_bundle(
        &self,
        bundle: SendBundleRequest,
        sim_overrides: SimBundleOverrides,
    ) -> Result<SimBundleResponse, jsonrpsee::core::Error> {
        MevApiClient::sim_bundle(self, bundle, sim_overrides).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FlashbotsSignerLayer;
    use ethers_core::rand::thread_rng;
    use ethers_signers::LocalWallet;
    use jsonrpsee::http_client::{transport, HttpClientBuilder};

    struct Client {
        inner: Box<dyn MevApiExt>,
    }

    #[allow(dead_code)]
    async fn assert_mev_api_box() {
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
        client
            .inner
            .send_bundle(SendBundleRequest {
                protocol_version: Default::default(),
                inclusion: Default::default(),
                bundle_body: vec![],
                validity: None,
                privacy: None,
            })
            .await
            .unwrap();
    }
}
