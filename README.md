# Usopp: Solana RPC Node Performance Benchmarking Tool

A Rust-based tool for benchmarking Solana RPC node performance by sending concurrent transactions and measuring various performance metrics.

## Features

- Concurrent transaction sending to multiple RPC nodes
- WebSocket-based transaction confirmation monitoring
- Detailed performance metrics collection

## Usage

```bash
cargo run --release --bin usopp -- -c config.json
```

### Configuration File

You can also provide a configuration file in JSON format:

```json
{
  "keypair_path": "/path/to/keypair.json",
  "recipient": "SJDjJMwSTPt3Bs3GKBGfESLqUcKRV8M1dnbhkLatu41",
  "amount_lamports": 1000000,
  "num_transactions": 10,
  "rpc_nodes": [
    {
      "name": "quicknode",
      "http_url": "https://api.mainnet-beta.solana.com",
      "ws_url": "wss://api.mainnet-beta.solana.com"
    }
  ]
}
```

## Output Format

```markdown
## Signature Confirmation Report

| Signature | quicknode Score | quicknode2 Score |
|---|---|---|
| 4LSk3GovW8vJW7RiZDdWWthmPGcr855ZXDGmWPqkciwtFSrzQVAEV8CpgvmR15JmNeKABt7gmsdxhwgm7megoXgx | 2 | 1 |

## Node Performance Summary (Lower Sum Score is Better)

| Order | Node Name | Sum Score |
|---|---|---|
| 1 | quicknode2 | 1 |
| 2 | quicknode | 2 |
```
