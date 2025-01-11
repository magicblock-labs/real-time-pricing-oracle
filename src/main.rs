mod args;
mod chain_pusher;
mod instructions;
mod price_parser;

use crate::args::{
    get_auth_header, get_price_feeds, get_private_key, get_solana_cluster, get_ws_url, Args,
};
use crate::chain_pusher::ChainPusher;
use crate::price_parser::parse_price_update;
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use solana_sdk::signature::{Keypair, Signer};
use tokio::time::{self, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{debug, error, info, warn};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::header::AUTHORIZATION;
use tungstenite::http::HeaderValue;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let private_key = get_private_key(args.private_key);
    let auth_header = get_auth_header(args.auth_header);
    let ws_url = get_ws_url(args.ws_url);
    let cluster_url = get_solana_cluster(args.cluster);
    let price_feeds = get_price_feeds(args.price_feeds);

    let payer = Keypair::from_base58_string(&private_key);
    info!(wallet_pubkey = ?payer.pubkey(), "Identity initialized");
    let chain_pusher = ChainPusher::new(&cluster_url, payer);
    let chain_pusher = std::sync::Arc::new(chain_pusher);
    loop {
        if let Err(e) =
            run_websocket_client(chain_pusher.clone(), &ws_url, &auth_header, &price_feeds).await
        {
            error!(error = ?e, "WebSocket connection error, attempting reconnection");
        }
        time::sleep(Duration::from_secs(5)).await;
    }
}
async fn run_websocket_client(
    chain_pusher: std::sync::Arc<ChainPusher>,
    url: &str,
    auth_header: &str,
    price_feeds: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    info!(url = %url, "Establishing WebSocket connection");

    let mut request = url.into_client_request()?;
    request
        .headers_mut()
        .insert(AUTHORIZATION, HeaderValue::from_str(auth_header)?);
    let (ws_stream, _) = connect_async(request).await?;

    info!("WebSocket connection established successfully");

    let (mut write, mut read) = ws_stream.split();
    let subscribe_message = serde_json::json!({
        "type": "subscribe",
        "data": price_feeds,
    });
    let message_text = serde_json::to_string(&subscribe_message)?;
    write.send(Message::text(message_text)).await?;

    info!("Subscribed successfully.");

    while let Some(msg) = read.next().await {
        match msg {
            Ok(message) => {
                if let Message::Text(msg) = message {
                    match parse_price_update(&msg.to_string()) {
                        Ok(updates) => {
                            debug!(updates_count = updates.len(), "Processing price updates");
                            chain_pusher.send_price_updates(&updates).await?;
                        }
                        Err(e) => warn!(error = ?e, "Failed to parse price update"),
                    }
                }
            }
            Err(e) => {
                error!(error = ?e, "WebSocket error occurred");
                break;
            }
        }
    }
    Ok(())
}
