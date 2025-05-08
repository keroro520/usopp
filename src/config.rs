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
    /// Path to the keypair file
    #[arg(short, long)]
    pub keypair: PathBuf,

    /// Recipient's public key
    #[arg(short, long)]
    pub recipient: String,

    /// Amount to transfer in lamports
    #[arg(short, long)]
    pub amount: u64,

    /// Number of transactions to send
    #[arg(short, long, default_value_t = 1)]
    pub num_txs: usize,

    /// Path to config file (optional)
    #[arg(short, long)]
    pub config: Option<PathBuf>,
}

impl BenchmarkConfig {
    pub fn from_cli(args: &CliArgs) -> Result<Self> {
        // Parse recipient pubkey
        let recipient = Pubkey::try_from(args.recipient.as_str())?;

        // Use devnet RPC nodes
        let rpc_nodes = vec![
            RpcNode {
                http_url: "https://api.devnet.solana.com".to_string(),
                ws_url: "wss://api.devnet.solana.com".to_string(),
            },
            RpcNode {
                http_url: "https://devnet.genesysgo.net".to_string(),
                ws_url: "wss://devnet.genesysgo.net".to_string(),
            },
        ];

        Ok(Self {
            keypair_path: args.keypair.clone(),
            recipient,
            amount_lamports: args.amount,
            num_transactions: args.num_txs,
            rpc_nodes,
        })
    }

    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&contents)?)
    }
}
