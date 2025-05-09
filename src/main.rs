mod config;
mod report;
mod rpc;
mod transaction;
mod websocket;

use anyhow::Result;
use clap::Parser;
use config::{BenchmarkConfig, CliArgs};
use rpc::RpcClientManager;
use solana_sdk::pubkey;
use solana_sdk::signature::read_keypair_file;
use std::fs;
use std::str::FromStr;
use tokio::task::JoinHandle;
use websocket::{ConfirmationResult, WebSocketHandle};

// Type alias for WebSocket task results
type NodeName = String;
type NodeConfirmationResults = Vec<ConfirmationResult>;
type WebSocketTaskResult = Result<(NodeName, NodeConfirmationResults)>;
type WebSocketJoinHandle = JoinHandle<WebSocketTaskResult>;

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

    // Pre-build all transactions
    let mut transactions = Vec::new();
    // Also collect signatures for WebSocket monitoring
    let mut transaction_signatures = Vec::new();

    tracing::info!("Building {} transactions...", config.num_transactions);
    for i in 0..config.num_transactions {
        let amount = config.amount_lamports + i as u64; // Ensure unique amount for unique hash if needed
        let builder = transaction::TransactionBuilder::new(
            config.rpc_nodes[0].http_url.clone(), // Using first node for tx building context
            keypair.insecure_clone(),
            recipient_pubkey,
            amount,
        );
        let built_transaction = builder.build_transaction().await?;
        transaction_signatures.push(built_transaction.signatures[0]);
        transactions.push(built_transaction);
    }
    tracing::info!("All {} transactions built.", transactions.len());

    // Spawn WebSocket monitoring threads
    let mut ws_handles: Vec<WebSocketJoinHandle> = Vec::new(); // Using type alias
    tracing::info!(
        "Spawning WebSocket monitoring threads for {} RPC nodes and {} signatures...",
        config.rpc_nodes.len(),
        transaction_signatures.len()
    );

    for rpc_node_config in &config.rpc_nodes {
        let node_name = rpc_node_config.name.clone();
        let node_ws_url = rpc_node_config.ws_url.clone();
        let signatures_clone = transaction_signatures.clone();
        // let ws_result_tx_clone = ws_result_tx.clone(); // Removed

        let handle = tokio::spawn(async move {
            tracing::info!(
                "Connecting WebSocket to {} ({}) ...",
                node_ws_url,
                node_name
            );
            let ws_handle = WebSocketHandle::new(node_ws_url.clone(), signatures_clone); // tx_clone removed
            match ws_handle.monitor_confirmation().await {
                // Now returns Result<Vec<...>>
                Ok(confirmations) => {
                    tracing::info!(
                        "WebSocket monitoring for {} ({}) completed, {} confirmations received.",
                        node_ws_url,
                        node_name,
                        confirmations.len()
                    );
                    Ok((node_name, confirmations)) // Return node_name along with confirmations
                }
                Err(e) => {
                    tracing::error!(
                        "WebSocket monitoring failed for {} ({}): {}. Thread finishing.",
                        node_ws_url,
                        node_name,
                        e
                    );
                    Err(e) // Propagate error out of the spawned task
                }
            }
        });
        ws_handles.push(handle);
    }

    // Initialize RPC clients (HTTP)
    let rpc_http_urls: Vec<String> = config
        .rpc_nodes
        .iter()
        .map(|node| node.http_url.clone())
        .collect();
    let rpc_manager = RpcClientManager::new(rpc_http_urls);

    // Send transactions via HTTP
    tracing::info!(
        "Sending {} transactions to {} RPC nodes via HTTP...",
        transactions.len(),
        config.rpc_nodes.len()
    );
    // This is currently synchronous in its internal implementation, but it's fine.
    rpc_manager.send_transactions(&transactions);
    tracing::info!("All transactions sent via HTTP.");

    // Collect results from WebSocket threads by awaiting handles
    let mut all_node_confirmations: Vec<(NodeName, NodeConfirmationResults)> = Vec::new();
    for handle in ws_handles {
        match handle.await {
            // This is Result<WebSocketTaskResult, JoinError>
            Ok(Ok(node_data)) => {
                all_node_confirmations.push(node_data);
            }
            Ok(Err(e)) => {
                tracing::error!("A WebSocket monitoring task returned an error: {}", e);
            }
            Err(e) => {
                tracing::error!(
                    "A WebSocket monitoring task failed to join (panicked): {}",
                    e
                );
            }
        }
    }

    // Generate and print the report
    if !all_node_confirmations.is_empty() {
        tracing::info!("Generating benchmark report...");
        let report_markdown = report::generate_report_markdown(&all_node_confirmations);
        tracing::info!(
            "
Benchmark Report:
{}",
            report_markdown
        );

        // Optionally, write to a file:
        match fs::write("benchmark_report.md", &report_markdown) {
            Ok(_) => tracing::info!("Benchmark report successfully written to benchmark_report.md"),
            Err(e) => tracing::error!("Failed to write benchmark report to file: {}", e),
        }
    } else {
        tracing::info!("No node confirmations received, skipping report generation.");
    }

    Ok(())
}
