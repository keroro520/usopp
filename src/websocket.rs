use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use solana_sdk::signature::Signature;
use std::collections::{HashMap, HashSet};
use std::time::{Instant, SystemTime};
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

pub struct WebSocketHandle {
    ws_url: String,
    signatures: Vec<Signature>,
    tx: mpsc::Sender<(Signature, SystemTime, u64)>,
}

impl WebSocketHandle {
    pub fn new(
        ws_url: String,
        signatures: Vec<Signature>,
        tx: mpsc::Sender<(Signature, SystemTime, u64)>,
    ) -> Self {
        Self {
            ws_url,
            signatures,
            tx,
        }
    }

    pub async fn monitor_confirmation(&self) -> Result<()> {
        let (mut ws_stream, _) = connect_async(&self.ws_url).await?;

        let mut request_id_counter: u64 = 1;
        // Maps our request_id to the signature we sent the subscription for
        let mut pending_acknowledgements: HashMap<u64, Signature> = HashMap::new();
        // Maps the server's subscription_id to the signature
        let mut active_subscriptions: HashMap<u64, Signature> = HashMap::new();
        // Keep track of signatures we are still waiting for notifications for
        let mut pending_notifications: HashSet<Signature> =
            self.signatures.iter().cloned().collect();

        for signature_to_subscribe in &self.signatures {
            let current_request_id = request_id_counter;
            request_id_counter += 1;

            let subscription_payload = SignatureSubscription {
                jsonrpc: "2.0".to_string(),
                id: current_request_id, // Use unique id for each subscription request
                method: "signatureSubscribe".to_string(),
                params: vec![
                    serde_json::to_value(signature_to_subscribe.to_string())?,
                    serde_json::json!({
                        // NOTE: "processed" commitment because we aim to compare the performance of different RPC nodes
                        "commitment": "processed",
                    }),
                ],
            };

            let payload_str = serde_json::to_string(&subscription_payload)
                .expect("Failed to serialize subscription payload");
            ws_stream
                .send(Message::Text(payload_str))
                .await
                .expect("Failed to send subscription request");
            pending_acknowledgements.insert(current_request_id, *signature_to_subscribe);
        }

        tracing::info!(
            "All subscription requests sent for {} signatures to {}. Waiting for acknowledgements and notifications.",
            pending_acknowledgements.len(),
            self.ws_url
        );

        while !pending_notifications.is_empty() {
            match ws_stream.next().await {
                Some(Ok(msg)) => match msg {
                    Message::Text(text) => {
                        tracing::debug!("Received WebSocket message on {}: {}", self.ws_url, text);

                        let v: serde_json::Value =
                            match serde_json::from_str(&text) {
                                Ok(val) => val,
                                Err(e) => {
                                    tracing::warn!(
                                    "Failed to parse message to JSON on {}: {}. Raw message: {}",
                                    self.ws_url, e, text
                                );
                                    continue;
                                }
                            };

                        // Check if it's a subscription acknowledgement
                        if v.get("id").is_some()
                            && v.get("result").is_some()
                            && v.get("method").is_none()
                        {
                            match serde_json::from_value::<SubscriptionAcknowledgement>(v.clone()) {
                                Ok(ack) => {
                                    if let Some(signature) =
                                        pending_acknowledgements.remove(&ack.id)
                                    {
                                        tracing::info!(
                                            "Subscription acknowledged for signature {} (Request ID: {}). WebSocket Subscription ID: {}. URL: {}",
                                            signature, ack.id, ack.result, self.ws_url
                                        );
                                        active_subscriptions.insert(ack.result, signature);
                                    } else {
                                        tracing::warn!(
                                            "Received acknowledgement for unknown request ID: {}. URL: {}. Raw: {}",
                                            ack.id, self.ws_url, text
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to deserialize SubscriptionAcknowledgement on {}: {}. Raw: {}",
                                        self.ws_url, e, text
                                    );
                                }
                            }
                        }
                        // Check if it's a signature notification
                        else if v
                            .get("method")
                            .is_some_and(|m| m == "signatureNotification")
                        {
                            match serde_json::from_value::<SignatureNotification>(v) {
                                Ok(notification) => {
                                    if let Some(signature) =
                                        active_subscriptions.get(&notification.params.subscription)
                                    {
                                        let result_data = notification.params.result;
                                        let no_error = result_data
                                            .value
                                            .err
                                            .as_ref()
                                            .is_none_or(|e_val| e_val.is_null());
                                        let slot = result_data.context.slot;
                                        let confirmation_timestamp = SystemTime::now();

                                        if no_error {
                                            tracing::info!(
                                                "Signature {} confirmed (finalized) at slot {} on {}. Timestamp: {:?}. WebSocket Sub ID: {}",
                                                signature, slot, self.ws_url, confirmation_timestamp, notification.params.subscription
                                            );
                                            if let Err(e) = self
                                                .tx
                                                .send((*signature, confirmation_timestamp, slot))
                                                .await
                                            {
                                                tracing::error!(
                                                    "Failed to send confirmation for {} to channel: {}",
                                                    signature, e
                                                );
                                            }
                                        } else {
                                            tracing::error!(
                                                "Signature {} finalized with error on {}: {:?}. Slot: {}. Timestamp: {:?}. WebSocket Sub ID: {}. Raw: {}",
                                                signature, self.ws_url, result_data.value.err, slot, confirmation_timestamp, notification.params.subscription, text
                                            );
                                            if let Err(e) = self
                                                .tx
                                                .send((*signature, confirmation_timestamp, slot))
                                                .await
                                            {
                                                tracing::error!(
                                                    "Failed to send error status for {} to channel: {}",
                                                    signature, e
                                                );
                                            }
                                        }
                                        // Remove from pending_notifications regardless of error, as we've received its terminal state.
                                        pending_notifications.remove(signature);
                                        // Optionally, remove from active_subscriptions if no more messages are expected for it.
                                        // active_subscriptions.remove(&notification.params.subscription);
                                    } else {
                                        tracing::warn!(
                                            "Received notification for unknown/inactive subscription ID: {}. URL: {}. Raw: {}",
                                            notification.params.subscription, self.ws_url, text
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to deserialize SignatureNotification on {}: {}. Raw: {}",
                                        self.ws_url, e, text
                                    );
                                }
                            }
                        } else {
                            tracing::warn!(
                                "Received unhandled WebSocket message structure on {}: {}",
                                self.ws_url,
                                text
                            );
                        }
                    }
                    Message::Close(close_frame) => {
                        tracing::info!(
                            "WebSocket connection to {} closed by server: {:?}",
                            self.ws_url,
                            close_frame
                        );
                        break; // Exit loop on close
                    }
                    _ => {
                        tracing::debug!("Received non-text WebSocket message on {}", self.ws_url);
                    }
                },
                Some(Err(e)) => {
                    tracing::error!(
                        "Error reading from WebSocket stream {}: {}. Remaining signatures: {}",
                        self.ws_url,
                        e,
                        pending_notifications.len()
                    );
                    break; // Connection error, stop monitoring this WebSocket
                }
                None => {
                    tracing::info!(
                        "WebSocket stream {} ended. Remaining signatures: {}",
                        self.ws_url,
                        pending_notifications.len()
                    );
                    break; // Stream ended
                }
            }
        }

        if !pending_notifications.is_empty() {
            tracing::warn!(
                "WebSocket {} finished monitoring with {} pending signatures: {:?}",
                self.ws_url,
                pending_notifications.len(),
                pending_notifications
            );
        } else {
            tracing::info!(
                "WebSocket {} finished monitoring all signatures.",
                self.ws_url
            );
        }

        Ok(())
    }
}
