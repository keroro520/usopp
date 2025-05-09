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
use std::time::Instant;
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

    // Initialize results collector
    let mut results = BenchmarkResults::new();

    // Create channel for WebSocket notifications
    let (tx, mut rx) = mpsc::channel(100);

    // Pre-build all transactions
    let mut transactions_with_build_time = Vec::new();
    for i in 0..config.num_transactions {
        let amount = config.amount_lamports + i as u64;
        let builder = transaction::TransactionBuilder::new(
            config.rpc_nodes[0].http_url.clone(),
            keypair.insecure_clone(),
            recipient_pubkey,
            amount,
        );
        let (transaction, build_time) = builder.build_transaction().await?;
        transactions_with_build_time.push((transaction, build_time));
    }

    // Process each transaction
    for (transaction, build_time) in transactions_with_build_time {
        let start_time = Instant::now();

        // Send transaction to all nodes
        let send_results = rpc_manager.send_transaction(&transaction).await?;

        // Start WebSocket monitoring for each node
        for (i, (signature, _)) in send_results.iter().enumerate() {
            let ws_manager = websocket::WebSocketHandle::new(
                config.rpc_nodes[i].ws_url.clone(),
                *signature,
                tx.clone(),
            );

            // Spawn WebSocket monitoring task
            tokio::spawn(async move {
                if let Err(e) = ws_manager.monitor_confirmation(start_time).await {
                    tracing::error!("WebSocket monitoring error: {:#}", e);
                    std::process::exit(1);
                }
            });
        }

        // Wait for confirmation from all nodes
        let mut node_metrics = Vec::new();
        for (i, _) in send_results
            .iter()
            .enumerate()
            .take(rpc_manager.num_clients())
        {
            if let Some((signature, confirm_time)) = rx.recv().await {
                let explorer_url = format!("https://solscan.io/tx/{}?cluster=devnet", signature);
                let metrics = NodeMetrics {
                    nodename: config.rpc_nodes[i].http_url.clone(),
                    explorer_url,
                    build_time,
                    send_time: send_results[i].1,
                    confirm_time,
                };
                node_metrics.push(metrics);
            }
        }

        // Add metrics to results
        for metrics in node_metrics {
            results.add_metrics(metrics);
        }
    }

    // Output results
    println!("{}", results.to_json());

    Ok(())
}
