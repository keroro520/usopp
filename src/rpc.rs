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

    /* // Commenting out unused method
    pub fn num_clients(&self) -> usize {
        self.clients.len()
    }
    */
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{
        message::Message,
        signature::{Keypair, Signer},
        system_instruction,
    };

    #[tokio::test]
    async fn test_send_transaction() {
        let from_keypair = Keypair::new();
        let to_keypair = Keypair::new();
        let amount = 1_000_000; // 0.001 SOL

        // Create a test transaction
        let rpc_client = RpcClient::new_with_commitment(
            "https://api.devnet.solana.com".to_string(),
            CommitmentConfig::confirmed(),
        );
        let recent_blockhash = rpc_client.get_latest_blockhash().await.unwrap();
        let transfer_instruction =
            system_instruction::transfer(&from_keypair.pubkey(), &to_keypair.pubkey(), amount);
        let message = Message::new(&[transfer_instruction], Some(&from_keypair.pubkey()));
        let mut transaction = Transaction::new_unsigned(message);
        transaction.sign(&[&from_keypair], recent_blockhash);

        // Create RPC client manager with test URLs
        let manager = RpcClientManager::new(vec![
            "https://api.devnet.solana.com".to_string(),
            "https://devnet.genesysgo.net".to_string(),
        ]);

        let results = manager.send_transaction(&transaction).await.unwrap();

        assert_eq!(results.len(), 2);
        for (signature, send_time) in results {
            assert!(send_time < Duration::from_secs(1));
            assert_eq!(signature, transaction.signatures[0]);
        }
    }
}
