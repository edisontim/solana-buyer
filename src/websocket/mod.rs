use std::{borrow::BorrowMut, net::TcpStream, str::FromStr};

use eyre::eyre;
use serde::de::DeserializeOwned;
use serde_json::json;
use solana_client::{
    rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter},
    rpc_response::{Response, RpcLogsResponse},
};

use tungstenite::{connect, Message};
use url::Url;

pub struct WebSocket {
    socket: Option<tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>>,
    config: WebSocketConfig,
    subscription_string: Option<String>,
}

pub struct WebSocketConfig {
    pub num_retries: u8,
    pub url: String,
}

impl WebSocket {
    pub fn read<T: DeserializeOwned>(&mut self) -> Result<T, eyre::Error> {
        if self.socket.is_none() {
            return Err(eyre!("Use subscription function before read"));
        }
        if self.config.num_retries == 0 {
            return Err(eyre!("Failed to receive message"));
        }
        loop {
            let read_result = self.socket.as_mut().unwrap().read();
            if read_result.is_err() {
                log::debug!("Connection lost: {}", read_result.err().unwrap());
                let _ = self.socket.as_mut().unwrap().close(None);
                let _ = self.socket.as_mut().unwrap().flush();
                self.socket = None;
                self.socket = Some(self.connect_and_subscribe()?);
                self.config.num_retries -= 1;
                continue;
            }
            let msg = read_result.unwrap();
            let msg_string = msg.to_string();
            let deserialize_result = serde_json::from_str::<T>(&msg_string);
            if deserialize_result.is_err() {
                self.config.num_retries -= 1;
                continue;
            }
            self.config.num_retries = 5;
            return Ok(deserialize_result.unwrap());
        }
    }

    pub fn create_new_logs_subscription(
        config: WebSocketConfig,
        subscription_logs_filter: RpcTransactionLogsFilter,
        subscription_logs_config: RpcTransactionLogsConfig,
    ) -> Result<Self, eyre::Error> {
        let subscription_string = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "logsSubscribe",
            "params": [subscription_logs_filter, subscription_logs_config]
        })
        .to_string();

        let mut temp_self = Self {
            socket: None,
            config,
            subscription_string: Some(subscription_string),
        };

        let socket = temp_self.connect_and_subscribe();
        temp_self.socket = Some(socket?);
        Ok(temp_self)
    }

    pub fn as_mut(&mut self) -> &mut Self {
        self
    }

    fn connect_and_subscribe(
        self: &Self,
    ) -> Result<tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>, eyre::Error>
    {
        let mut socket = WebSocket::attemp_connection(&self.config.url, self.config.num_retries)?;
        self.attempt_subscription(&mut socket, self.config.num_retries)?;
        Ok(socket)
    }

    fn attemp_connection(
        url: &str,
        mut num_retries: u8,
    ) -> Result<tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>, eyre::Error>
    {
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
        self: &Self,
        socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
        mut num_retries: u8,
    ) -> Result<(), eyre::Error> {
        loop {
            if num_retries == 0 {
                return Err(eyre!("Failed to subscribe to websocket"));
            }
            let subscription_result = WebSocket::subscribe(
                socket.borrow_mut(),
                &self
                    .subscription_string
                    .clone()
                    .expect("No subscription string provided"),
            );
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
        let _ = socket
            .send(Message::Text(
                String::from_str(subscription_string).unwrap(),
            ))
            .unwrap();
        let _ = serde_json::from_str::<SubscriptionResponse>(&socket.read()?.to_string());
        Ok(())
    }
}

#[allow(unused)]
#[derive(Debug, serde::Deserialize)]
pub struct LogsSubscribeResponse {
    jsonrpc: String,
    method: String,
    params: SubscribeResponseParams,
}

#[allow(unused)]
#[derive(Debug, serde::Deserialize)]
struct SubscribeResponseParams {
    subscription: u8,
    result: Response<RpcLogsResponse>,
}

#[allow(unused)]
#[derive(Debug, serde::Deserialize)]
struct SubscriptionResponse {
    jsonrpc: String,
    result: u8,
    id: u8,
}
