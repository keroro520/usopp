use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcNode {
    pub http_url: String,
    pub ws_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub keypair_path: PathBuf,
    pub recipient: Pubkey,
    pub amount_lamports: u64,
    pub num_transactions: usize,
    pub rpc_nodes: Vec<RpcNode>,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct CliArgs {
    /// Path to config file (required)
    #[arg(short, long)]
    pub config: PathBuf,
}

impl BenchmarkConfig {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&contents)?)
    }
}
