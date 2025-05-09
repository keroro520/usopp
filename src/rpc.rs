use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, transaction::Transaction};
use std::sync::{mpsc, Arc};
use std::thread;

pub struct RpcClientManager {
    clients: Vec<Arc<RpcClient>>,
}

impl RpcClientManager {
    pub fn new(rpc_urls: Vec<String>) -> Self {
        let clients = rpc_urls
            .into_iter()
            // TODO: @kero what use of commitment config?
            .map(|url| {
                Arc::new(RpcClient::new_with_commitment(
                    url,
                    CommitmentConfig::confirmed(),
                ))
            })
            .collect();

        Self { clients }
    }

    // NOTE: In order to send transactions to all clients in parallel, we create threads for each client,
    //       and each thread will receive a Vec<Transaction> from the main thread and send them to the
    //       client in parallel.
    pub fn send_transactions(&self, transactions: &[Transaction]) {
        if transactions.is_empty() {
            return;
        }

        let mut thread_handles = Vec::with_capacity(self.clients.len());
        let mut senders = Vec::with_capacity(self.clients.len());

        for client_arc in &self.clients {
            let (tx, rx) = mpsc::channel::<Vec<Transaction>>();
            senders.push(tx);

            let current_client_arc = Arc::clone(client_arc);
            let handle = thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap_or_else(|e| {
                        panic!("Failed to create Tokio runtime for worker thread: {:?}", e)
                    });

                let transactions = rx.recv().unwrap();
                for transaction in transactions {
                    rt.block_on(async {
                        current_client_arc
                            .send_transaction(&transaction)
                            .await
                            .unwrap_or_else(|e| {
                                panic!("Failed to send transaction via RPC client: {:?}", e)
                            });
                    });
                }
            });
            thread_handles.push(handle);
        }

        for sender in &senders {
            sender.send(transactions.to_owned()).unwrap();
        }

        drop(senders);

        for handle in thread_handles {
            handle.join().unwrap_or_else(|panic_payload| {
                std::panic::resume_unwind(panic_payload);
            });
        }
    }
}
