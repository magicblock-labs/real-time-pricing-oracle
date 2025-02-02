mod args;
mod chain_pusher;
mod instructions;
mod price_parser;

use bytes::BytesMut;
use clap::Parser;
use native_tls::TlsConnector as NativeTlsConnector;
use ratchet_rs::{
    deflate::DeflateExtProvider, HeaderValue, Message, PayloadType, TryIntoRequest, UpgradedClient,
    WebSocketClientBuilder, WebSocketStream,
};
use solana_sdk::signature::{Keypair, Signer};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::time::{self, Duration};
use tokio_native_tls::TlsConnector;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::args::{
    get_auth_header, get_price_feeds, get_private_key, get_solana_cluster, get_ws_url, Args,
};
use crate::chain_pusher::ChainPusher;
use crate::price_parser::parse_price_update;

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
    let chain_pusher = Arc::new(ChainPusher::new(&cluster_url, payer));

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
    chain_pusher: Arc<ChainPusher>,
    url: &str,
    auth_header: &str,
    price_feeds: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    info!(url = %url, "Establishing WebSocket connection");

    let url = Url::parse(url)?;
    let host = url.host_str().ok_or("Missing host in URL")?;
    let address = format!("{}:{}", host, url.port().unwrap_or(443));
    let stream = TcpStream::connect(address).await?;

    let mut request = url.clone().try_into_request()?;
    request
        .headers_mut()
        .insert("AUTHORIZATION", HeaderValue::from_str(auth_header)?);

    let stream: Box<dyn WebSocketStream> = if url.scheme() == "wss" {
        let tls_connector = TlsConnector::from(NativeTlsConnector::new()?);
        Box::new(tls_connector.connect(host, stream).await?)
    } else {
        Box::new(stream)
    };

    let upgraded = WebSocketClientBuilder::default()
        .extension(DeflateExtProvider::default())
        .subscribe(stream, request)
        .await?;

    let UpgradedClient { mut websocket, .. } = upgraded;
    info!("WebSocket connected.");

    let mut buf = BytesMut::new();
    let subscribe_message = serde_json::json!({
        "type": "subscribe",
        "data": price_feeds,
    });
    let message_text = serde_json::to_string(&subscribe_message)?;
    websocket
        .write(message_text.as_bytes(), PayloadType::Text)
        .await?;

    info!("Subscribed to price feeds.");

    while let Ok(message) = websocket.read(&mut buf).await {
        match message {
            Message::Text => match parse_price_update(&String::from_utf8_lossy(&buf)) {
                Ok(updates) => {
                    debug!(updates_count = updates.len(), "Processing price updates");
                    chain_pusher.send_price_updates(&updates).await?;
                }
                Err(e) => warn!(error = ?e, "Failed to parse price update"),
            },
            Message::Close(_) => return Err("WebSocket closed".into()),
            _ => {}
        }
        buf.clear();
    }
    Ok(())
}
