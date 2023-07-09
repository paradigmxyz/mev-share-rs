use jsonrpsee::{core::RpcResult, proc_macros::rpc};

/// Eth bundle rpc interface.
///
/// See also <https://docs.flashbots.net/flashbots-auction/searchers/advanced/rpc-endpoint>
#[cfg_attr(not(feature = "client"), rpc(server, namespace = "eth"))]
#[cfg_attr(all(feature = "client", feature = "server"), rpc(server, client, namespace = "eth"))]
#[cfg_attr(not(feature = "server"), rpc(client, namespace = "eth"))]
#[async_trait::async_trait]
pub trait EthBundleApi {
    /// `eth_sendBundle` can be used to send your bundles to the builder.
    #[method(name = "sendBundle")]
    async fn send_bundle(&self, request: ()) -> RpcResult<()>;

    /// `eth_callBundle` can be used to simulate a bundle against a specific block number, including
    /// simulating a bundle at the top of the next block.
    #[method(name = "callBundle")]
    async fn call_bundle(&self, request: ()) -> RpcResult<()>;

    /// `eth_cancelBundle` is used to prevent a submitted bundle from being included on-chain. See [bundle cancellations](https://docs.flashbots.net/flashbots-auction/searchers/advanced/bundle-cancellations) for more information.
    #[method(name = "cancelBundle")]
    async fn cancel_bundle(&self, request: ()) -> RpcResult<()>;

    /// `eth_sendPrivateTransaction` is used to send a single transaction to Flashbots. Flashbots will attempt to build a block including the transaction for the next 25 blocks. See [Private Transactions](https://docs.flashbots.net/flashbots-auction/searchers/advanced/private-transaction) for more info.
    #[method(name = "sendPrivateTransaction")]
    async fn send_private_transaction(&self, request: ()) -> RpcResult<()>;

    /// The `eth_cancelPrivateTransaction` method stops private transactions from being submitted
    /// for future blocks.
    ///
    /// A transaction can only be cancelled if the request is signed by the same key as the
    /// eth_sendPrivateTransaction call submitting the transaction in first place.
    #[method(name = "cancelPrivateTransaction")]
    async fn cancel_private_transaction(&self, request: ()) -> RpcResult<()>;
}
