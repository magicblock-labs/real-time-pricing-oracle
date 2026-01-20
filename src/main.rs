mod args;
mod blockhash_cache;
mod instructions;
mod types;

mod stork {
    pub mod chain_pusher;
    pub mod price_parser;
}
mod pyth_lazer {
    pub mod chain_pusher;
    pub mod price_parser;
}

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
    get_auth_header, get_channel, get_price_feeds, get_private_key, get_solana_cluster,
    get_ws_urls, Args,
};
use crate::pyth_lazer::chain_pusher::PythChainPusher;
use crate::stork::chain_pusher::StorkChainPusher;
use crate::types::ChainPusher;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let private_key = get_private_key(args.private_key);
    let auth_header = get_auth_header(args.auth_header);
    let ws_urls = get_ws_urls(args.ws_url, args.ws_urls);
    let cluster_url = get_solana_cluster(args.cluster);
    let price_feeds = get_price_feeds(args.price_feeds);
    let channel = get_channel(args.channel);

    let payer = Keypair::from_base58_string(&private_key);
    info!(wallet_pubkey = ?payer.pubkey(), "Identity initialized");

    let chain_pusher: Arc<dyn ChainPusher> = if ws_urls.iter().any(|url| url.contains("stork")) {
        Arc::new(StorkChainPusher::new(&cluster_url, payer).await)
    } else {
        Arc::new(PythChainPusher::new(&cluster_url, payer).await)
    };

    loop {
        let mut last_error = None;

        for ws_url in &ws_urls {
            match run_websocket_client(&chain_pusher, ws_url, &auth_header, &price_feeds, &channel)
                .await
            {
                Ok(_) => break,
                Err(e) => {
                    error!(error = ?e, url = ws_url, "WebSocket connection failed, trying next URL");
                    last_error = Some(e);
                }
            }
        }

        // if all URLs fail, wait before trying again
        if let Some(e) = last_error {
            error!(error = ?e, "All WebSocket URLs failed, retrying in 3 seconds");
            time::sleep(Duration::from_secs(3)).await;
        }
    }
}

async fn run_websocket_client(
    chain_pusher: &Arc<dyn ChainPusher>,
    url: &str,
    auth_header: &str,
    price_feeds: &[String],
    channel: &str,
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
    let message_text = chain_pusher
        .feeds_subscription_msg(price_feeds, channel)
        .await?;

    info!(message = %message_text, "Subscribing to price feeds");

    websocket
        .write(message_text.as_bytes(), PayloadType::Text)
        .await?;

    info!("Subscribed to price feeds.");

    loop {
        buf.clear();
        let res = time::timeout(Duration::from_secs(30), websocket.read(&mut buf)).await;
        match res {
            Ok(Ok(message)) => match message {
                Message::Text => {
                    if let Err(e) = chain_pusher
                        .process_update(&String::from_utf8_lossy(&buf))
                        .await
                    {
                        warn!(error = ?e, message = %String::from_utf8_lossy(&buf), "Failed to parse price update")
                    } else {
                        debug!("Processed price updates");
                    }
                }
                Message::Close(_) => return Err("WebSocket closed".into()),
                Message::Ping(payload) => {
                    websocket.write(&payload, PayloadType::Pong).await?;
                }
                Message::Pong(_) => {
                    debug!("Received pong");
                }
                _ => {}
            },
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                debug!("Sending ping");
                websocket.write(&[], PayloadType::Ping).await?;
            }
        }
    }
}
