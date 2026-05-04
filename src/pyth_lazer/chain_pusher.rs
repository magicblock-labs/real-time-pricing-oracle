use crate::blockhash_cache::BlockhashCache;
use crate::instructions::update_price_feed;
use crate::pyth_lazer::price_parser::parse_price_update;
use crate::types::{ChainPusher, UpdateData};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::collections::HashSet;
use tracing::info;
use url::Url;

pub struct PythChainPusher {
    rpc_client: RpcClient,
    payer: Keypair,
    provider: String,
    blockhash_cache: BlockhashCache,
    http_client: reqwest::Client,
}

#[async_trait]
impl ChainPusher for PythChainPusher {
    async fn new(rpc_url: &str, payer_keypair: Keypair) -> Self {
        let rpc_client = RpcClient::new(rpc_url.to_string());
        let rpc_clone = rpc_client.get_inner_client().clone();

        PythChainPusher {
            rpc_client,
            payer: payer_keypair,
            provider: "pyth-lazer".to_string(),
            blockhash_cache: BlockhashCache::new(rpc_clone).await,
            http_client: reqwest::Client::new(),
        }
    }

    async fn feeds_subscription_msg(
        &self,
        price_feeds: &[String],
        channel: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let symbols = self.get_pyth_symbols(price_feeds).await?;
        let price_feed_ids: Vec<i32> = price_feeds
            .iter()
            .filter_map(|feed| {
                symbols
                    .iter()
                    .find(|symbol| symbol.name == *feed)
                    .map(|symbol| symbol.pyth_lazer_id)
            })
            .collect();

        let found_feeds: HashSet<&str> =
            symbols.iter().map(|symbol| symbol.name.as_str()).collect();
        let missing_feeds: Vec<&str> = price_feeds
            .iter()
            .map(String::as_str)
            .filter(|feed| !found_feeds.contains(feed))
            .collect();
        if !missing_feeds.is_empty() {
            return Err(format!("Unknown Pyth price feed(s): {}", missing_feeds.join(", ")).into());
        }

        let subscribe_message = serde_json::json!({
            "type": "subscribe",
            "subscriptionId": 0,
            "priceFeedIds": price_feed_ids,
            "properties": ["price", "feedUpdateTimestamp"],
            "formats": ["solana"],
            "deliveryFormat": "json",
            "jsonBinaryEncoding": "hex",
            "channel": channel,
            "ignoreInvalidFeeds": true,
        });
        Ok(serde_json::to_string(&subscribe_message).expect("Failed to serialize message"))
    }

    async fn process_update(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let updates = parse_price_update(message)?;
        self.send_price_updates(&updates).await
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PythSymbol {
    pyth_lazer_id: i32,
    name: String,
    symbol: String,
    description: String,
    asset_type: String,
    exponent: i32,
    cmc_id: Option<i32>,
    interval: Option<String>,
    #[serde(flatten)]
    extra: std::collections::HashMap<String, serde_json::Value>,
}

impl PythChainPusher {
    async fn send_price_updates(
        &self,
        updates: &Vec<UpdateData>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut ixs = vec![];
        for update in updates {
            let ix = update_price_feed(&self.payer.pubkey(), &self.provider, update);
            ixs.push(ix);
        }
        let tx = Transaction::new_signed_with_payer(
            &ixs,
            Some(&self.payer.pubkey()),
            &[&self.payer],
            self.blockhash_cache.get_blockhash().await,
        );

        let options = RpcSendTransactionConfig {
            skip_preflight: true,
            ..Default::default()
        };
        let rpc_client = self.rpc_client.get_inner_client().clone();
        tokio::spawn(async move {
            match rpc_client.send_transaction_with_config(&tx, options).await {
                Ok(signature) => {
                    info!("\nTransaction sent: {}", signature);
                }
                Err(err) => {
                    info!("\nTransaction error: {}", err);
                }
            }
        });
        Ok(())
    }

    async fn get_pyth_symbols(
        &self,
        price_feeds: &[String],
    ) -> Result<Vec<PythSymbol>, Box<dyn std::error::Error>> {
        let mut symbols = Vec::new();

        for feed in price_feeds {
            let mut url = Url::parse("https://pyth.dourolabs.app/v1/symbols")?;
            url.query_pairs_mut().append_pair("query", feed);

            let response = self.http_client.get(url).send().await?;

            if !response.status().is_success() {
                return Err(format!(
                    "Pyth symbols API returned {} while looking up {}",
                    response.status(),
                    feed
                )
                .into());
            }

            let mut matching_symbols = response.json::<Vec<PythSymbol>>().await?;
            symbols.append(&mut matching_symbols);
        }

        Ok(symbols)
    }
}
