mod config;
mod metrics;
mod rpc;
mod transaction;
mod websocket;

use anyhow::Result;
use clap::Parser;
use config::{BenchmarkConfig, CliArgs};
use metrics::{BenchmarkResults, NodeMetrics};
use rpc::RpcClientManager;
use solana_sdk::pubkey;
use solana_sdk::signature::read_keypair_file;
use std::str::FromStr;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Parse command line arguments
    let args = CliArgs::parse();

    // Load configuration
    let config = BenchmarkConfig::from_file(&args.config)?;

    // Parse recipient pubkey
    let recipient_pubkey = pubkey::Pubkey::from_str(&config.recipient)
        .map_err(|e| anyhow::anyhow!("Invalid recipient pubkey: {}", e))?;

    // Load keypair
    let keypair = read_keypair_file(&config.keypair_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to read keypair from {:?}: {}",
            config.keypair_path,
            e
        )
    })?;

    // Initialize RPC clients
    let rpc_urls: Vec<String> = config
        .rpc_nodes
        .iter()
        .map(|node| node.http_url.clone())
        .collect();
    let rpc_manager = RpcClientManager::new(rpc_urls);

    // Pre-build all transactions
    let mut transactions = Vec::new();
    for i in 0..config.num_transactions {
        let amount = config.amount_lamports + i as u64;
        let builder = transaction::TransactionBuilder::new(
            config.rpc_nodes[0].http_url.clone(),
            keypair.insecure_clone(),
            recipient_pubkey,
            amount,
        );
        let transaction = builder.build_transaction().await?;
        transactions.push(transaction);
    }

    // Send transactions
    tracing::info!(
        "Sending {} transactions to {} RPC nodes...",
        transactions.len(),
        config.rpc_nodes.len()
    );
    // Ensuring the correct plural method name is used.
    rpc_manager.send_transactions(&transactions);
    tracing::info!("All transactions sent via HTTP.");

    Ok(())
}
