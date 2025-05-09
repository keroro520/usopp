mod config;
mod metrics;
mod rpc;
mod transaction;
mod websocket;

use anyhow::Result;
use clap::Parser;
use config::{BenchmarkConfig, CliArgs};
use rpc::RpcClientManager;
use solana_sdk::pubkey;
use solana_sdk::signature::{read_keypair_file, Signature};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime};
use tokio::task::JoinHandle;
use websocket::{ConfirmationResult, WebSocketHandle};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Capture benchmark start time if needed for relative durations later
    let benchmark_start_time = Instant::now();
    let benchmark_start_system_time = SystemTime::now();

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
    let mut ws_handles: Vec<JoinHandle<Result<Vec<ConfirmationResult>>>> = Vec::new(); // Corrected JoinHandle type
    tracing::info!(
        "Spawning WebSocket monitoring threads for {} RPC nodes and {} signatures...",
        config.rpc_nodes.len(),
        transaction_signatures.len()
    );

    for rpc_node_config in &config.rpc_nodes {
        let node_ws_url = rpc_node_config.ws_url.clone();
        let signatures_clone = transaction_signatures.clone();
        // let ws_result_tx_clone = ws_result_tx.clone(); // Removed

        let handle = tokio::spawn(async move {
            tracing::info!("Connecting WebSocket to {}...", node_ws_url);
            let ws_handle = WebSocketHandle::new(node_ws_url.clone(), signatures_clone); // tx_clone removed
            match ws_handle.monitor_confirmation().await {
                // Now returns Result<Vec<...>>
                Ok(confirmations) => {
                    tracing::info!(
                        "WebSocket monitoring for {} completed, {} confirmations received.",
                        node_ws_url,
                        confirmations.len()
                    );
                    Ok(confirmations)
                }
                Err(e) => {
                    tracing::error!(
                        "WebSocket monitoring failed for {}: {}. Thread finishing.",
                        node_ws_url,
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
    let mut all_node_confirmations: Vec<Vec<ConfirmationResult>> = Vec::new();
    for handle in ws_handles {
        match handle.await {
            // This is Result<Result<Vec<ConfirmationResult>>, JoinError>
            Ok(Ok(node_confirmations)) => {
                all_node_confirmations.push(node_confirmations);
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

    tracing::info!("Processing collected WebSocket results...");

    let mut confirmed_transactions: HashMap<Signature, (SystemTime, u64)> = HashMap::new();
    let total_expected_confirmations = transaction_signatures.len();

    for node_confirmations_vec in all_node_confirmations {
        for (signature, timestamp, slot) in node_confirmations_vec {
            if !confirmed_transactions.contains_key(&signature) {
                let duration_since_start = timestamp
                    .duration_since(benchmark_start_system_time)
                    .unwrap_or_else(|_| Duration::from_secs(0));
                tracing::info!(
                    "CONFIRMED (from collected results): Signature {} at {:?} (took {:?}), slot {}. ({}/{})",
                    signature,
                    timestamp,
                    duration_since_start,
                    slot,
                    confirmed_transactions.len() + 1,
                    total_expected_confirmations
                );
                confirmed_transactions.insert(signature, (timestamp, slot));
            } else {
                // Potentially log if a signature is confirmed by multiple nodes, if relevant
                // Or if the timestamp/slot differs, which might be interesting data.
                tracing::debug!(
                    "DUPLICATE CONF (from collected results): Signature {} already processed. New data: {:?}, slot {}.",
                    signature, timestamp, slot
                );
            }
        }
    }

    if benchmark_start_time.elapsed() > Duration::from_secs(120)
        && confirmed_transactions.len() < total_expected_confirmations
    {
        tracing::warn!(
            "Benchmark timeout likely exceeded. Received {}/{} confirmations.",
            confirmed_transactions.len(),
            total_expected_confirmations
        );
    }

    tracing::info!(
        "Finished processing WebSocket results. {} unique transactions confirmed.",
        confirmed_transactions.len()
    );
    if !confirmed_transactions.is_empty() {
        for (sig, (timestamp, slt)) in &confirmed_transactions {
            let duration_since_start = timestamp
                .duration_since(benchmark_start_system_time)
                .unwrap_or_else(|_| Duration::from_secs(0));
            tracing::debug!(
                "  Signature: {}, Timestamp: {:?}, Slot: {}, Took: {:?}",
                sig,
                timestamp,
                slt,
                duration_since_start
            );
        }
    }

    // Wait for all WebSocket threads to finish - This is already done by awaiting handles above.
    // tracing::info!("Waiting for all WebSocket monitoring threads to complete...");
    // for handle in ws_handles { ... } // This loop is now for collecting results
    tracing::info!("All WebSocket monitoring tasks have been awaited.");

    // Placeholder for further metrics processing
    // let results = BenchmarkResults { ... };
    // results.print_summary();

    Ok(())
}
