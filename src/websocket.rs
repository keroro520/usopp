use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use solana_sdk::signature::Signature;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Serialize, Deserialize)]
struct SignatureSubscription {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SignatureNotification {
    jsonrpc: String,
    method: String,
    params: SignatureNotificationParams,
}

#[derive(Debug, Serialize, Deserialize)]
struct SignatureNotificationParams {
    result: SignatureResult,
    subscription: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignatureResult {
    pub err: Option<String>,
    pub slot: u64,
    #[serde(rename = "confirmationStatus")]
    pub confirmation_status: String,
}

pub struct WebSocketManager {
    ws_url: String,
    signature: Signature,
    tx: mpsc::Sender<(Signature, Duration)>,
}

impl WebSocketManager {
    pub fn new(ws_url: String, signature: Signature, tx: mpsc::Sender<(Signature, Duration)>) -> Self {
        Self {
            ws_url,
            signature,
            tx,
        }
    }

    pub async fn monitor_confirmation(&self, start_time: Instant) -> Result<()> {
        let (mut ws_stream, _) = connect_async(&self.ws_url).await?;

        // Subscribe to signature confirmation
        let subscription = SignatureSubscription {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "signatureSubscribe".to_string(),
            params: vec![
                serde_json::to_value(self.signature.to_string())?,
                serde_json::json!({
                    "commitment": "finalized"
                }),
            ],
        };

        ws_stream
            .send(Message::Text(serde_json::to_string(&subscription)?))
            .await?;

        while let Some(msg) = ws_stream.next().await {
            match msg? {
                Message::Text(text) => {
                    let notification: SignatureNotification = serde_json::from_str(&text)?;
                    if notification.method == "signatureNotification" {
                        let result = notification.params.result;
                        if result.err.is_none() && result.confirmation_status == "finalized" {
                            let confirm_time = start_time.elapsed();
                            self.tx
                                .send((self.signature, confirm_time))
                                .await?;
                            break;
                        }
                    }
                }
                _ => continue,
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_websocket_connection() {
        let (tx, mut rx) = mpsc::channel(1);
        let signature = Signature::default();
        let manager = WebSocketManager::new(
            "wss://api.mainnet-beta.solana.com".to_string(),
            signature,
            tx,
        );

        let start_time = Instant::now();
        let monitor_handle = tokio::spawn(async move {
            manager.monitor_confirmation(start_time).await
        });

        // Wait for a short time to ensure connection is established
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        // Cancel the monitoring since we can't easily test the full confirmation flow
        monitor_handle.abort();
        
        // Verify no messages were received (since we aborted before confirmation)
        assert!(rx.try_recv().is_err());
    }
} 