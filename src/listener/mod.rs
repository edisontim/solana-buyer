use std::str::FromStr;

use crate::{constants::CREATE_POOL_FEE_ACCOUNT_ADDRESS, types::Config};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_response::{Response, RpcLogsResponse},
};

use tungstenite::{connect, Message};
use url::Url;

pub struct Listener {
    http_client: RpcClient,
    ws_rpc_url: String,
}

impl Listener {
    pub fn from_config(config: Config) -> Self {
        let http_client = RpcClient::new(String::from_str(&config.http_rpc_url).unwrap());
        Self {
            http_client,
            ws_rpc_url: config.ws_rpc_url,
        }
    }
    pub fn listen(self: Self) {
        WebSocket::logs_subscribe(&self.ws_rpc_url);
    }
}

pub struct WebSocket {}

impl WebSocket {
    pub fn logs_subscribe(ws_rpc_url: &str) {
        let (mut socket, _) = connect(Url::parse(ws_rpc_url).unwrap()).expect("Can't connect");
        let connection_msg = format!(
            r#"{{"jsonrpc": "2.0","id": 1,"method": "logsSubscribe","params": [{{"mentions": [ "{}" ]}},{{"commitment": "confirmed"}}]}}"#,
            CREATE_POOL_FEE_ACCOUNT_ADDRESS
        );
        let _ = socket
            .send(Message::Text(connection_msg.clone().into()))
            .unwrap();
        loop {
            let msg: Result<Message, tungstenite::Error> = socket.read();

            match msg {
                Ok(val) => {
                    let parsed = serde_json::from_str::<LogsSubscribeResponse>(&val.to_string());
                    match parsed {
                        Ok(parsed_logs) => println!("parsed {:?}", parsed_logs),
                        Err(_) => {
                            serde_json::from_str::<SubscriptionResponse>(&val.to_string()).expect("expected either a Logs response or SubscriptionResponse, got something else");
                        }
                    };
                }
                Err(e) => {
                    println!("Failure in websocket {}", e);
                    let _ = socket
                        .send(Message::Text(connection_msg.clone().into()))
                        .unwrap();
                    let _: Result<Message, tungstenite::Error> = socket.read();
                }
            };
        }
    }
}

#[allow(unused)]
#[derive(Debug, serde::Deserialize)]
struct LogsSubscribeResponse {
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
