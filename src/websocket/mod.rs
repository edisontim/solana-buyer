use std::{borrow::BorrowMut, net::TcpStream};

use eyre::eyre;
use serde::de::DeserializeOwned;
use serde_json::json;
use solana_client::{
    rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter},
    rpc_response::{Response, RpcLogsResponse},
};
use std::marker::PhantomData;
use tungstenite::{connect, Message};
use url::Url;

#[allow(dead_code)]
pub struct Uninitialized;
#[allow(dead_code)]
pub struct Initialized;
#[allow(dead_code)]
pub struct Initializing;

pub struct WebSocket<Status = Uninitialized> {
    socket: Option<tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>>,
    config: WebSocketConfig,
    subscription_string: Option<String>,
    status: PhantomData<Status>,
}

pub struct WebSocketConfig {
    pub num_retries: u8,
    pub url: String,
}

impl WebSocket<Initialized> {
    pub fn read<T: DeserializeOwned + std::fmt::Debug>(&mut self) -> Result<T, eyre::Error> {
        if self.socket.is_none() {
            return Err(eyre!("Use subscription function before read"));
        }
        loop {
            let read_result = self.socket.as_mut().unwrap().read();
            if read_result.is_err() {
                log::warn!("Connection lost: {}", read_result.err().unwrap());
                let _ = self.socket.as_mut().unwrap().close(None);
                let _ = self.socket.as_mut().unwrap().flush();
                self.reconnect()?;
                self.config.num_retries -= 1;
                continue;
            }
            let msg = read_result.unwrap().to_string();
            let deserialize_result = serde_json::from_str::<T>(&msg);
            if deserialize_result.is_err() {
                log::warn!(
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

    pub fn reconnect(&mut self) -> Result<(), eyre::Error> {
        let mut socket = attempt_connection(&self.config.url, self.config.num_retries)?;
        attempt_subscription(
            &self.subscription_string.clone().unwrap(),
            &mut socket,
            self.config.num_retries,
        )?;
        self.socket.replace(socket);
        Ok(())
    }
}

impl WebSocket<Uninitialized> {
    pub fn create_new_logs_subscription(
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

        ws.connect_and_subscribe()?;
        Ok(WebSocket::from_uninitialized(ws))
    }

    fn connect_and_subscribe(&mut self) -> Result<(), eyre::Error> {
        let mut socket = attempt_connection(&self.config.url, self.config.num_retries)?;
        attempt_subscription(
            &self.subscription_string.clone().unwrap(),
            &mut socket,
            self.config.num_retries,
        )?;
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

fn attempt_connection(
    url: &str,
    mut num_retries: u8,
) -> Result<tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>, eyre::Error> {
    loop {
        if num_retries == 0 {
            return Err(eyre!("failed to connect after 5 tries"));
        }
        let connection_result = connect(Url::parse(url).unwrap());
        if connection_result.is_err() {
            log::warn!("Failed to connect websocket");
            num_retries -= 1;
            continue;
        }
        let (new_socket, _) = connection_result.unwrap();
        break Ok(new_socket);
    }
}

fn attempt_subscription(
    subscription_string: &str,
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    mut num_retries: u8,
) -> Result<(), eyre::Error> {
    loop {
        if num_retries == 0 {
            return Err(eyre!("Failed to subscribe to websocket"));
        }
        let subscription_result = subscribe(socket.borrow_mut(), subscription_string);
        match subscription_result {
            Ok(()) => {
                log::debug!("Successfully subscribed to ws");
                return Ok(());
            }
            Err(e) => {
                log::warn!("Failed to subscribe to ws: {}", e);
                num_retries -= 1;
                continue;
            }
        };
    }
}

fn subscribe(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    subscription_string: &str,
) -> Result<(), eyre::Error> {
    socket
        .send(Message::Text(subscription_string.to_string()))
        .unwrap();
    let _ = serde_json::from_str::<SubscriptionResponse>(&socket.read()?.to_string());
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
    result: u8,
    id: u8,
}