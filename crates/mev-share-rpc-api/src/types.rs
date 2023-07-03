//! MEV-share bundle type bindings

use ethers_core::types::{Bytes, U64, TxHash};
use serde::{Deserialize, Serialize};

/// A bundle of transactions to send to the matchmaker.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendBundleRequest {
    /// The version of the MEV-share API to use.
    #[serde(rename = "version")]
    pub protocol_version: ProtocolVersion,
    /// Data used by block builders to check if the bundle should be considered for inclusion.
    #[serde(rename = "inclusion")]
    pub inclusion_predicate: InclusionPredicate,
    /// The transactions to include in the bundle.
    #[serde(rename = "body")]
    pub bundle_body: Vec<BundleItem>,
}

/// Data used by block builders to check if the bundle should be considered for inclusion.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InclusionPredicate {
    /// The first block the bundle is valid for.
    pub block: U64,
    /// The last block the bundle is valid for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_block: Option<U64>,
}

/// A bundle tx, which can either be a transaction hash, or a full tx.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum BundleItem {
    /// The hash of the transaction we are trying to backrun.
    TxHash {
        /// Tx hash.
        hash: TxHash,
    },
    /// A new signed transaction.
    #[serde(rename_all = "camelCase")]
    Tx {
        /// Bytes of the signed transaction.
        tx: Bytes,
        /// If true, the transaction can revert without the bundle being considered invalid.
        can_revert: bool,
    },
}

/// Response from the matchmaker after sending a bundle.
#[derive(Deserialize, Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendBundleResponse {
    /// Hash of the bundle bodies.
    bundle_hash: TxHash,
}

/// The version of the MEV-share API to use.
#[derive(Deserialize, Debug, Serialize, Clone, Default)]
pub enum ProtocolVersion {
    #[default]
    #[serde(rename = "beta-1")]
    /// The beta-1 version of the API.
    Beta1,
    /// The 0.1 version of the API.
    #[serde(rename = "v0.1")]
    V0_1,
}

impl SendBundleRequest {
    /// Create a new bundle request.
    pub fn new(
        block_num: U64,
        max_block: Option<U64>,
        protocol_version: ProtocolVersion,
        bundle_body: Vec<BundleItem>,
    ) -> Self {
        Self {
            protocol_version,
            inclusion_predicate: InclusionPredicate {
                block: block_num,
                max_block,
            },
            bundle_body,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::types::SendBundleRequest;

    #[test]
    fn can_deserialize() {
        let str = r#"
        [{
            "version": "v0.1",
            "inclusion": {
                "block": "0x1"
            },
            "body": [{
                "tx": "0x02f86b0180843b9aca00852ecc889a0082520894c87037874aed04e51c29f582394217a0a2b89d808080c080a0a463985c616dd8ee17d7ef9112af4e6e06a27b071525b42182fe7b0b5c8b4925a00af5ca177ffef2ff28449292505d41be578bebb77110dfc09361d2fb56998260",
                "canRevert": false
            }]
        }]
        "#;
        let res: Result<Vec<SendBundleRequest>, _> = serde_json::from_str(str);
        assert!(res.is_ok());
    }
}
