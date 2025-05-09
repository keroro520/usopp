use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use solana_sdk::signature::Signature;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

// Sent to the server to subscribe
#[derive(Debug, Serialize, Deserialize)]
struct SignatureSubscription {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<serde_json::Value>,
}

// Received from server as acknowledgement of subscription
#[derive(Debug, Deserialize)]
struct SubscriptionAcknowledgement {
    id: u64,     // Matches the id in SignatureSubscription request
    result: u64, // This is the subscription ID
}

// Structures for the actual signature notification message
#[derive(Debug, Serialize, Deserialize)]
struct NotificationContext {
    slot: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct NotificationValue {
    err: Option<serde_json::Value>, // Error object or null
}

#[derive(Debug, Serialize, Deserialize)]
struct NotificationResultData {
    context: NotificationContext,
    value: NotificationValue,
}

#[derive(Debug, Serialize, Deserialize)]
struct SignatureNotificationParams {
    result: NotificationResultData, // Updated field type
    subscription: u64,              // ID for the subscription
}

#[derive(Debug, Serialize, Deserialize)]
struct SignatureNotification {
    jsonrpc: String,
    method: String,
    params: SignatureNotificationParams,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignatureResult {
    pub err: Option<String>, // This struct remains for potential external use, but isn't directly deserialized from WS message anymore
    pub slot: u64,
    #[serde(rename = "confirmationStatus")]
    pub confirmation_status: String,
}

pub struct WebSocketHandle {
    ws_url: String,
    signature: Signature,
    tx: mpsc::Sender<(Signature, Duration)>,
}

impl WebSocketHandle {
    pub fn new(
        ws_url: String,
        signature: Signature,
        tx: mpsc::Sender<(Signature, Duration)>,
    ) -> Self {
        Self {
            ws_url,
            signature,
            tx,
        }
    }

    pub async fn monitor_confirmation(&self) -> Result<()> {
        let monitoring_start_time = Instant::now();
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
                    tracing::debug!("Received WebSocket message: {}", text);

                    // Attempt to parse as a generic JSON value to inspect its structure
                    let v: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(val) => val,
                        Err(e) => {
                            tracing::warn!(
                                "Failed to parse message to JSON: {}. Raw message: {}",
                                e,
                                text
                            );
                            continue;
                        }
                    };

                    // Check if it's a subscription acknowledgement
                    // Acknowledgement has "id" and "result", but no "method"
                    if v.get("id").is_some()
                        && v.get("result").is_some()
                        && v.get("method").is_none()
                    {
                        match serde_json::from_value::<SubscriptionAcknowledgement>(v) {
                            Ok(ack) => {
                                tracing::info!(
                                    "Subscription acknowledged for request id {}. WebSocket Subscription ID: {}. Signature: {}",
                                    ack.id,
                                    ack.result,
                                    self.signature
                                );
                                // Continue waiting for the actual notification
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to deserialize as SubscriptionAcknowledgement: {}. Raw message: {}",
                                    e,
                                    text
                                );
                            }
                        }
                    }
                    // Check if it's a signature notification
                    // Notification has "method": "signatureNotification"
                    else if v
                        .get("method")
                        .is_some_and(|m| m == "signatureNotification")
                    {
                        match serde_json::from_value::<SignatureNotification>(v) {
                            Ok(notification) => {
                                tracing::debug!(
                                    "Deserialized as SignatureNotification: {:?}",
                                    notification
                                );
                                // Check if this notification is for the correct subscription if needed,
                                // though for a single monitored signature, it should be.
                                // Example: if notification.params.subscription == stored_subscription_id

                                let result_data = notification.params.result;
                                let no_error = result_data
                                    .value
                                    .err
                                    .as_ref()
                                    .is_none_or(|e_val| e_val.is_null());

                                // When subscribing with "finalized" commitment, the notification itself means it's finalized.
                                // We just need to check for an error.
                                if no_error {
                                    let confirm_time = monitoring_start_time.elapsed();
                                    tracing::info!(
                                        "Signature {} confirmed (finalized) at slot {}. Time: {:?}",
                                        self.signature,
                                        result_data.context.slot,
                                        confirm_time
                                    );
                                    if let Err(e) =
                                        self.tx.send((self.signature, confirm_time)).await
                                    {
                                        tracing::error!(
                                            "Failed to send confirmation to channel: {}",
                                            e
                                        );
                                    }
                                    break; // Transaction confirmed
                                } else {
                                    tracing::error!(
                                        "Signature {} finalized with error: {:?}. Slot: {}. Raw notification: {}",
                                        self.signature,
                                        result_data.value.err,
                                        result_data.context.slot,
                                        text
                                    );
                                    // Decide if to break or continue based on requirements for error handling.
                                    // For now, let's break as it's an error state for this signature.
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to deserialize as SignatureNotification: {}. Raw message: {}",
                                    e,
                                    text
                                );
                            }
                        }
                    } else {
                        tracing::warn!("Received unhandled WebSocket message structure: {}", text);
                    }
                }
                Message::Close(close_frame) => {
                    tracing::info!("WebSocket connection closed by server: {:?}", close_frame);
                    break; // Exit loop on close
                }
                _ => {
                    tracing::debug!("Received non-text WebSocket message");
                    continue;
                }
            }
        }

        Ok(())
    }
}
