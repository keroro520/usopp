mod config;
mod metrics;
mod rpc;
mod transaction;
mod websocket;

use anyhow::Result;
use clap::Parser;
use config::{BenchmarkConfig, CliArgs, RpcNode};
use metrics::{BenchmarkResults, NodeMetrics};
use rpc::RpcClientManager;
use solana_sdk::pubkey;
use solana_sdk::signature::{read_keypair_file, Signature};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use websocket::WebSocketHandle;

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

    // Create an mpsc channel for WebSocket results
    // Channel sends (Signature, Confirmation SystemTime, Slot)
    let (ws_result_tx, mut ws_result_rx) = mpsc::channel::<(Signature, SystemTime, u64)>(
        config.num_transactions * config.rpc_nodes.len(),
    );

    // Spawn WebSocket monitoring threads
    let mut ws_handles: Vec<JoinHandle<Result<()>>> = Vec::new();
    tracing::info!(
        "Spawning WebSocket monitoring threads for {} RPC nodes and {} signatures...",
        config.rpc_nodes.len(),
        transaction_signatures.len()
    );

    for rpc_node_config in &config.rpc_nodes {
        let node_ws_url = rpc_node_config.ws_url.clone();
        let signatures_clone = transaction_signatures.clone();
        let ws_result_tx_clone = ws_result_tx.clone();

        let handle = tokio::spawn(async move {
            tracing::info!("Connecting WebSocket to {}...", node_ws_url);
            let ws_handle =
                WebSocketHandle::new(node_ws_url.clone(), signatures_clone, ws_result_tx_clone);
            if let Err(e) = ws_handle.monitor_confirmation().await {
                tracing::error!(
                    "WebSocket monitoring failed for {}: {}. Thread finishing.",
                    node_ws_url,
                    e
                );
                return Err(e); // Propagate error out of the spawned task
            }
            Ok(())
        });
        ws_handles.push(handle);
    }
    // Drop the original sender to ensure the channel closes when all clones are dropped
    drop(ws_result_tx);

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

    // Collect results from WebSocket threads
    let mut confirmed_transactions: HashMap<Signature, (SystemTime, u64)> = HashMap::new();
    let total_expected_confirmations = transaction_signatures.len();

    tracing::info!("Waiting for transaction confirmations via WebSockets...");
    let overall_timeout =
        Duration::from_secs(config.transaction_timeout_seconds.unwrap_or(120) as u64);

    loop {
        if confirmed_transactions.len() >= total_expected_confirmations
            || benchmark_start_time.elapsed() > overall_timeout
        {
            if benchmark_start_time.elapsed() > overall_timeout
                && confirmed_transactions.len() < total_expected_confirmations
            {
                tracing::warn!(
                    "Overall timeout reached while waiting for confirmations. Received {}/{}",
                    confirmed_transactions.len(),
                    total_expected_confirmations
                );
            }
            break;
        }

        match tokio::time::timeout(Duration::from_secs(1), ws_result_rx.recv()).await {
            Ok(Some((signature, timestamp, slot))) => {
                if !confirmed_transactions.contains_key(&signature) {
                    let duration_since_start = timestamp
                        .duration_since(benchmark_start_system_time)
                        .unwrap_or_else(|_| Duration::from_secs(0));
                    tracing::info!(
                        "CONFIRMED: Signature {} at {:?} (took {:?}), slot {}. ({}/{})",
                        signature,
                        timestamp,
                        duration_since_start,
                        slot,
                        confirmed_transactions.len() + 1,
                        total_expected_confirmations
                    );
                    confirmed_transactions.insert(signature, (timestamp, slot));
                } else {
                    let duration_since_start = timestamp
                        .duration_since(benchmark_start_system_time)
                        .unwrap_or_else(|_| Duration::from_secs(0));
                    tracing::debug!(
                        "DUPLICATE CONF: Signature {} already confirmed. New confirmation at {:?} (took {:?}), slot {}.",
                        signature, timestamp, duration_since_start, slot
                    );
                }
            }
            Ok(None) => {
                tracing::info!("WebSocket result channel closed.");
                break; // Channel closed, no more results will arrive
            }
            Err(_) => {}
        }
    }

    tracing::info!(
        "Finished collecting WebSocket results. {} unique transactions confirmed.",
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
    if confirmed_transactions.len() < total_expected_confirmations {
        tracing::warn!(
            "{} transactions were not confirmed via WebSocket within the timeout.",
            total_expected_confirmations - confirmed_transactions.len()
        );
    }

    // Wait for all WebSocket threads to finish
    tracing::info!("Waiting for all WebSocket monitoring threads to complete...");
    for handle in ws_handles {
        match handle.await {
            Ok(Ok(_)) => { /* Thread completed successfully */ }
            Ok(Err(e)) => tracing::error!(
                "A WebSocket monitoring thread panicked or returned an error: {}",
                e
            ),
            Err(e) => tracing::error!("A WebSocket monitoring thread failed to join: {}", e),
        }
    }
    tracing::info!("All WebSocket monitoring threads completed.");

    // Placeholder for further metrics processing
    // let results = BenchmarkResults { ... };
    // results.print_summary();

    Ok(())
}
