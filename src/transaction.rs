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

    pub async fn build_transaction(&self) -> Result<Transaction> {
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

        Ok(transaction)
    }
}
