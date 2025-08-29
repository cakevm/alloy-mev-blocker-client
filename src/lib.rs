use alloy_consensus::TxEnvelope;
use alloy_provider::{Network, Provider};
use alloy_rpc_types_eth::Transaction;
use alloy_transport::TransportResult;
use async_trait::async_trait;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use tracing::error;

pub const MEV_BLOCKER_SEARCHERS_URL: &str = "wss://searchers.mevblocker.io";

#[derive(Debug, Clone)]
pub struct MevBlockerTx(pub Transaction<TxEnvelope>);

// Adjust fields to parse into `alloy_rpc_types_eth::Transaction`.
// MEV Blocker pending transactions lacks e.g. fields like `r`, `s`, `v`, and `yParity`.
// API doc: https://docs.cow.fi/mevblocker/searchers/bidding-on-transactions
impl<'de> Deserialize<'de> for MevBlockerTx {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut value: Value = Deserialize::deserialize(deserializer)?;
        let original_input = value.to_string(); // Save the original value for logging

        // If the "type" field is missing, add type 0x0
        if value.get("type").is_none()
            && let Some(obj) = value.as_object_mut()
        {
            obj.insert("type".to_string(), Value::String("0x0".to_string()));
        }

        // Put the content of the "data" field into the "input" field
        // If the "data" field is null use "0x" as the default value
        if let Some(data) = value.get_mut("data") {
            let mut input = data.take();
            if input.is_null() {
                input = Value::String("0x".to_string());
            }
            if let Some(obj) = value.as_object_mut() {
                obj.insert("input".to_string(), input);
            }
        }
        value.as_object_mut().unwrap().remove("data");

        if value.get("type").unwrap_or(&Value::String("0x".to_string())).as_str().unwrap_or_default() == "0x3" {
            if value.get("blobVersionedHashes").is_none()
                && let Some(obj) = value.as_object_mut()
            {
                obj.insert("blobVersionedHashes".to_string(), Value::Array(vec![]));
            }
            if value.get("maxFeePerBlobGas").is_none()
                && let Some(obj) = value.as_object_mut()
            {
                obj.insert("maxFeePerBlobGas".to_string(), Value::String("0x0".to_string()));
            }
        }

        // If the "type" field is 0x4 and "authorizationList" is missing, add an empty array
        if value.get("type").unwrap_or(&Value::String("0x".to_string())).as_str().unwrap_or_default() == "0x4"
            && value.get("authorizationList").is_none()
            && let Some(obj) = value.as_object_mut()
        {
            obj.insert("authorizationList".to_string(), Value::Array(vec![]));
        }

        // Add the "r", "s", "v" fields
        if let Some(obj) = value.as_object_mut() {
            obj.insert("r".to_string(), Value::String("".to_string()));
            obj.insert("s".to_string(), Value::String("".to_string()));
            obj.insert("v".to_string(), Value::String("0x1B".to_string()));
            obj.insert("yParity".to_string(), Value::String("0x1".to_string()));
        }

        let tx: Transaction<TxEnvelope> = match serde_json::from_value(value) {
            Ok(tx) => tx,
            Err(err) => {
                // This can only happen when the format of MEV Blocker changes, or we have a bug.
                // Log this error here with the original input, because it will be swallowed by Alloy.
                error!(?err, %original_input, "Error deserializing MevBlockerTx");
                return Err(serde::de::Error::custom(err));
            }
        };

        Ok(MevBlockerTx(tx))
    }
}

#[async_trait]
pub trait MevBlockerApi<N>: Send + Sync {
    async fn subscribe_mev_blocker_pending_transactions(&self) -> TransportResult<alloy_pubsub::Subscription<MevBlockerTx>>;
}

#[async_trait]
impl<N, P> MevBlockerApi<N> for P
where
    N: Network,
    P: Provider<N>,
{
    async fn subscribe_mev_blocker_pending_transactions(&self) -> TransportResult<alloy_pubsub::Subscription<MevBlockerTx>> {
        self.root().client().pubsub_frontend().ok_or_else(alloy_transport::TransportErrorKind::pubsub_unavailable)?;

        let mut call = self.client().request("eth_subscribe", ("mevBlocker_subscribePartialPendingTransactions",));
        call.set_is_subscription();
        let id = call.await?;
        self.root().get_subscription(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{TxHash, address};
    use alloy_provider::network::TransactionResponse;
    use std::str::FromStr;

    #[test]
    fn test_deserialize_type_0() {
        let tx_raw = r#"{
            "nonce": "0x1",
            "gasPrice": "0x171a390d1",
            "gas": "0xb6bd",
            "to": "0xa1b2c3d4e5f6789abcdef0123456789abcdef012",
            "value": "0x0",
            "data": "0x1234",
            "hash": "0x1111111111111111111111111111111111111111111111111111111111111111",
            "from": "0xfedcba0987654321fedcba0987654321fedcba09"
        }"#;

        let tx: MevBlockerTx = serde_json::from_str(tx_raw).unwrap();
        assert_eq!(tx.0.from(), address!("fedcba0987654321fedcba0987654321fedcba09"));
        assert_eq!(tx.0.tx_hash(), TxHash::from_str("0x1111111111111111111111111111111111111111111111111111111111111111").unwrap());
    }

    #[test]
    fn test_deserialize_type_2() {
        let tx_raw = r#"{
            "chainId": "0x1",
            "to": "0x9876543210abcdef9876543210abcdef98765432",
            "value": "0x409d6f54da38000",
            "data": "0x1234",
            "accessList": [],
            "nonce": "0xa",
            "maxPriorityFeePerGas": "0x0",
            "maxFeePerGas": "0x171906896",
            "gas": "0x262e6",
            "type": "0x2",
            "hash": "0x3333333333333333333333333333333333333333333333333333333333333333",
            "from": "0xabcdef0123456789abcdef0123456789abcdef01"
        }"#;

        let tx: MevBlockerTx = serde_json::from_str(tx_raw).unwrap();
        assert_eq!(tx.0.from(), address!("abcdef0123456789abcdef0123456789abcdef01"));
        assert_eq!(tx.0.tx_hash(), TxHash::from_str("0x3333333333333333333333333333333333333333333333333333333333333333").unwrap());
    }

    #[test]
    fn test_deserialize_type_1() {
        let raw_tx = r#"{
            "chainId": "0x1",
            "to": "0xdef9876543210abcdef9876543210abcdef98765",
            "value": "0xfc1eb84cae93d1d",
            "data": "0x1234",
            "accessList": [],
            "nonce": "0x491",
            "gasPrice": "0x239cfbce0",
            "gas": "0x31cf1",
            "type": "0x1",
            "hash": "0x2222222222222222222222222222222222222222222222222222222222222222",
            "from": "0x123456789abcdef0123456789abcdef012345678"
        }"#;

        let tx: MevBlockerTx = serde_json::from_str(raw_tx).unwrap();
        assert_eq!(tx.0.from(), address!("123456789abcdef0123456789abcdef012345678"));
        assert_eq!(tx.0.tx_hash(), TxHash::from_str("0x2222222222222222222222222222222222222222222222222222222222222222").unwrap());
    }

    #[test]
    fn test_deserialize_type_2_with_access_list() {
        let tx_raw = r#"{
            "chainId": "0x1",
            "to": "0x5432109876543210987654321098765432109876",
            "value": "0x0",
            "data": "0x1234",
            "accessList": [
                {
                    "address": "0x1111111111111111111111111111111111111111",
                    "storageKeys": []
                },
                {
                    "address": "0x2222222222222222222222222222222222222222",
                    "storageKeys": [
                        "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc",
                        "0x97b1af316dcacf90a3ec3fed778de1155ab6cfb9c9285e99caa9742e51837418"
                    ]
                }
            ],
            "nonce": "0x1789",
            "maxPriorityFeePerGas": "0x0",
            "maxFeePerGas": "0x6c455a394",
            "gas": "0xf3936",
            "type": "0x2",
            "hash": "0x4444444444444444444444444444444444444444444444444444444444444444",
            "from": "0x0987654321098765432109876543210987654321"
        }"#;

        let tx: MevBlockerTx = serde_json::from_str(tx_raw).unwrap();
        assert_eq!(tx.0.from(), address!("0987654321098765432109876543210987654321"));
        assert_eq!(tx.0.tx_hash(), TxHash::from_str("0x4444444444444444444444444444444444444444444444444444444444444444").unwrap());
    }

    #[test]
    fn test_deserialize_type_3() {
        let raw_tx = r#"{
            "accessList": [],
            "chainId": "0x1",
            "data": null,
            "from": "0x6789abcdef0123456789abcdef0123456789abcd",
            "gas": "0x5208",
            "hash": "0x5555555555555555555555555555555555555555555555555555555555555555",
            "maxFeePerGas": "0x60b66031a",
            "maxPriorityFeePerGas": "0x0",
            "nonce": "0x6663",
            "to": "0xcdef0123456789abcdef0123456789abcdef0123",
            "type": "0x3",
            "value": "0x0"
        }"#;

        let tx: MevBlockerTx = serde_json::from_str(raw_tx).unwrap();
        assert_eq!(tx.0.from(), address!("6789abcdef0123456789abcdef0123456789abcd"));
        assert_eq!(tx.0.tx_hash(), TxHash::from_str("5555555555555555555555555555555555555555555555555555555555555555").unwrap());
    }

    #[test]
    fn test_deserialize_type_4() {
        let tx_raw = r#"{
            "accessList": [],
            "chainId": "0x1",
            "data": "0x2ba03a7900000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000030b5149626955069c159d21045a01175035b986656c1226d46060f151e6cece0919254a91a60a8fc2e21fdd4ff73b15df300000000000000000000000000000000",
            "from": "0xa1b2c3d4e5f6789abcdef0123456789abcdef012",
            "gas": "0x30d40",
            "hash": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            "maxFeePerGas": "0x1605dd319",
            "maxPriorityFeePerGas": "0x0",
            "nonce": "0x2c",
            "to": "0xfedcba0987654321fedcba0987654321fedcba09",
            "type": "0x4",
            "value": "0x0"
        }"#;

        let tx: MevBlockerTx = serde_json::from_str(tx_raw).unwrap();
        assert_eq!(tx.0.from(), address!("a1b2c3d4e5f6789abcdef0123456789abcdef012"));
        assert_eq!(tx.0.tx_hash(), TxHash::from_str("0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef").unwrap());
    }
}
