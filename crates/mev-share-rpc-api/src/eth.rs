use alloy::rpc::types::mev::{CancelBundleRequest, CancelPrivateTransactionRequest, EthBundleHash, EthCallBundleResponse, EthCallBundleTransactionResult, EthSendBundle, PrivateTransactionRequest};
use alloy_primitives::{Bytes, B256};

// re-export the rpc server trait
#[cfg(feature = "server")]
pub use rpc::EthBundleApiServer;

/// jsonrpsee generated code.
///
/// This hides the generated client trait which is replaced by the `EthBundleApiClient` trait.
mod rpc {
    use alloy::rpc::types::mev::{CancelBundleRequest, CancelPrivateTransactionRequest, EthBundleHash, EthCallBundleResponse, EthCallBundleTransactionResult, EthSendBundle, PrivateTransactionRequest};
    use alloy_primitives::{Bytes, B256};
    use jsonrpsee::proc_macros::rpc;

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
        async fn send_bundle(
            &self,
            bundle: EthSendBundle,
        ) -> jsonrpsee::core::RpcResult<EthBundleHash>;

        /// `eth_callBundle` can be used to simulate a bundle against a specific block number,
        /// including simulating a bundle at the top of the next block.
        #[method(name = "callBundle")]
        async fn call_bundle(
            &self,
            request: EthCallBundleResponse,
        ) -> jsonrpsee::core::RpcResult<EthCallBundleTransactionResult>;

        /// `eth_cancelBundle` is used to prevent a submitted bundle from being included on-chain. See [bundle cancellations](https://docs.flashbots.net/flashbots-auction/searchers/advanced/bundle-cancellations) for more information.
        #[method(name = "cancelBundle")]
        async fn cancel_bundle(
            &self,
            request: CancelBundleRequest,
        ) -> jsonrpsee::core::RpcResult<()>;

        /// `eth_sendPrivateTransaction` is used to send a single transaction to Flashbots. Flashbots will attempt to build a block including the transaction for the next 25 blocks. See [Private Transactions](https://docs.flashbots.net/flashbots-protect/additional-documentation/eth-sendPrivateTransaction) for more info.
        #[method(name = "sendPrivateTransaction")]
        async fn send_private_transaction(
            &self,
            request: PrivateTransactionRequest,
        ) -> jsonrpsee::core::RpcResult<B256>;

        /// The `eth_sendPrivateRawTransaction` method can be used to send private transactions to
        /// the RPC endpoint. Private transactions are protected from frontrunning and kept
        /// private until included in a block. A request to this endpoint needs to follow
        /// the standard eth_sendRawTransaction
        #[method(name = "sendPrivateRawTransaction")]
        async fn send_private_raw_transaction(
            &self,
            bytes: Bytes,
        ) -> jsonrpsee::core::RpcResult<B256>;

        /// The `eth_cancelPrivateTransaction` method stops private transactions from being
        /// submitted for future blocks.
        ///
        /// A transaction can only be cancelled if the request is signed by the same key as the
        /// eth_sendPrivateTransaction call submitting the transaction in first place.
        #[method(name = "cancelPrivateTransaction")]
        async fn cancel_private_transaction(
            &self,
            request: CancelPrivateTransactionRequest,
        ) -> jsonrpsee::core::RpcResult<bool>;
    }
}

/// An object safe version of the `EthBundleApiClient` trait.
#[cfg(feature = "client")]
#[async_trait::async_trait]
pub trait EthBundleApiClient {
    /// `eth_sendBundle` can be used to send your bundles to the builder.
    async fn send_bundle(
        &self,
        bundle: EthSendBundle,
    ) -> Result<EthBundleHash, jsonrpsee::core::Error>;

    /// `eth_callBundle` can be used to simulate a bundle against a specific block number, including
    /// simulating a bundle at the top of the next block.
    async fn call_bundle(
        &self,
        request: EthCallBundleResponse,
    ) -> Result<EthCallBundleTransactionResult, jsonrpsee::core::Error>;

    /// `eth_cancelBundle` is used to prevent a submitted bundle from being included on-chain. See [bundle cancellations](https://docs.flashbots.net/flashbots-auction/searchers/advanced/bundle-cancellations) for more information.
    async fn cancel_bundle(
        &self,
        request: CancelBundleRequest,
    ) -> Result<(), jsonrpsee::core::Error>;

    /// `eth_sendPrivateTransaction` is used to send a single transaction to Flashbots. Flashbots will attempt to build a block including the transaction for the next 25 blocks. See [Private Transactions](https://docs.flashbots.net/flashbots-protect/additional-documentation/eth-sendPrivateTransaction) for more info.
    async fn send_private_transaction(
        &self,
        request: PrivateTransactionRequest,
    ) -> Result<B256, jsonrpsee::core::Error>;

    /// The `eth_sendPrivateRawTransaction` method can be used to send private transactions to the
    /// RPC endpoint. Private transactions are protected from frontrunning and kept private until
    /// included in a block. A request to this endpoint needs to follow the standard
    /// eth_sendRawTransaction
    async fn send_private_raw_transaction(
        &self,
        bytes: Bytes,
    ) -> Result<B256, jsonrpsee::core::Error>;

    /// The `eth_cancelPrivateTransaction` method stops private transactions from being submitted
    /// for future blocks.
    ///
    /// A transaction can only be cancelled if the request is signed by the same key as the
    /// eth_sendPrivateTransaction call submitting the transaction in first place.
    async fn cancel_private_transaction(
        &self,
        request: CancelPrivateTransactionRequest,
    ) -> Result<bool, jsonrpsee::core::Error>;
}

#[cfg(feature = "client")]
#[async_trait::async_trait]
impl<T> EthBundleApiClient for T
where
    T: rpc::EthBundleApiClient + Sync,
{
    async fn send_bundle(
        &self,
        bundle: EthSendBundle,
    ) -> Result<EthBundleHash, jsonrpsee::core::Error> {
        rpc::EthBundleApiClient::send_bundle(self, bundle).await
    }

    async fn call_bundle(
        &self,
        request: EthCallBundleResponse,
    ) -> Result<EthCallBundleTransactionResult, jsonrpsee::core::Error> {
        rpc::EthBundleApiClient::call_bundle(self, request).await
    }

    async fn cancel_bundle(
        &self,
        request: CancelBundleRequest,
    ) -> Result<(), jsonrpsee::core::Error> {
        rpc::EthBundleApiClient::cancel_bundle(self, request).await
    }

    async fn send_private_transaction(
        &self,
        request: PrivateTransactionRequest,
    ) -> Result<B256, jsonrpsee::core::Error> {
        rpc::EthBundleApiClient::send_private_transaction(self, request).await
    }

    async fn send_private_raw_transaction(
        &self,
        bytes: Bytes,
    ) -> Result<B256, jsonrpsee::core::Error> {
        rpc::EthBundleApiClient::send_private_raw_transaction(self, bytes).await
    }

    async fn cancel_private_transaction(
        &self,
        request: CancelPrivateTransactionRequest,
    ) -> Result<bool, jsonrpsee::core::Error> {
        rpc::EthBundleApiClient::cancel_private_transaction(self, request).await
    }
}

#[cfg(all(test, feature = "client"))]
mod tests {
    use super::*;

    #[allow(dead_code)]
    struct Client {
        inner: Box<dyn EthBundleApiClient>,
    }
}
