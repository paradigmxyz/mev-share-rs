//! RPC simulator implementation.

use crate::simulation::{BundleSimulationOutcome, BundleSimulator};
use futures_util::{pin_mut, FutureExt};
use mev_share_rpc_api::{clients::MevApiClient, SendBundleRequest, SimBundleOverrides};
use std::{
    future::Future,
    pin::{pin, Pin},
};

#[derive(Clone, Debug)]
pub struct RpcSimulator<Client> {
    client: Client,
}

impl<Client> RpcSimulator<Client> {
    /// Creates a new RPC simulator.
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

impl<Client> RpcSimulator<Client>
where
    Client: MevApiClient + Unpin + Clone + Send + Sync + 'static,
{
    async fn sim_bundle(
        &self,
        bundle: SendBundleRequest,
        sim_overrides: SimBundleOverrides,
    ) -> BundleSimulationOutcome {
        todo!()
    }
}

impl<Client> BundleSimulator for RpcSimulator<Client>
where
    Client: MevApiClient + Unpin + Clone + Send + Sync + 'static,
{
    type Simulation = Pin<Box<dyn Future<Output = BundleSimulationOutcome> + Send + Sync>>;

    fn simulate_bundle(
        &self,
        bundle: SendBundleRequest,
        sim_overrides: SimBundleOverrides,
    ) -> Self::Simulation {
        let this = self.clone();
        Box::pin(async move { this.sim_bundle(bundle, sim_overrides).await })
    }
}
