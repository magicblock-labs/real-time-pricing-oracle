use clap::{Parser, ValueEnum};
use solana_sdk::signature::Keypair;

#[derive(Debug, Clone, ValueEnum)]
pub enum ChannelType {
    #[value(name = "real_time")]
    RealTime,
    #[value(name = "fixed_rate@1ms")]
    FixedRate1ms,
    #[value(name = "fixed_rate@50ms")]
    FixedRate50ms,
    #[value(name = "fixed_rate@200ms")]
    FixedRate200ms,
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ChannelType::RealTime => write!(f, "real_time"),
            ChannelType::FixedRate1ms => write!(f, "fixed_rate@1ms"),
            ChannelType::FixedRate50ms => write!(f, "fixed_rate@50ms"),
            ChannelType::FixedRate200ms => write!(f, "fixed_rate@200ms"),
        }
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(long, help = "Private key for the Solana wallet")]
    pub private_key: Option<String>,
    #[arg(long, help = "Authorization header for the WebSocket connection")]
    pub auth_header: Option<String>,
    #[arg(long, help = "WebSocket URL for the price feed")]
    pub ws_url: Option<String>,
    #[arg(long, help = "Solana cluster URL")]
    pub cluster: Option<String>,
    #[arg(long, help = "Comma-separated list of price feeds")]
    pub price_feeds: Option<String>,
    #[arg(
        long,
        help = "Channel of the WebSocket to subscribe to (real_time, fixed_rate@1ms, fixed_rate@50ms, fixed_rate@200ms)"
    )]
    pub channel: Option<ChannelType>,
}

pub fn get_ws_url(cli_url: Option<String>) -> String {
    std::env::var("ORACLE_WS_URL")
        .ok()
        .or(cli_url)
        .unwrap_or_else(|| "ws://localhost:8765".to_string())
}

pub fn get_auth_header(cli_auth: Option<String>) -> String {
    std::env::var("ORACLE_AUTH_HEADER")
        .ok()
        .or(cli_auth)
        .expect(
            "ORACLE_AUTH_HEADER environment variable or --auth-header argument must be provided",
        )
}

pub fn get_solana_cluster(cli_cluster: Option<String>) -> String {
    std::env::var("SOLANA_CLUSTER")
        .ok()
        .or(cli_cluster)
        .unwrap_or_else(|| "https://devnet.magicblock.app/".to_string())
}

pub fn get_price_feeds(cli_feeds: Option<String>) -> Vec<String> {
    std::env::var("ORACLE_PRICE_FEEDS")
        .ok()
        .or(cli_feeds)
        .unwrap_or_else(|| "SOLUSD".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect()
}

pub fn get_private_key(cli_key: Option<String>) -> String {
    std::env::var("ORACLE_PRIVATE_KEY")
        .ok()
        .or(cli_key)
        .unwrap_or(Keypair::new().to_base58_string())
}

pub fn get_channel(cli_channel: Option<ChannelType>) -> String {
    let valid_values = ChannelType::value_variants()
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>();

    std::env::var("ORACLE_CHANNEL")
        .map(|env_channel| {
            ChannelType::value_variants()
                .iter()
                .find(|variant| {
                    variant.to_string().eq_ignore_ascii_case(&env_channel)
                })
                .map(|v| v.to_string())
                .unwrap_or_else(|| {
                    panic!(
                        "Invalid ORACLE_CHANNEL value: '{}'. Accepted values: {}",
                        env_channel,
                        valid_values.join(", ")
                    )
                })
        })
        .ok()
        .or(cli_channel.map(|c| c.to_string()))
        .unwrap_or_else(|| ChannelType::FixedRate50ms.to_string())
}