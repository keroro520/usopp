use serde_json;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::signer::Signer;
use std::fs::File;
use std::io::Write;

fn main() {
    let keypair = Keypair::new();
    let bytes = keypair.to_bytes();
    let json = serde_json::to_string(&bytes.to_vec()).unwrap();
    let mut file = File::create("test-keypair.json").unwrap();
    file.write_all(json.as_bytes()).unwrap();
    println!("Generated keypair with public key: {}", keypair.pubkey());
}
