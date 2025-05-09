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

    // Initialize results collector
    let mut results = BenchmarkResults::new();

    // Create channel for WebSocket notifications
    let (tx, mut rx) = mpsc::channel(100);

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

    // Process each transaction
    for transaction in transactions.iter() {
        // Send transaction to all nodes
        let send_results = rpc_manager.send_transaction(transaction).await?;

        // Start WebSocket monitoring for each node
        for (node_idx, (signature, _send_duration)) in send_results.iter().enumerate() {
            if node_idx < config.rpc_nodes.len() {
                let ws_manager = websocket::WebSocketHandle::new(
                    config.rpc_nodes[node_idx].ws_url.clone(),
                    *signature,
                    tx.clone(),
                );

                // Spawn WebSocket monitoring task
                tokio::spawn(async move {
                    if let Err(e) = ws_manager.monitor_confirmation().await {
                        tracing::error!("WebSocket monitoring error: {:#}", e);
                        std::process::exit(1);
                    }
                });
            }
        }

        // Wait for confirmation from all nodes
        let mut node_metrics_for_tx = Vec::new();
        for _ in 0..send_results.len() {
            if let Some((signature, confirm_time)) = rx.recv().await {
                let mut original_node_idx = None;
                for (idx, (s, _)) in send_results.iter().enumerate() {
                    if *s == signature {
                        original_node_idx = Some(idx);
                        break;
                    }
                }

                if let Some(node_idx) = original_node_idx {
                    if node_idx < config.rpc_nodes.len() {
                        let explorer_url =
                            format!("https://solscan.io/tx/{}?cluster=devnet", signature);
                        let metrics = NodeMetrics {
                            nodename: config.rpc_nodes[node_idx].http_url.clone(),
                            explorer_url,
                            confirm_time,
                        };
                        node_metrics_for_tx.push(metrics);
                    }
                } else {
                    tracing::warn!(
                        "Received confirmation for an unknown signature: {}",
                        signature
                    );
                }
            }
        }

        // Add metrics to results
        for metrics in node_metrics_for_tx {
            results.add_metrics(metrics);
        }
    }

    // Output results
    println!("{}", results.to_json());

    Ok(())
}
