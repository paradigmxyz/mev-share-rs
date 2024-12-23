#![warn(missing_docs, unreachable_pub, unused_crate_dependencies)]
#![deny(unused_must_use, rust_2018_idioms)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

//! Rust client implementation for the [MEV-share protocol](https://github.com/flashbots/mev-share)

pub mod client;
pub use client::EventClient;

#[cfg(feature = "server")]
pub mod server;
