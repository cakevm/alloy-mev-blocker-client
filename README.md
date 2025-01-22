# Alloy MEV Blocker Client

This crate provides an extension to use the [Searchers API](https://docs.cow.fi/mevblocker/searchers/listening-for-transactions) of [MEV Blocker](https://cow.fi/mev-blocker) with [Alloy](https://github.com/alloy-rs/alloy). Since the signature fields are stripped, the parsing of the transaction fails silently in Alloy. For that reason, this extension adds those fields on-the-fly during the deserialization so that a pending transaction can be deserialized into an `alloy_rpc_types_eth::Transaction`.


# Why not fix this in Alloy?
It is hard to tell who is right here. Alloy has a valid point to require all fields for a valid transaction. For some clients with less strict typing, the parsing of the transaction works. This does not require any workaround and for that reason, the API of MEV Blocker is for many people easy to use. For Alloy, it is not possible to parse the transaction without the signature fields.

# Usage
See `subscribe_mev_blocker.rs` in [examples](./examples) for a full usage examples. 

Example usage:
```rust
let ws_client = WsConnect::new(MEV_BLOCKER_SEARCHERS_URL);
let provider = ProviderBuilder::new().on_ws(ws_client).await?;

let subscription = provider.subscribe_mev_blocker_pending_transactions().await?;
```

# Acknowledgements
Many thanks to the [CoW DAO](https://cow.fi/) to provide such an API. And many thanks to the [alloy-rs](https://github.com/alloy-rs) team.

# License
This project is licensed under the [Apache 2.0](./LICENSE-APACHE) or [MIT](./LICENSE-MIT).