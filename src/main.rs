use std::str::FromStr;

mod constants;
mod listener;
mod subcommands;
mod swapper;
mod types;
mod utils;
mod websocket;

use {
    subcommands::{Args, Subcommands},
    types::ProgramConfig,
};

use clap::Parser;

use solana_client::nonblocking::rpc_client::RpcClient;

use utils::init_logging;

use std::sync::Arc;

#[tokio::main]
async fn main() {
    init_logging();

    let config = ProgramConfig::from_dotenv();

    let args = Args::parse();

    let client = Arc::new(RpcClient::new(
        String::from_str(&config.http_rpc_url).unwrap(),
    ));

    match args.command {
        Subcommands::Listen(listen) => listen.run(client, config).await,
        Subcommands::InstantSwap(instant_swap) => instant_swap.run(client, config).await,
    }
}
