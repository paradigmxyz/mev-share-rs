//! MEV-share bundle type bindings

use ethers_core::types::{Bytes, U64, TxHash, Address};
use serde::{Deserialize, Serialize};

/// A bundle of transactions to send to the matchmaker.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
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
    /// Requirements for the bundle to be included in the block. 
    #[serde(rename = "validity", skip_serializing_if = "Option::is_none")] 
    pub validity_predicate: Option<ValidityPredicate>,
    /// Preferences on what data should be shared about the bundle and its transactions
    #[serde(rename = "privacy", skip_serializing_if = "Option::is_none")]
    pub privacy_predicate: Option<PrivacyPredicate>,
}

/// Data used by block builders to check if the bundle should be considered for inclusion.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InclusionPredicate {
    /// The first block the bundle is valid for.
    pub block: U64,
    /// The last block the bundle is valid for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_block: Option<U64>,
}

/// A bundle tx, which can either be a transaction hash, or a full tx.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum BundleItem {
    /// The hash of either a transaction or bundle we are trying to backrun.
    Hash {
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

/// Requirements for the bundle to be included in the block.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValidityPredicate {
    /// Specifies the minimum percent of a given bundle's earnings to redistribute 
    /// for it to be included in a builder's block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refund: Option<Vec<Refund>>,
    /// Specifies what addresses should receive what percent of the overall refund for this bundle, 
    /// if it is enveloped by another bundle (eg. a searcher backrun).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refund_config: Option<Vec<RefundConfig>>,
}

/// Specifies the minimum percent of a given bundle's earnings to redistribute
/// for it to be included in a builder's block.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Refund {
    /// The index of the transaction in the bundle.
    pub body_idx: u64,
    /// The minimum percent of the bundle's earnings to redistribute.
    pub percent: u64,
}

/// Specifies what addresses should receive what percent of the overall refund for this bundle,
/// if it is enveloped by another bundle (eg. a searcher backrun).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RefundConfig {
    /// The address to refund.
    pub address: Address,
    /// The minimum percent of the bundle's earnings to redistribute.
    pub percent: u64,
}

/// Preferences on what data should be shared about the bundle and its transactions
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyPredicate {
    /// Hints on what data should be shared about the bundle and its transactions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<Vec<PrivacyHint>>,
    /// The addresses of the builders that should be allowed to see the bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builders: Option<Vec<Address>>,
}

/// Hints on what data should be shared about the bundle and its transactions
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyHint {
    /// The calldata of the transaction.
    Calldata,
    /// The address of the contract the transaction is calling.
    ContractAddress,
    /// The logs emitted by the transaction.
    Logs,
    /// The 4-byte function selector of the transaction.
    FunctionSelector,
    /// The hash of the transaction.
    Hash,
}



/// Response from the matchmaker after sending a bundle.
#[derive(Deserialize, Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SendBundleResponse {
    /// Hash of the bundle bodies.
    bundle_hash: TxHash,
}

/// The version of the MEV-share API to use.
#[derive(Deserialize, Debug, Serialize, Clone, Default, PartialEq)]
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
            validity_predicate: None,
            privacy_predicate: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use ethers_core::types::Bytes;

    use crate::{types::SendBundleRequest, types::ProtocolVersion, InclusionPredicate, BundleItem, ValidityPredicate, RefundConfig, PrivacyPredicate, PrivacyHint};

    #[test]
    fn can_deserialize_simple() {
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

    #[test]
    fn can_deserialize_complex() {
        let str = r#"
        [{
            "version": "v0.1",
            "inclusion": {
                "block": "0x1"
            },
            "body": [{
                "tx": "0x02f86b0180843b9aca00852ecc889a0082520894c87037874aed04e51c29f582394217a0a2b89d808080c080a0a463985c616dd8ee17d7ef9112af4e6e06a27b071525b42182fe7b0b5c8b4925a00af5ca177ffef2ff28449292505d41be578bebb77110dfc09361d2fb56998260",
                "canRevert": false
            }], 
            "privacy": {
                "hints": [
                  "calldata"
                ]
              },
              "validity": {
                "refundConfig": [
                  {
                    "address": "0x8EC1237b1E80A6adf191F40D4b7D095E21cdb18f",
                    "percent": 100
                  }
                ]
              }
        }]
        "#;
        let res: Result<Vec<SendBundleRequest>, _> = serde_json::from_str(str);
        assert!(res.is_ok());
    }

    #[test]
    fn can_serialize_complex() {
        let str = r#"
        [{
            "version": "v0.1",
            "inclusion": {
                "block": "0x1"
            },
            "body": [{
                "tx": "0x02f86b0180843b9aca00852ecc889a0082520894c87037874aed04e51c29f582394217a0a2b89d808080c080a0a463985c616dd8ee17d7ef9112af4e6e06a27b071525b42182fe7b0b5c8b4925a00af5ca177ffef2ff28449292505d41be578bebb77110dfc09361d2fb56998260",
                "canRevert": false
            }], 
            "privacy": {
                "hints": [
                  "calldata"
                ]
              },
              "validity": {
                "refundConfig": [
                  {
                    "address": "0x8EC1237b1E80A6adf191F40D4b7D095E21cdb18f",
                    "percent": 100
                  }
                ]
              }
        }]
        "#;
        let bundle_body = vec![BundleItem::Tx {
            tx: Bytes::from_str("0x02f86b0180843b9aca00852ecc889a0082520894c87037874aed04e51c29f582394217a0a2b89d808080c080a0a463985c616dd8ee17d7ef9112af4e6e06a27b071525b42182fe7b0b5c8b4925a00af5ca177ffef2ff28449292505d41be578bebb77110dfc09361d2fb56998260").unwrap(),
            can_revert: false,
        }];

        let validity_predicate = ValidityPredicate {
            refund_config: Some(vec![RefundConfig {
                address: "0x8EC1237b1E80A6adf191F40D4b7D095E21cdb18f".parse().unwrap(),
                percent: 100,
            }]),
            ..Default::default()
        };
        let privacy_predicate = PrivacyPredicate {
            hints: Some(vec![PrivacyHint::Calldata]),
            ..Default::default()
        };
        let bundle = SendBundleRequest {
            protocol_version: ProtocolVersion::V0_1,
            inclusion_predicate: InclusionPredicate {
                block: 1.into(),
                max_block: None,
            },
            bundle_body,
            validity_predicate: Some(validity_predicate),
            privacy_predicate: Some(privacy_predicate),
        };
        let expected = serde_json::from_str::<Vec<SendBundleRequest>>(str).unwrap();
        assert_eq!(bundle, expected[0]);
    }
}
