use alloy_mev_blocker_client::{MEV_BLOCKER_SEARCHERS_URL, MevBlockerApi};
use alloy_provider::ProviderBuilder;
use alloy_transport::TransportError;
use alloy_transport_ws::WsConnect;
use futures_util::StreamExt;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<(), TransportError> {
    rustls::crypto::aws_lc_rs::default_provider().install_default().expect("Failed to install default CryptoProvider");

    // Change this to "trace" to see websocket messages that alloy receives
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from("info")).init();

    // Connect to MEV Blocker searchers API
    let ws_client = WsConnect::new(MEV_BLOCKER_SEARCHERS_URL);
    let provider = ProviderBuilder::new().connect_ws(ws_client).await?;

    // Here we subscribe to MEV Blocker pending transactions
    let subscription = provider.subscribe_mev_blocker_pending_transactions().await?;

    let mut stream = subscription.into_stream();
    info!("Subscribed to MEV Blocker pending transactions");

    // This loop will print all pending transactions received from MEV Blocker
    while let Some(event) = stream.next().await {
        info!("Received: {:?}", event);
    }

    Ok(())
}
