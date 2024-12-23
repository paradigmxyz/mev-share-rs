use alloy::rpc::types::mev::{
    SendBundleRequest, SendBundleResponse, SimBundleOverrides, SimBundleResponse,
};

// re-export the rpc server trait
#[cfg(feature = "server")]
pub use rpc::MevApiServer;

/// jsonrpsee generated code.
///
/// This hides the generated client trait which is replaced by the `MevApiClient` trait.
mod rpc {
    use alloy::rpc::types::mev::{
        SendBundleRequest, SendBundleResponse, SimBundleOverrides, SimBundleResponse,
    };
    use jsonrpsee::proc_macros::rpc;

    /// Mev rpc interface.
    #[cfg_attr(not(feature = "client"), rpc(server, namespace = "mev"))]
    #[cfg_attr(all(feature = "client", feature = "server"), rpc(server, client, namespace = "mev"))]
    #[cfg_attr(not(feature = "server"), rpc(client, namespace = "mev"))]
    #[async_trait::async_trait]
    pub trait MevApi {
        /// Submitting bundles to the relay. It takes in a bundle and provides a bundle hash as a
        /// return value.
        #[method(name = "sendBundle")]
        async fn send_bundle(
            &self,
            request: SendBundleRequest,
        ) -> jsonrpsee::core::RpcResult<SendBundleResponse>;

        /// Similar to `mev_sendBundle` but instead of submitting a bundle to the relay, it returns
        /// a simulation result. Only fully matched bundles can be simulated.
        #[method(name = "simBundle")]
        async fn sim_bundle(
            &self,
            bundle: SendBundleRequest,
            sim_overrides: SimBundleOverrides,
        ) -> jsonrpsee::core::RpcResult<SimBundleResponse>;
    }
}

/// An object safe version of the `MevApiClient` trait.
#[cfg(feature = "client")]
#[async_trait::async_trait]
pub trait MevApiClient {
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

#[cfg(feature = "client")]
#[async_trait::async_trait]
impl<T> MevApiClient for T
where
    T: rpc::MevApiClient + Sync,
{
    async fn send_bundle(
        &self,
        request: SendBundleRequest,
    ) -> Result<SendBundleResponse, jsonrpsee::core::Error> {
        rpc::MevApiClient::send_bundle(self, request).await
    }

    async fn sim_bundle(
        &self,
        bundle: SendBundleRequest,
        sim_overrides: SimBundleOverrides,
    ) -> Result<SimBundleResponse, jsonrpsee::core::Error> {
        rpc::MevApiClient::sim_bundle(self, bundle, sim_overrides).await
    }
}

#[cfg(all(test, feature = "client"))]
mod tests {
    use super::*;
    use crate::FlashbotsSignerLayer;
    use alloy::signers::local::PrivateKeySigner;
    use jsonrpsee::http_client::{transport, HttpClientBuilder};

    struct Client {
        inner: Box<dyn MevApiClient>,
    }

    #[allow(dead_code)]
    async fn assert_mev_api_box() {
        let fb_signer: PrivateKeySigner =
            "4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318".parse().unwrap();
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
