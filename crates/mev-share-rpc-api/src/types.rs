//! MEV-share bundle type bindings

use ethers_core::types::{Address, Bytes, TxHash, U64, BlockId, Log};
use serde::{
    ser::{SerializeSeq, Serializer},
    Deserialize, Deserializer, Serialize,
};

/// A bundle of transactions to send to the matchmaker.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SendBundleRequest {
    /// The version of the MEV-share API to use.
    #[serde(rename = "version")]
    pub protocol_version: ProtocolVersion,
    /// Data used by block builders to check if the bundle should be considered for inclusion.
    #[serde(rename = "inclusion")]
    pub inclusion: Inclusion,
    /// The transactions to include in the bundle.
    #[serde(rename = "body")]
    pub bundle_body: Vec<BundleItem>,
    /// Requirements for the bundle to be included in the block.
    #[serde(rename = "validity", skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
    /// Preferences on what data should be shared about the bundle and its transactions
    #[serde(rename = "privacy", skip_serializing_if = "Option::is_none")]
    pub privacy: Option<Privacy>,
}

/// Data used by block builders to check if the bundle should be considered for inclusion.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Inclusion {
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
pub struct Validity {
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
pub struct Privacy {
    /// Hints on what data should be shared about the bundle and its transactions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<PrivacyHint>,
    /// The addresses of the builders that should be allowed to see the bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builders: Option<Vec<Address>>,
}

/// Hints on what data should be shared about the bundle and its transactions
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PrivacyHint {
    /// The calldata of the bundle's transactions should be shared.
    pub calldata: bool,
    /// The address of the bundle's transactions should be shared.
    pub contract_address: bool,
    /// The logs of the bundle's transactions should be shared.
    pub logs: bool,
    /// The function selector of the bundle's transactions should be shared.
    pub function_selector: bool,
    /// The hash of the bundle's transactions should be shared.
    pub hash: bool,
    /// The hash of the bundle should be shared.
    pub tx_hash: bool,
}

#[allow(missing_docs)]
impl PrivacyHint {
    pub fn with_calldata(mut self) -> Self {
        self.calldata = true;
        self
    }

    pub fn with_contract_address(mut self) -> Self {
        self.contract_address = true;
        self
    }

    pub fn with_logs(mut self) -> Self {
        self.logs = true;
        self
    }

    pub fn with_function_selector(mut self) -> Self {
        self.function_selector = true;
        self
    }

    pub fn with_hash(mut self) -> Self {
        self.hash = true;
        self
    }

    pub fn with_tx_hash(mut self) -> Self {
        self.tx_hash = true;
        self
    }

    pub fn has_calldata(&self) -> bool {
        self.calldata
    }

    pub fn has_contract_address(&self) -> bool {
        self.contract_address
    }

    pub fn has_logs(&self) -> bool {
        self.logs
    }

    pub fn has_function_selector(&self) -> bool {
        self.function_selector
    }

    pub fn has_hash(&self) -> bool {
        self.hash
    }

    pub fn has_tx_hash(&self) -> bool {
        self.tx_hash
    }

    fn num_hints(&self) -> usize {
        let mut num_hints = 0;
        if self.calldata {
            num_hints += 1;
        }
        if self.contract_address {
            num_hints += 1;
        }
        if self.logs {
            num_hints += 1;
        }
        if self.function_selector {
            num_hints += 1;
        }
        if self.hash {
            num_hints += 1;
        }
        if self.tx_hash {
            num_hints += 1;
        }
        num_hints
    }
}

impl Serialize for PrivacyHint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.num_hints()))?;
        if self.calldata {
            seq.serialize_element("calldata")?;
        }
        if self.contract_address {
            seq.serialize_element("contract_address")?;
        }
        if self.logs {
            seq.serialize_element("logs")?;
        }
        if self.function_selector {
            seq.serialize_element("function_selector")?;
        }
        if self.hash {
            seq.serialize_element("hash")?;
        }
        if self.tx_hash {
            seq.serialize_element("tx_hash")?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for PrivacyHint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hints = Vec::<String>::deserialize(deserializer)?;
        let mut privacy_hint = PrivacyHint::default();
        for hint in hints {
            match hint.as_str() {
                "calldata" => privacy_hint.calldata = true,
                "contract_address" => privacy_hint.contract_address = true,
                "logs" => privacy_hint.logs = true,
                "function_selector" => privacy_hint.function_selector = true,
                "hash" => privacy_hint.hash = true,
                "tx_hash" => privacy_hint.tx_hash = true,
                _ => return Err(serde::de::Error::custom("invalid privacy hint")),
            }
        }
        Ok(privacy_hint)
    }
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

/// Optional fields to override simulation state.
#[derive(Deserialize, Debug, Serialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SimBundleOptions { 
    /// Block used for simulation state. Defaults to latest block.
    /// Block header data will be derived from parent block by default.
    /// Specify other params to override the default values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_block: Option<BlockId>,
    /// Block number used for simulation, defaults to parentBlock.number + 1
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_number: Option<U64>,
    /// Coinbase used for simulation, defaults to parentBlock.coinbase
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coinbase: Option<Address>,
    /// Timestamp used for simulation, defaults to parentBlock.timestamp + 12
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<U64>,
    /// Gas limit used for simulation, defaults to parentBlock.gasLimit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_limit: Option<U64>,
    /// Base fee used for simulation, defaults to parentBlock.baseFeePerGas
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_fee: Option<U64>,
    /// Timeout in seconds, defaults to 5
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<U64>,
}

/// Response from the matchmaker after sending a simulation request.
#[derive(Deserialize, Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SimBundleResponse {
    /// Whether the simulation was successful.
    pub success: bool,
    /// Error message if the simulation failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The block number of the simulated block.
    pub state_block: U64,
    /// The gas price of the simulated block.
    pub mev_gas_price: U64,
    /// The profit of the simulated block.
    pub profit: U64,
    /// The refundable value of the simulated block.
    pub refundable_value: U64,
    /// The gas used by the simulated block.
    pub gas_used: U64,
    /// Logs returned by mev_simBundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logs: Option<Vec<SimBundleLogs>>,
}

/// Logs returned by mev_simBundle.
#[derive(Deserialize, Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SimBundleLogs {
    /// Logs for transactions in bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_logs: Option<Vec<Log>>,
    /// Logs for bundles in bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_logs: Option<Vec<SimBundleLogs>>,
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
            inclusion: Inclusion { block: block_num, max_block },
            bundle_body,
            validity: None,
            privacy: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use ethers_core::types::Bytes;

    use crate::{
        types::{ProtocolVersion, SendBundleRequest},
        BundleItem, Inclusion, Privacy, PrivacyHint, RefundConfig, Validity,
    };

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

        let validity = Some(Validity {
            refund_config: Some(vec![RefundConfig {
                address: "0x8EC1237b1E80A6adf191F40D4b7D095E21cdb18f".parse().unwrap(),
                percent: 100,
            }]),
            ..Default::default()
        });

        let privacy = Some(Privacy {
            hints: Some(PrivacyHint { calldata: true, ..Default::default() }),
            ..Default::default()
        });

        let bundle = SendBundleRequest {
            protocol_version: ProtocolVersion::V0_1,
            inclusion: Inclusion { block: 1.into(), max_block: None },
            bundle_body,
            validity,
            privacy,
        };
        let expected = serde_json::from_str::<Vec<SendBundleRequest>>(str).unwrap();
        assert_eq!(bundle, expected[0]);
    }

    #[test]
    fn can_serialize_privacy_hint() {
        let hint = PrivacyHint {
            calldata: true,
            contract_address: true,
            logs: true,
            function_selector: true,
            hash: true,
            tx_hash: true,
        };
        let expected =
            r#"["calldata","contract_address","logs","function_selector","hash","tx_hash"]"#;
        let actual = serde_json::to_string(&hint).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn can_deserialize_privacy_hint() {
        let hint = PrivacyHint {
            calldata: true,
            contract_address: false,
            logs: true,
            function_selector: false,
            hash: true,
            tx_hash: false,
        };
        let expected = r#"["calldata","logs","hash"]"#;
        let actual: PrivacyHint = serde_json::from_str(expected).unwrap();
        assert_eq!(actual, hint);
    }
}
