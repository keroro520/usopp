use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig, signature::Signature, transaction::Transaction,
};
use std::time::{Duration, Instant};

pub struct RpcClientManager {
    clients: Vec<RpcClient>,
}

impl RpcClientManager {
    pub fn new(rpc_urls: Vec<String>) -> Self {
        let clients = rpc_urls
            .into_iter()
            .map(|url| RpcClient::new_with_commitment(url, CommitmentConfig::confirmed()))
            .collect();

        Self { clients }
    }

    pub async fn send_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<Vec<(Signature, Duration)>> {
        let mut results = Vec::with_capacity(self.clients.len());

        for client in &self.clients {
            let start = Instant::now();
            let signature = client.send_transaction(transaction).await?;
            let send_time = start.elapsed();
            results.push((signature, send_time));
        }

        Ok(results)
    }
}
