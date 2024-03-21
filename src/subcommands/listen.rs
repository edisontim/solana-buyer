use std::sync::Arc;

use clap::Args;
use solana_client::nonblocking::rpc_client::RpcClient;

use crate::{listener::Listener, types::ProgramConfig};
#[derive(Debug, Args)]
pub struct ListenSubcommand;

impl ListenSubcommand {
    pub async fn run(self, client: Arc<RpcClient>, config: ProgramConfig) {
        let listener = Listener::from_config(client, config);
        listener.listen().await;
    }
}
