use crate::instructions::update_price_feed;
use crate::price_parser::UpdateData;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use tracing::info;

pub struct ChainPusher {
    rpc_client: RpcClient,
    payer: Keypair,
    provider: String,
}

impl ChainPusher {
    pub fn new(rpc_url: &str, payer_keypair: Keypair) -> Self {
        let rpc_client = RpcClient::new(rpc_url.to_string());

        ChainPusher {
            rpc_client,
            payer: payer_keypair,
            provider: "stork".to_string(),
        }
    }

    pub async fn send_price_updates(
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
            self.rpc_client.get_latest_blockhash()?,
        );
        let signature = self.rpc_client.send_transaction(&tx)?;
        info!("\nTransaction sent: {}", signature);
        Ok(())
    }
}
