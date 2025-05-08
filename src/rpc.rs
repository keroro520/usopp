use anyhow::{Context, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};
use std::time::{Duration, Instant};
use tokio::time::sleep;

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

    pub async fn request_airdrop(&self, pubkey: &Pubkey, lamports: u64) -> Result<()> {
        const MAX_RETRIES: u32 = 5;
        const RETRY_DELAY: Duration = Duration::from_secs(5);
        const AIRDROP_AMOUNT: u64 = 1_000_000_000; // 1 SOL

        let client = &self.clients[0];

        for attempt in 1..=MAX_RETRIES {
            println!("Airdrop attempt {} of {}", attempt, MAX_RETRIES);
            
            match client.request_airdrop(pubkey, AIRDROP_AMOUNT).await {
                Ok(signature) => {
                    println!("Airdrop request successful, waiting for confirmation...");
                    match client.confirm_transaction(&signature).await {
                        Ok(_) => {
                            // Verify the balance after airdrop
                            match client.get_balance(pubkey).await {
                                Ok(balance) => {
                                    println!("Current balance: {} lamports", balance);
                                    if balance >= AIRDROP_AMOUNT {
                                        return Ok(());
                                    }
                                    println!("Balance verification failed. Expected >= {}, got {}", AIRDROP_AMOUNT, balance);
                                }
                                Err(e) => println!("Failed to verify balance: {}", e),
                            }
                        }
                        Err(e) => println!("Failed to confirm airdrop: {}", e),
                    }
                }
                Err(e) => println!("Airdrop request failed: {}", e),
            }

            if attempt < MAX_RETRIES {
                println!("Waiting {} seconds before retry...", RETRY_DELAY.as_secs());
                sleep(RETRY_DELAY).await;
            }
        }

        Err(anyhow::anyhow!(
            "airdrop request failed after {} attempts",
            MAX_RETRIES
        ))
    }

    pub async fn send_transaction(&self, transaction: &Transaction) -> Result<Vec<(Signature, Duration)>> {
        let mut results = Vec::with_capacity(self.clients.len());
        
        for client in &self.clients {
            let start = Instant::now();
            let signature = client.send_transaction(transaction).await?;
            let send_time = start.elapsed();
            results.push((signature, send_time));
        }

        Ok(results)
    }

    pub fn num_clients(&self) -> usize {
        self.clients.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{
        message::Message,
        pubkey::Pubkey,
        signature::{Keypair, Signer},
        system_instruction,
    };

    #[tokio::test]
    async fn test_send_transaction() {
        let from_keypair = Keypair::new();
        let to_keypair = Keypair::new();
        let amount = 1_000_000; // 0.001 SOL

        // Create a test transaction
        let recent_blockhash = from_keypair.sign_message(&[0u8]);
        let transfer_instruction = system_instruction::transfer(
            &from_keypair.pubkey(),
            &to_keypair.pubkey(),
            amount,
        );
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