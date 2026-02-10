use alloy_consensus::{Signed, TxEip1559, TxEip2930, TxEip4844, TxEip4844Variant, TxEip7702, TxEnvelope, TxLegacy, transaction::Recovered};
use alloy_eips::eip2930::AccessList;
use alloy_primitives::{Address, B256, Bytes, Signature, TxKind, U256};
use alloy_provider::{Network, Provider};
use alloy_rpc_types_eth::Transaction;
use alloy_transport::TransportResult;
use async_trait::async_trait;
use serde::{Deserialize, Deserializer};

pub const MEV_BLOCKER_SEARCHERS_URL: &str = "wss://searchers.mevblocker.io";

#[derive(Debug, Clone)]
pub struct MevBlockerTx(pub Transaction<TxEnvelope>);

/// Raw transaction from the MEV Blocker API.
/// API doc: https://docs.mevblocker.io/how-to/searchers/listen
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMevBlockerTx {
    #[serde(default, with = "alloy_serde::quantity::opt")]
    chain_id: Option<u64>,
    #[serde(default)]
    to: Option<Address>,
    #[serde(default)]
    value: U256,
    #[serde(default, deserialize_with = "deserialize_data")]
    data: Bytes,
    #[serde(default)]
    access_list: AccessList,
    #[serde(with = "alloy_serde::quantity")]
    nonce: u64,
    #[serde(default, with = "alloy_serde::quantity::opt")]
    gas_price: Option<u128>,
    #[serde(default, with = "alloy_serde::quantity::opt")]
    max_priority_fee_per_gas: Option<u128>,
    #[serde(default, with = "alloy_serde::quantity::opt")]
    max_fee_per_gas: Option<u128>,
    #[serde(with = "alloy_serde::quantity")]
    gas: u64,
    #[serde(default, rename = "type", with = "alloy_serde::quantity::opt")]
    tx_type: Option<u8>,
    hash: B256,
    from: Address,
}

/// Deserialize the `data` field which can be a hex string or null.
fn deserialize_data<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Bytes>::deserialize(deserializer)?.unwrap_or_default())
}

impl<'de> Deserialize<'de> for MevBlockerTx {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawMevBlockerTx::deserialize(deserializer)?;

        let sig = Signature::new(U256::ZERO, U256::ZERO, false);
        let tx_type = raw.tx_type.unwrap_or(0);

        let envelope = match tx_type {
            0 => TxEnvelope::Legacy(Signed::new_unchecked(
                TxLegacy {
                    chain_id: raw.chain_id,
                    nonce: raw.nonce,
                    gas_price: raw.gas_price.unwrap_or(0),
                    gas_limit: raw.gas,
                    to: raw.to.map_or(TxKind::Create, TxKind::Call),
                    value: raw.value,
                    input: raw.data,
                },
                sig,
                raw.hash,
            )),
            1 => TxEnvelope::Eip2930(Signed::new_unchecked(
                TxEip2930 {
                    chain_id: raw.chain_id.unwrap_or(1),
                    nonce: raw.nonce,
                    gas_price: raw.gas_price.unwrap_or(0),
                    gas_limit: raw.gas,
                    to: raw.to.map_or(TxKind::Create, TxKind::Call),
                    value: raw.value,
                    access_list: raw.access_list,
                    input: raw.data,
                },
                sig,
                raw.hash,
            )),
            2 => TxEnvelope::Eip1559(Signed::new_unchecked(
                TxEip1559 {
                    chain_id: raw.chain_id.unwrap_or(1),
                    nonce: raw.nonce,
                    gas_limit: raw.gas,
                    max_fee_per_gas: raw.max_fee_per_gas.unwrap_or(0),
                    max_priority_fee_per_gas: raw.max_priority_fee_per_gas.unwrap_or(0),
                    to: raw.to.map_or(TxKind::Create, TxKind::Call),
                    value: raw.value,
                    access_list: raw.access_list,
                    input: raw.data,
                },
                sig,
                raw.hash,
            )),
            3 => TxEnvelope::Eip4844(Signed::new_unchecked(
                TxEip4844Variant::TxEip4844(TxEip4844 {
                    chain_id: raw.chain_id.unwrap_or(1),
                    nonce: raw.nonce,
                    gas_limit: raw.gas,
                    max_fee_per_gas: raw.max_fee_per_gas.unwrap_or(0),
                    max_priority_fee_per_gas: raw.max_priority_fee_per_gas.unwrap_or(0),
                    to: raw.to.unwrap_or_default(),
                    value: raw.value,
                    access_list: raw.access_list,
                    blob_versioned_hashes: vec![],
                    max_fee_per_blob_gas: 0,
                    input: raw.data,
                }),
                sig,
                raw.hash,
            )),
            4 => TxEnvelope::Eip7702(Signed::new_unchecked(
                TxEip7702 {
                    chain_id: raw.chain_id.unwrap_or(1),
                    nonce: raw.nonce,
                    gas_limit: raw.gas,
                    max_fee_per_gas: raw.max_fee_per_gas.unwrap_or(0),
                    max_priority_fee_per_gas: raw.max_priority_fee_per_gas.unwrap_or(0),
                    to: raw.to.unwrap_or_default(),
                    value: raw.value,
                    access_list: raw.access_list,
                    authorization_list: vec![],
                    input: raw.data,
                },
                sig,
                raw.hash,
            )),
            other => return Err(serde::de::Error::custom(format!("unknown tx type: 0x{other:x}"))),
        };

        Ok(MevBlockerTx(Transaction {
            inner: Recovered::new_unchecked(envelope, raw.from),
            block_hash: None,
            block_number: None,
            transaction_index: None,
            effective_gas_price: None,
        }))
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

        let mut call = self.client().request("eth_subscribe", ("mevblocker_partialPendingTransactions",));
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
