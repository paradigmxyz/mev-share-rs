//! # mev-share-rs
//!
//! A client library for MEV-Share written in rust. 

#![warn(missing_docs, unreachable_pub, unused_crate_dependencies)]
#![deny(unused_must_use, rust_2018_idioms)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

#[doc(inline)]
pub use mev_share_rpc_api as rpc;
#[doc(inline)]
pub use mev_share_sse as sse;