use async_trait::async_trait;
use binary_options_tools_core_pre::error::CoreError;
use binary_options_tools_core_pre::reimports::bounded_async;
use binary_options_tools_core_pre::traits::ReconnectCallback;
use binary_options_tools_core_pre::{
    error::CoreResult,
    reimports::{AsyncReceiver, AsyncSender, Message},
    traits::{ApiModule, Rule},
};
use core::fmt;
use futures_util::stream::unfold;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::{select, sync::oneshot};

use tracing::{debug, warn};
use uuid::Uuid;

use crate::pocketoption::candle::{BaseCandle, SubscriptionType};
use crate::pocketoption::error::PocketError;
use crate::pocketoption::types::{MultiPatternRule, StreamData as RawCandle, SubscriptionEvent};
use crate::pocketoption::{
    candle::Candle, // Assuming this exists in your types
    error::PocketResult,
    state::State,
};

#[derive(Serialize)]
pub struct ChangeSymbol {
    // Making it public as it may be used somewhere else
    pub asset: String,
    pub period: i64,
}

#[derive(Deserialize)]
pub struct History {
    pub asset: String,
    pub period: u32,
    pub candles: Vec<BaseCandle>,
    pub history: Vec<Vec<f64>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ServerResponse {
    History(History),
    Candle(RawCandle),
}

impl fmt::Display for ChangeSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "42[\"changeSymbol\",{}]",
            serde_json::to_string(&self).map_err(|_| fmt::Error)?
        )
    }
}

/// Maximum number of concurrent subscriptions allowed
const MAX_SUBSCRIPTIONS: usize = 4;
const MAX_CHANNEL_CAPACITY: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum SubscriptionError {
    #[error("Maximum subscriptions limit reached")]
    MaxSubscriptionsReached,
    #[error("Subscription already exists")]
    SubscriptionAlreadyExists,
}

/// Command enum for the `SubscriptionsApiModule`.
#[derive(Debug)]
pub enum Command {
    /// Subscribe to an asset's stream
    Subscribe {
        asset: String,
        responder: oneshot::Sender<PocketResult<AsyncReceiver<SubscriptionEvent>>>,
    },
    /// Unsubscribe from an asset's stream
    Unsubscribe {
        asset: String,
        responder: oneshot::Sender<PocketResult<()>>,
    },
    /// History
    History {
        asset: String,
        period: u32,
        responder: oneshot::Sender<PocketResult<Vec<Candle>>>,
    },
    /// Requests the number of active subscriptions
    SubscriptionCount(oneshot::Sender<u32>),
}

/// Response enum for subscription commands
#[derive(Debug)]
pub enum CommandResponse {
    /// Successful subscription with stream receiver
    SubscriptionSuccess {
        command_id: Uuid,
        stream_receiver: AsyncReceiver<SubscriptionEvent>,
    },
    /// Subscription failed
    SubscriptionFailed {
        command_id: Uuid,
        error: Box<PocketError>,
    },
    /// History Response
    History { command_id: Uuid, data: Vec<Candle> },
    /// Unsubscription successful
    UnsubscriptionSuccess { command_id: Uuid },
    /// Unsubscription failed
    UnsubscriptionFailed {
        command_id: Uuid,
        error: Box<PocketError>,
    },
    /// Returns the number of active subscriptions
    SubscriptionCount(u32),
    /// History failed
    HistoryFailed {
        command_id: Uuid,
        error: Box<PocketError>,
    },
}

/// Represents the data sent through the subscription stream.
pub struct SubscriptionStream {
    receiver: AsyncReceiver<SubscriptionEvent>,
    sender: Option<AsyncSender<Command>>,
    command_receiver: AsyncReceiver<CommandResponse>,
    asset: String,
    sub_type: SubscriptionType,
}

/// Callback for when there is a disconnection
struct SubscriptionCallback;

#[async_trait]
impl ReconnectCallback<State> for SubscriptionCallback {
    async fn call(&self, state: Arc<State>, ws_sender: &AsyncSender<Message>) -> CoreResult<()> {
        tokio::time::sleep(Duration::from_secs(2)).await; // FIXME: This is a temporary delay, it may need to be fine tuned
        // Resubscribe to all active subscriptions
        for symbol in state.active_subscriptions.read().await.keys() {
            tokio::time::sleep(Duration::from_secs(1)).await;
            // Resubscribe to each active subscription
            send_subscribe_message(ws_sender, symbol, 1).await?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct SubscriptionsHandle {
    sender: AsyncSender<Command>,
    receiver: AsyncReceiver<CommandResponse>,
}

impl SubscriptionsHandle {
    /// Subscribe to an asset's real-time data stream.
    ///
    /// # Arguments
    /// * `asset` - The asset symbol to subscribe to
    ///
    /// # Returns
    /// * `PocketResult<SubscriptionStream>` - Subscription stream
    ///
    /// # Errors
    /// * Returns error if maximum subscriptions reached
    /// * Returns error if subscription fails
    pub async fn subscribe(
        &self,
        asset: String,
        sub_type: SubscriptionType,
    ) -> PocketResult<SubscriptionStream> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Command::Subscribe {
                asset: asset.clone(),
                responder: tx,
            })
            .await
            .map_err(CoreError::from)?;

        let stream_receiver = rx
            .await
            .map_err(|_| CoreError::Other("SubscriptionsApiModule responder dropped".into()))??;

        Ok(SubscriptionStream {
            receiver: stream_receiver,
            sender: Some(self.sender.clone()),
            command_receiver: self.receiver.clone(),
            asset,
            sub_type,
        })
    }

    /// Unsubscribe from an asset's stream.
    ///
    /// # Arguments
    /// * `asset` - The symbol of the asset to unsubscribe from
    ///
    /// # Returns
    /// * `PocketResult<()>` - Success or error
    pub async fn unsubscribe(&self, asset: String) -> PocketResult<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Command::Unsubscribe {
                asset,
                responder: tx,
            })
            .await
            .map_err(CoreError::from)?;

        rx.await
            .map_err(|_| CoreError::Other("SubscriptionsApiModule responder dropped".into()))?
    }

    /// Get the number of active subscriptions.
    ///
    /// # Returns
    /// * `PocketResult<u32>` - Number of active subscriptions
    pub async fn get_active_subscriptions_count(&self) -> PocketResult<u32> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Command::SubscriptionCount(tx))
            .await
            .map_err(CoreError::from)?;

        rx.await
            .map_err(|_| CoreError::Other("SubscriptionsApiModule responder dropped".into()).into())
    }

    /// Check if maximum subscriptions limit is reached.
    ///
    /// # Returns
    /// * `PocketResult<bool>` - True if limit reached
    pub async fn is_max_subscriptions_reached(&self) -> PocketResult<bool> {
        self.get_active_subscriptions_count()
            .await
            .map(|count| count as usize == MAX_SUBSCRIPTIONS)
    }

    /// Gets the history for an asset with its period
    ///
    /// **Constraint:**
    /// Only one outstanding history call per `(asset, period)` is supported.
    /// Duplicate requests will be rejected with `HistoryFailed`.
    ///
    /// # Arguments
    /// * `asset` - The asset symbol
    /// * `period` - The period in minutes
    /// # Returns
    /// * `PocketResult<Vec<Candle>>` - Vector of candles
    pub async fn history(&self, asset: String, period: u32) -> PocketResult<Vec<Candle>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Command::History {
                asset,
                period,
                responder: tx,
            })
            .await
            .map_err(CoreError::from)?;

        rx.await
            .map_err(|_| CoreError::Other("SubscriptionsApiModule responder dropped".into()).into())?
    }
}

/// The API module for handling subscription operations.
pub struct SubscriptionsApiModule {
    state: Arc<State>,
    command_receiver: AsyncReceiver<Command>,
    _command_responder: AsyncSender<CommandResponse>,
    message_receiver: AsyncReceiver<Arc<Message>>,
    to_ws_sender: AsyncSender<Message>,
    history_responders: HashMap<(String, u32), oneshot::Sender<PocketResult<Vec<Candle>>>>,
}

#[async_trait]
impl ApiModule<State> for SubscriptionsApiModule {
    type Command = Command;
    type CommandResponse = CommandResponse;
    type Handle = SubscriptionsHandle;

    fn new(
        state: Arc<State>,
        command_receiver: AsyncReceiver<Self::Command>,
        command_responder: AsyncSender<Self::CommandResponse>,
        message_receiver: AsyncReceiver<Arc<Message>>,
        to_ws_sender: AsyncSender<Message>,
    ) -> Self {
        Self {
            state,
            command_receiver,
            _command_responder: command_responder,
            message_receiver,
            to_ws_sender,
            history_responders: HashMap::new(),
        }
    }

    fn create_handle(
        sender: AsyncSender<Self::Command>,
        receiver: AsyncReceiver<Self::CommandResponse>,
    ) -> Self::Handle {
        SubscriptionsHandle { sender, receiver }
    }

    async fn run(&mut self) -> CoreResult<()> {
        // TODO: Implement the main run loop
        // This loop should handle:
        // 1. Incoming commands (Subscribe, Unsubscribe, StreamTerminationRequest)
        // 2. Incoming WebSocket messages with asset data
        // 3. Managing subscription limits
        // 4. Forwarding data to appropriate streams
        //
        loop {
            select! {
                Ok(cmd) = self.command_receiver.recv() => {
                    match cmd {
                        Command::Subscribe { asset, responder } => {
                            if self.is_max_subscriptions_reached().await {
                                let _ = responder.send(Err(SubscriptionError::MaxSubscriptionsReached.into()));
                                continue;
                            } else {
                                // Create stream channel
                                if let Err(e) = self.send_subscribe_message(&asset, 1).await {
                                    let _ = responder.send(Err(e.into()));
                                    continue;
                                }
                                let (stream_sender, stream_receiver) = bounded_async(MAX_CHANNEL_CAPACITY);
                                if let Err(e) = self.add_subscription(asset.clone(), stream_sender).await {
                                    let _ = responder.send(Err(PocketError::General(e)));
                                    continue;
                                }

                                // Send success response with stream receiver
                                let _ = responder.send(Ok(stream_receiver));
                            }
                        },
                        Command::Unsubscribe { asset, responder } => {
                            match self.remove_subscription(&asset).await {
                                Ok(b) => {
                                    if b {
                                        let _ = responder.send(Ok(()));
                                    } else {
                                        let _ = responder.send(Err(PocketError::General("Subscription not found".to_string())));
                                    }
                                },
                                Err(e) => {
                                    let _ = responder.send(Err(e.into()));
                                }
                            }
                        },
                        Command::SubscriptionCount(responder) => {
                            let count = self.state.active_subscriptions.read().await.len() as u32;
                            let _ = responder.send(count);
                        },
                        Command::History { asset, period, responder } => {
                            // Enforce single request
                            let is_duplicate = self.history_responders.contains_key(&(asset.clone(), period));
                            if is_duplicate {
                                let _ = responder.send(Err(PocketError::General(format!("Duplicate history request for asset: {}, period: {}", asset, period))));
                            } else {
                                if let Err(e) = self.send_subscribe_message(&asset, period).await {
                                    let _ = responder.send(Err(e.into()));
                                } else {
                                    self.history_responders.insert((asset, period), responder);
                                }
                            }
                        }
                    }
                },
                Ok(msg) = self.message_receiver.recv() => {
                    // TODO: Handle incoming WebSocket messages
                    // 1. Parse message for asset data
                    // 2. Find corresponding subscription
                    // 3. Forward data to stream
                    // 4. Handle subscription confirmations/errors
                    match msg.as_ref() {
                        Message::Binary(data) => {
                            // Parse the message for asset data
                            match serde_json::from_slice::<ServerResponse>(data) {
                                Ok(ServerResponse::Candle(data)) => {
                                    // Forward data to stream
                                    if let Err(e) = self.forward_data_to_stream(&data.symbol, data.price, data.timestamp).await {
                                        warn!(target: "SubscriptionsApiModule", "Failed to forward data: {}", e);
                                    }
                                },
                                Ok(ServerResponse::History(data)) => {
                                    if let Some(responder) = self.history_responders.remove(&(data.asset.clone(), data.period)) {
                                        match data.candles.into_iter().map(|c| Candle::try_from((c, data.asset.clone()))).collect::<Result<Vec<_>, _>>() {
                                            Ok(candles) => {
                                                let _ = responder.send(Ok(candles));
                                            }
                                            Err(e) => {
                                                let _ = responder.send(Err(PocketError::General(e.to_string())));
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(target: "SubscriptionsApiModule", "Received data: {:?}",  String::from_utf8(data.to_vec()));
                                    warn!(target: "SubscriptionsApiModule", "Failed to parse message: {}", e);
                                }
                            }
                        },
                        _ => {
                            warn!(target: "SubscriptionsApiModule", "Received unsupported message type");
                            debug!(target: "SubscriptionsApiModule", "Message: {:?}", msg);
                        }
                    }
                }
            }
        }
    }

    fn callback(
        _shared_state: Arc<State>,
        _command_receiver: AsyncReceiver<Self::Command>,
        _command_responder: AsyncSender<Self::CommandResponse>,
        _message_receiver: AsyncReceiver<Arc<Message>>,
        _to_ws_sender: AsyncSender<Message>,
    ) -> CoreResult<Option<Box<dyn ReconnectCallback<State>>>> {
        Ok(Some(Box::new(SubscriptionCallback)))
    }

    fn rule(_: Arc<State>) -> Box<dyn Rule + Send + Sync> {
        // TODO: Implement rule for subscription-related messages
        // This should match messages like:
        // - Asset data updates
        // - Subscription confirmations
        // - Subscription errors
        Box::new(MultiPatternRule::new(vec![
            r#"451-["updateStream",{"#,
            r#"451-["updateHistoryNewFast","#,
        ]))
    }
}

impl SubscriptionsApiModule {
    /// Check if maximum subscriptions limit is reached.
    ///
    /// # Returns
    /// * `bool` - True if limit reached
    async fn is_max_subscriptions_reached(&self) -> bool {
        self.state.active_subscriptions.read().await.len() >= MAX_SUBSCRIPTIONS
    }

    /// Add a new subscription.
    ///
    /// # Arguments
    /// * `subscription_id` - The subscription ID
    /// * `asset` - The asset symbol
    /// * `stream_sender` - The sender for stream data
    ///
    /// # Returns
    /// * `Result<(), String>` - Success or error message
    async fn add_subscription(
        &mut self,
        asset: String,
        stream_sender: AsyncSender<SubscriptionEvent>,
    ) -> PocketResult<()> {
        if self.is_max_subscriptions_reached().await {
            return Err(SubscriptionError::MaxSubscriptionsReached.into());
        }

        // Check if subscription already exists
        if self
            .state
            .active_subscriptions
            .read()
            .await
            .contains_key(&asset)
        {
            return Err(SubscriptionError::SubscriptionAlreadyExists.into());
        }

        // Add to active subscriptions
        self.state
            .active_subscriptions
            .write()
            .await
            .insert(asset, stream_sender);
        Ok(())
    }

    /// Remove a subscription.
    ///
    /// # Arguments
    /// * `asset` - The asset symbol
    ///
    /// # Returns
    /// * `PocketResult<bool>` - True if subscription was removed, false if not found
    async fn remove_subscription(&mut self, asset: &str) -> CoreResult<bool> {
        // TODO: Implement subscription removal
        // 1. Remove from active_subscriptions
        // 2. Remove from asset_to_subscription
        // 3. Return removed subscription info
        if let Some(stream_sender) = self.state.active_subscriptions.write().await.remove(asset) {
            stream_sender.send(SubscriptionEvent::Terminated { reason: "Unsubscribed from main module".to_string() })
                .await.inspect_err(|e| warn!(target: "SubscriptionsApiModule", "Failed to send termination signal: {}", e))?;
            return Ok(true);
        }
        self.resend_connection_messages().await?;
        Ok(false)
    }

    async fn resend_connection_messages(&self) -> CoreResult<()> {
        // Resend connection messages to re-establish subscriptions
        for symbol in self.state.active_subscriptions.read().await.keys() {
            // Send subscription message for each active asset
            self.send_subscribe_message(symbol, 1).await?;
        }
        Ok(())
    }

    /// Send subscription message to WebSocket.
    ///
    /// # Arguments
    /// * `asset` - The asset to subscribe to
    async fn send_subscribe_message(&self, asset: &str, period: u32) -> CoreResult<()> {
        // TODO: Implement WebSocket subscription message
        // Create and send appropriate subscription message format
        send_subscribe_message(&self.to_ws_sender, asset, period).await
    }
    /// Process incoming asset data and forward to appropriate streams.
    ///
    /// # Arguments
    /// * `asset` - The asset symbol
    /// * `candle` - The candle data
    async fn forward_data_to_stream(
        &self,
        asset: &str,
        price: f64,
        timestamp: f64,
    ) -> CoreResult<()> {
        // TODO: Implement data forwarding
        // 1. Find subscription by asset
        // 2. Send StreamData::Candle to stream
        // 3. Handle send errors (stream might be closed)
        if let Some(stream_sender) = self.state.active_subscriptions.read().await.get(asset) {
            stream_sender
                .send(SubscriptionEvent::Update {
                    asset: asset.to_string(),
                    price,
                    timestamp,
                })
                .await
                .map_err(CoreError::from)?;
        }
        // If no subscription found for assets it's not an error, just ignore it
        Ok(())
    }
}

impl SubscriptionStream {
    /// Get the asset symbol for this subscription stream
    pub fn asset(&self) -> &str {
        &self.asset
    }

    /// Unsubscribe from the stream
    pub async fn unsubscribe(mut self) -> PocketResult<()> {
        // Send unsubscribe command through the main handle
        if let Some(sender) = self.sender.take() {
            let (tx, rx) = oneshot::channel();
            sender
                .send(Command::Unsubscribe {
                    asset: self.asset.clone(),
                    responder: tx,
                })
                .await
                .map_err(CoreError::from)?;
            
            rx.await.map_err(|_| CoreError::Other("SubscriptionsApiModule responder dropped".into()))?
        } else {
            Ok(())
        }
    }

    /// Receive the next candle from the stream
    pub async fn receive(&mut self) -> PocketResult<Candle> {
        loop {
            match self.receiver.recv().await {
                Ok(crate::pocketoption::types::SubscriptionEvent::Update {
                    asset,
                    price,
                    timestamp,
                }) => {
                    if asset == self.asset {
                        let candle = self.process_update(timestamp, price)?;
                        if let Some(candle) = candle {
                            return Ok(candle);
                        }
                        // Continue if no candle is ready yet
                    }
                    // Continue if asset doesn't match (shouldn't happen but safety check)
                }
                Ok(crate::pocketoption::types::SubscriptionEvent::Terminated { reason }) => {
                    return Err(PocketError::General(format!("Stream terminated: {reason}")));
                }
                Err(e) => {
                    return Err(CoreError::from(e).into());
                }
            }
        }
    }

    /// Process an incoming price update based on subscription type
    fn process_update(&mut self, timestamp: f64, price: f64) -> PocketResult<Option<Candle>> {
        let asset = self.asset().to_string();
        if let Some(c) = self
            .sub_type
            .update(&BaseCandle::from((timestamp, price)))?
        {
            // Successfully updated candle
            Ok(Some(Candle::try_from((c, asset)).map_err(|e| {
                warn!(target: "SubscriptionStream", "Failed to convert candle: {}", e);
                PocketError::General(format!("Failed to convert candle: {e}"))
            })?))
        } else {
            // No complete candle yet, continue waiting
            Ok(None)
        }
    }

    /// Convert to a futures Stream
    pub fn to_stream(self) -> impl futures_util::Stream<Item = PocketResult<Candle>> + 'static {
        Box::pin(unfold(self, |mut stream| async move {
            let result = stream.receive().await;
            Some((result, stream))
        }))
    }

    // /// Convert to a futures Stream with a static lifetime using Arc
    // pub fn to_stream_static(
    //     self
    // ) -> impl futures_util::Stream<Item = PocketResult<Candle>> + 'static {
    //     Box::pin(unfold(self, |mut stream| async move {
    //         let result = stream.receive().await;
    //         Some((result, stream))
    //     }))
    // }

    /// Check if the subscription type uses time alignment
    pub fn is_time_aligned(&self) -> bool {
        matches!(self.sub_type, SubscriptionType::TimeAligned { .. })
    }

    /// Get the current subscription type
    pub fn subscription_type(&self) -> &SubscriptionType {
        &self.sub_type
    }
}

// Add Clone implementation for SubscriptionStream
impl Clone for SubscriptionStream {
    fn clone(&self) -> Self {
        Self {
            receiver: self.receiver.clone(),
            sender: self.sender.clone(),
            command_receiver: self.command_receiver.clone(),
            asset: self.asset.clone(),
            sub_type: self.sub_type.clone(),
        }
    }
}

async fn send_subscribe_message(
    ws_sender: &AsyncSender<Message>,
    asset: &str,
    period: u32,
) -> CoreResult<()> {
    // TODO: Implement WebSocket subscription message
    // Create and send appropriate subscription message format
    ws_sender
        .send(Message::text(
            ChangeSymbol {
                asset: asset.to_string(),
                period: period as i64,
            }
            .to_string(),
        ))
        .await
        .map_err(CoreError::from)?;
    ws_sender
        .send(Message::text(format!("42[\"unsubfor\",\"{asset}\"]")))
        .await
        .map_err(CoreError::from)?;
    ws_sender
        .send(Message::text(format!("42[\"subfor\",\"{asset}\"]")))
        .await
        .map_err(CoreError::from)?;
    Ok(())
}

impl Drop for SubscriptionStream {
    fn drop(&mut self) {
        // Send Unsubscribe signal when the stream is dropped
        // This will gracefully end the stream and notify any listeners
        debug!(target: "SubscriptionStream", "Dropping subscription stream for asset: {}", self.asset);
        // Send Unsubscribe signal to the main handle
        // This will notify the main module to remove this subscription
        // We don't need to wait for response since we're consuming self
        // and it will be dropped anyway
        if let Some(sender) = &self.sender {
            let (tx, _) = oneshot::channel();
            let _ = sender
                .as_sync()
                .send(Command::Unsubscribe {
                    asset: self.asset.clone(),
                    responder: tx,
                })
                .inspect_err(|e| {
                    warn!(target: "SubscriptionStream", "Failed to send unsubscribe command: {}", e);
                });
        }
    }
}
