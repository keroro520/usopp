# Ussop: Solana RPC Node Performance Benchmarking Tool

A Rust-based tool for benchmarking Solana RPC node performance by sending concurrent transactions and measuring various performance metrics.

## Features

- Concurrent transaction sending to multiple RPC nodes
- WebSocket-based transaction confirmation monitoring
- Detailed performance metrics collection
- JSON output for easy analysis
- Support for both CLI and config file input

## Installation

```bash
git clone https://github.com/yourusername/usopp.git
cd usopp
cargo build --release
```

## Usage

### Command Line Arguments

```bash
usopp --keypair <KEYPAIR_PATH> --recipient <RECIPIENT_PUBKEY> --amount <LAMPORTS> --num-txs <NUM_TRANSACTIONS> [--config <CONFIG_PATH>]
```

### Configuration File

You can also provide a configuration file in JSON format:

```json
{
  "keypair_path": "/path/to/keypair.json",
  "recipient": "RecipientPubkeyHere",
  "amount_lamports": 1000000,
  "num_transactions": 10,
  "rpc_nodes": [
    {
      "http_url": "https://api.mainnet-beta.solana.com",
      "ws_url": "wss://api.mainnet-beta.solana.com"
    }
  ]
}
```

## Output Format

The tool outputs a JSON file containing detailed performance metrics for each transaction and RPC node:

```json
{
  "node_metrics": [
    {
      "node_url": "https://api.mainnet-beta.solana.com",
      "signature": "TransactionSignatureHere",
      "build_time": "100ms",
      "send_time": "200ms",
      "confirm_time": "1500ms",
      "status": "Success"
    }
  ],
  "total_transactions": 10,
  "successful_transactions": 10,
  "failed_transactions": 0,
  "average_build_time": "100ms",
  "average_send_time": "200ms",
  "average_confirm_time": "1500ms"
}
```

## Development

### Prerequisites

- Rust 1.70 or later
- Solana CLI tools (for keypair generation)

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```

### Running Clippy

```bash
cargo clippy
```

## License

MIT License

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request 