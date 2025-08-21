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
            "to": "0x8b6301d34de337698ba27e01a30b74799aed7b4a",
            "value": "0x0",
            "data": "0x1234",
            "hash": "0xbfb35a8a3e435b7d78ab3c187904fd9bb72ef0e0fd2c28b5d979f71f01d2fca5",
            "from": "0x29ef51af25c37f274c994ea520e3925772ac1bd3"
        }"#;

        let tx: MevBlockerTx = serde_json::from_str(tx_raw).unwrap();
        assert_eq!(tx.0.from(), address!("29ef51af25c37f274c994ea520e3925772ac1bd3"));
        assert_eq!(tx.0.tx_hash(), TxHash::from_str("0xbfb35a8a3e435b7d78ab3c187904fd9bb72ef0e0fd2c28b5d979f71f01d2fca5").unwrap());
    }

    #[test]
    fn test_deserialize_type_2() {
        let tx_raw = r#"{
            "chainId": "0x1",
            "to": "0xf3de3c0d654fda23dad170f0f320a92172509127",
            "value": "0x409d6f54da38000",
            "data": "0x1234",
            "accessList": [],
            "nonce": "0xa",
            "maxPriorityFeePerGas": "0x0",
            "maxFeePerGas": "0x171906896",
            "gas": "0x262e6",
            "type": "0x2",
            "hash": "0xe2e1255ea1d8f60a0867095253beac0819c86b4e5341cf30c90d23a702a3fa6e",
            "from": "0xab10b06f30a148ff6cfe0d1ee5441a7d2643a610"
        }"#;

        let tx: MevBlockerTx = serde_json::from_str(tx_raw).unwrap();
        assert_eq!(tx.0.from(), address!("ab10b06f30a148ff6cfe0d1ee5441a7d2643a610"));
        assert_eq!(tx.0.tx_hash(), TxHash::from_str("0xe2e1255ea1d8f60a0867095253beac0819c86b4e5341cf30c90d23a702a3fa6e").unwrap());
    }

    #[test]
    fn test_deserialize_type_1() {
        let raw_tx = r#"{
            "chainId": "0x1",
            "to": "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
            "value": "0xfc1eb84cae93d1d",
            "data": "0x1234",
            "accessList": [],
            "nonce": "0x491",
            "gasPrice": "0x239cfbce0",
            "gas": "0x31cf1",
            "type": "0x1",
            "hash": "0xbebfd9b44436d788d73793fb8165c6385333eeea97df4c897b29f2391516a0be",
            "from": "0xa73b2ec30bf671daac4f7ac0428cbd3641251bd9"
        }"#;

        let tx: MevBlockerTx = serde_json::from_str(raw_tx).unwrap();
        assert_eq!(tx.0.from(), address!("a73b2ec30bf671daac4f7ac0428cbd3641251bd9"));
        assert_eq!(tx.0.tx_hash(), TxHash::from_str("0xbebfd9b44436d788d73793fb8165c6385333eeea97df4c897b29f2391516a0be").unwrap());
    }

    #[test]
    fn test_deserialize_type_2_with_access_list() {
        let tx_raw = r#"{
            "chainId": "0x1",
            "to": "0x9008d19f58aabd9ed0d60971565aa8510560ab41",
            "value": "0x0",
            "data": "0x1234",
            "accessList": [
                {
                    "address": "0x1923dfee706a8e78157416c29cbccfde7cdf4102",
                    "storageKeys": []
                },
                {
                    "address": "0x2c4c28ddbdac9c5e7055b4c863b72ea0149d8afe",
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
            "hash": "0xc1bc47c70dcfb9fe381432e71509b6909df55c99197750782a86aca8570fdfe3",
            "from": "0x00806daa2cfe49715ea05243ff259deb195760fc"
        }"#;

        let tx: MevBlockerTx = serde_json::from_str(tx_raw).unwrap();
        assert_eq!(tx.0.from(), address!("00806daa2cfe49715ea05243ff259deb195760fc"));
        assert_eq!(tx.0.tx_hash(), TxHash::from_str("0xc1bc47c70dcfb9fe381432e71509b6909df55c99197750782a86aca8570fdfe3").unwrap());
    }

    #[test]
    fn test_deserialize_type_3() {
        let raw_tx = r#"{
            "accessList": [],
            "chainId": "0x1",
            "data": null,
            "from": "0x52ee324f2bcd0c5363d713eb9f62d1ee47266ac1",
            "gas": "0x5208",
            "hash": "0x1fb55f6e31763cc5f77c3aa2f92d28415c771f9f34c17e280b70c2fe23837fed",
            "maxFeePerGas": "0x60b66031a",
            "maxPriorityFeePerGas": "0x0",
            "nonce": "0x6663",
            "to": "0x9be0c82d5ba973a9e6861695626d4f9983e80c88",
            "type": "0x3",
            "value": "0x0"
        }"#;

        let tx: MevBlockerTx = serde_json::from_str(raw_tx).unwrap();
        assert_eq!(tx.0.from(), address!("52ee324f2bcd0c5363d713eb9f62d1ee47266ac1"));
        assert_eq!(tx.0.tx_hash(), TxHash::from_str("1fb55f6e31763cc5f77c3aa2f92d28415c771f9f34c17e280b70c2fe23837fed").unwrap());
    }
}
