#![warn(missing_docs, unreachable_pub, unused_crate_dependencies)]
#![deny(unused_must_use, rust_2018_idioms)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! MEV-Share simulation backend abstractions.

use alloy::rpc::types::mev::{SendBundleRequest, SimBundleOverrides, SimBundleResponse};
use std::{error::Error, future::Future};

mod rpc;
mod service;
pub use rpc::RpcSimulator;
pub use service::*;

/// A type that can start a bundle simulation.
pub trait BundleSimulator: Unpin + Send + Sync {
    /// An in progress bundle simulation.
    type Simulation: Future<Output = BundleSimulationOutcome> + Unpin + Send;

    /// Starts a bundle simulation.
    fn simulate_bundle(
        &self,
        bundle: SendBundleRequest,
        sim_overrides: SimBundleOverrides,
    ) -> Self::Simulation;
}

/// Errors that can occur when simulating a bundle.
#[derive(Debug)]
pub enum BundleSimulationOutcome {
    /// The simulation was successful.
    Success(SimBundleResponse),
    /// The simulation failed and is not recoverable.
    Fatal(Box<dyn Error + Send + Sync>),
    /// The simulation failed and should be rescheduled.
    Reschedule(Box<dyn Error + Send + Sync>),
}

/// A simulated bundle.
#[derive(Debug, Clone)]
pub struct SimulatedBundle {
    /// The request object that was used for simulation.
    pub request: SendBundleRequest,
    /// The overrides that were used for simulation.
    pub overrides: SimBundleOverrides,
    /// The response from the simulation.simulation
    pub response: SimBundleResponse,
    /// The number of retries that were used for simulation.
    pub retries: usize,
}

impl SimulatedBundle {
    /// Returns true if the simulation was successful.
    pub fn is_success(&self) -> bool {
        self.response.success
    }

    /// Returns the profit of the simulation.
    pub fn profit(&self) -> u64 {
        self.response.profit.to()
    }

    /// Returns the gas used by the simulation.
    pub fn gas_used(&self) -> u64 {
        self.response.gas_used
    }

    /// Returns the mev gas price of the simulation.
    pub fn mev_gas_price(&self) -> u64 {
        self.response.mev_gas_price.to()
    }

    ///
    pub fn extract_hints(&self) {}
}
