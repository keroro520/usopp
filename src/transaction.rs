use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use std::time::{Duration, Instant};

pub struct TransactionBuilder {
    rpc_client: RpcClient,
    from_keypair: Keypair,
    to_pubkey: Pubkey,
    amount_lamports: u64,
}

impl TransactionBuilder {
    pub fn new(
        rpc_url: String,
        from_keypair: Keypair,
        to_pubkey: Pubkey,
        amount_lamports: u64,
    ) -> Self {
        Self {
            rpc_client: RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed()),
            from_keypair,
            to_pubkey,
            amount_lamports,
        }
    }

    pub async fn build_transaction(&self) -> Result<(Transaction, Duration)> {
        let start = Instant::now();

        // Get recent blockhash
        let recent_blockhash = self.rpc_client.get_latest_blockhash().await?;

        // Create transfer instruction
        let transfer_instruction = system_instruction::transfer(
            &self.from_keypair.pubkey(),
            &self.to_pubkey,
            self.amount_lamports,
        );

        // Build and sign transaction
        let message = Message::new(&[transfer_instruction], Some(&self.from_keypair.pubkey()));
        let mut transaction = Transaction::new_unsigned(message);
        transaction.sign(&[&self.from_keypair], recent_blockhash);

        let build_time = start.elapsed();
        Ok((transaction, build_time))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signature::Signer;

    #[tokio::test]
    async fn test_build_transaction() {
        let from_keypair = Keypair::new();
        let to_keypair = Keypair::new();
        let amount = 1_000_000; // 0.001 SOL

        let builder = TransactionBuilder::new(
            "https://api.mainnet-beta.solana.com".to_string(),
            from_keypair,
            to_keypair.pubkey(),
            amount,
        );

        let (transaction, build_time) = builder.build_transaction().await.unwrap();
        
        assert!(build_time < Duration::from_secs(1));
        assert_eq!(transaction.message.instructions.len(), 1);
        assert!(transaction.verify().is_ok());
    }
} 