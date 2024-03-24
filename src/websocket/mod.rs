use std::borrow::BorrowMut;

use eyre::{eyre, OptionExt};
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde_json::json;
use solana_client::{
    rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter},
    rpc_response::{Response, RpcLogsResponse},
};
use std::marker::PhantomData;
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
use url::Url;

#[allow(dead_code)]
pub struct Uninitialized;
#[allow(dead_code)]
pub struct Initialized;
#[allow(dead_code)]
pub struct Initializing;

pub struct WebSocket<Status = Uninitialized> {
    socket: Option<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>,
    config: WebSocketConfig,
    subscription_string: Option<String>,
    status: PhantomData<Status>,
}

pub struct WebSocketConfig {
    pub num_retries: u8,
    pub url: String,
}

impl WebSocket<Initialized> {
    pub async fn read<T: DeserializeOwned + std::fmt::Debug>(&mut self) -> Result<T, eyre::Error> {
        if self.socket.is_none() {
            return Err(eyre!("Use subscription function before read"));
        }
        loop {
            let read_result = self
                .socket
                .as_mut()
                .unwrap()
                .next()
                .await
                .ok_or_eyre("Failed to read from ws");
            if read_result.is_err() {
                tracing::warn!("connection lost: {}", read_result.err().unwrap());
                let _ = self.socket.as_mut().unwrap().close(None);
                let _ = self.socket.as_mut().unwrap().flush();
                self.reconnect().await?;
                self.config.num_retries -= 1;
                continue;
            }
            let msg = read_result??.to_string();
            let deserialize_result = serde_json::from_str::<T>(&msg);
            if deserialize_result.is_err() {
                tracing::warn!(
                    "Expected other type: found {:?}",
                    deserialize_result.unwrap()
                );
                self.config.num_retries -= 1;
                continue;
            }
            self.config.num_retries = 5;
            return Ok(deserialize_result.unwrap());
        }
    }

    pub async fn reconnect(&mut self) -> Result<(), eyre::Error> {
        let mut socket = attempt_connection(&self.config.url, self.config.num_retries).await?;
        attempt_subscription(
            &self.subscription_string.clone().unwrap(),
            &mut socket,
            self.config.num_retries,
        )
        .await?;
        self.socket.replace(socket);
        Ok(())
    }
}

impl WebSocket<Uninitialized> {
    pub async fn create_new_logs_subscription(
        config: WebSocketConfig,
        subscription_logs_filter: RpcTransactionLogsFilter,
        subscription_logs_config: RpcTransactionLogsConfig,
    ) -> Result<WebSocket<Initialized>, eyre::Error> {
        let subscription_string = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "logsSubscribe",
            "params": [subscription_logs_filter, subscription_logs_config]
        })
        .to_string();

        let mut ws = Self {
            socket: None,
            config,
            subscription_string: Some(subscription_string),
            status: PhantomData,
        };

        ws.connect_and_subscribe().await?;
        Ok(WebSocket::from_uninitialized(ws))
    }

    async fn connect_and_subscribe(&mut self) -> Result<(), eyre::Error> {
        let mut socket = attempt_connection(&self.config.url, self.config.num_retries).await?;
        attempt_subscription(
            &self.subscription_string.clone().unwrap(),
            &mut socket,
            self.config.num_retries,
        )
        .await?;
        self.socket.replace(socket);
        Ok(())
    }
}

impl WebSocket<Initializing> {
    pub fn from_uninitialized(uninitialized: WebSocket) -> WebSocket<Initialized> {
        WebSocket::<Initialized> {
            socket: uninitialized.socket,
            config: uninitialized.config,
            subscription_string: uninitialized.subscription_string,
            status: PhantomData,
        }
    }
}

async fn attempt_connection(
    url: &str,
    mut num_retries: u8,
) -> Result<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, eyre::Error> {
    loop {
        if num_retries == 0 {
            return Err(eyre!("failed to connect after 5 tries"));
        }
        let maybe_ws_stream = connect_async(Url::parse(url).unwrap()).await;
        if maybe_ws_stream.is_err() {
            tracing::warn!(
                "Failed to connect to websocket {:?}",
                maybe_ws_stream.unwrap_err()
            );
            num_retries -= 1;
            continue;
        }
        let (ws_stream, _) = maybe_ws_stream.unwrap();
        break Ok(ws_stream);
    }
}

async fn attempt_subscription(
    subscription_string: &str,
    socket: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    mut num_retries: u8,
) -> Result<(), eyre::Error> {
    loop {
        if num_retries == 0 {
            return Err(eyre!("Failed to subscribe to websocket"));
        }
        let subscription_result = subscribe(socket.borrow_mut(), subscription_string).await;
        match subscription_result {
            Ok(()) => {
                tracing::debug!("Successfully subscribed to ws");
                return Ok(());
            }
            Err(e) => {
                tracing::warn!("Failed to subscribe to ws: {}", e);
                num_retries -= 1;
                continue;
            }
        };
    }
}

async fn subscribe(
    socket: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    subscription_string: &str,
) -> Result<(), eyre::Error> {
    let (mut write, mut read) = socket.split();
    let _ = write
        .send(Message::from(subscription_string.to_string()))
        .await;
    let _ = serde_json::from_str::<SubscriptionResponse>(
        &read
            .next()
            .await
            .ok_or_eyre("Failed to read subscription response")??
            .to_string(),
    );
    Ok(())
}

#[allow(unused)]
#[derive(Debug, serde::Deserialize)]
pub struct LogsSubscribeResponse {
    pub jsonrpc: String,
    pub method: String,
    pub params: SubscribeResponseParams,
}

#[allow(unused)]
#[derive(Debug, serde::Deserialize)]
pub struct SubscribeResponseParams {
    pub subscription: u32,
    pub result: Response<RpcLogsResponse>,
}

#[allow(unused)]
#[derive(Debug, serde::Deserialize)]
struct SubscriptionResponse {
    jsonrpc: String,
    result: u64,
    id: u64,
}
