#![warn(missing_docs, unreachable_pub, unused_crate_dependencies)]
#![deny(unused_must_use, rust_2018_idioms)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! MEV-Share RPC interface definitions

/// `mev` namespace
mod mev;

/// type bindings 
mod types;
pub use types::*;

/// flashbots-style auth 
mod auth;
pub use auth::*;

/// re-export of all server traits
#[cfg(feature = "server")]
pub use servers::*;

/// Aggregates all server traits.
#[cfg(feature = "server")]
#[doc(hidden)]
pub mod servers {
    pub use crate::mev::MevApiServer;
}

/// re-export of all client traits
#[cfg(feature = "client")]
pub use clients::*;

/// Aggregates all client traits.
#[cfg(feature = "client")]
#[doc(hidden)]
pub mod clients {
    pub use crate::mev::MevApiClient;
}
