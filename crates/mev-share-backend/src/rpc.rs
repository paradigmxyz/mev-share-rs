//! RPC simulator implementation.

use crate::{BundleSimulationOutcome, BundleSimulator};
use mev_share_rpc_api::{clients::MevApiClient, jsonrpsee, SendBundleRequest, SimBundleOverrides};
use std::{fmt, fmt::Formatter, future::Future, pin::Pin, sync::Arc};

/// A [BundleSimulator] that sends bundles via RPC, see also: [MevApiClient].
#[derive(Clone)]
pub struct RpcSimulator<Client> {
    client: Client,
    is_err_recoverable:
        Arc<dyn Fn(&jsonrpsee::core::Error) -> bool + Unpin + Send + Sync + 'static>,
}

impl<Client> RpcSimulator<Client> {
    /// Creates a new RPC simulator.
    pub fn new(client: Client) -> Self {
        Self::with_recoverable_fn(client, is_nonce_too_low)
    }

    /// Creates a new RPC simulator with a custom recoverable error function.
    ///
    /// By default the error is recoverable if it contains the "nonce too low" error.
    pub fn with_recoverable_fn<F>(client: Client, f: F) -> Self
    where
        F: Fn(&jsonrpsee::core::Error) -> bool + Unpin + Send + Sync + 'static,
    {
        Self { client, is_err_recoverable: Arc::new(f) }
    }
}

impl<Client> BundleSimulator for RpcSimulator<Client>
where
    Client: MevApiClient + Unpin + Clone + Send + Sync + 'static,
{
    type Simulation = Pin<Box<dyn Future<Output = BundleSimulationOutcome> + Send>>;

    fn simulate_bundle(
        &self,
        bundle: SendBundleRequest,
        sim_overrides: SimBundleOverrides,
    ) -> Self::Simulation {
        let this = self.clone();
        Box::pin(async move {
            match this.client.sim_bundle(bundle, sim_overrides).await {
                Ok(res) => BundleSimulationOutcome::Success(res),
                Err(err) => {
                    if (this.is_err_recoverable)(&err) {
                        BundleSimulationOutcome::Reschedule(Box::new(err))
                    } else {
                        BundleSimulationOutcome::Fatal(Box::new(err))
                    }
                }
            }
        })
    }
}

impl<C> fmt::Debug for RpcSimulator<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RpcSimulator")
            .field("client", &"...")
            .field("is_err_recoverable", &"...")
            .finish()
    }
}

/// whether the error message contains a nonce too low error
///
/// This is the default recoverable error for RPC simulator
pub(crate) fn is_nonce_too_low(err: &jsonrpsee::core::Error) -> bool {
    err.to_string().contains("nonce too low")
}
