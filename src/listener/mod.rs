use std::str::FromStr;

use crate::types::Config;
use solana_client::nonblocking::rpc_client::RpcClient;

pub struct Listener {
    client: RpcClient,
    ws_rpc_url: String,
}

impl Listener {
    pub fn from_config(config: Config) -> Self {
        let client = RpcClient::new(String::from_str(&config.http_rpc_url).unwrap());
        Self {
            client,
            ws_rpc_url: config.ws_rpc_url,
        }
    }
    pub fn listen(self: Self) {}
}
