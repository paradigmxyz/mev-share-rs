use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use crate::{SendBundleRequest, SendBundleResponse, SimBundleOptions, SimBundleResponse};

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
    async fn sim_bundle(&self, bundle: SendBundleRequest, sim_options: SimBundleOptions) -> RpcResult<SimBundleResponse>;
}
