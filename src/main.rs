use std::str::FromStr;

mod constants;
mod listener;
mod subcommands;
mod swapper;
mod types;
mod utils;
mod websocket;

use {
    subcommands::{instant_swap::InstantSwapSubcommand, Args, Subcommands},
    swapper::Swapper,
    types::Config,
};

use clap::Parser;
use constants::CREATE_POOL_FEE_ACCOUNT_ADDRESS;
use listener::Listener;
use websocket::{LogsSubscribeResponse, WebSocket, WebSocketConfig};

use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcTransactionLogsConfig};

use utils::{get_market_id, init_logging};

use std::sync::Arc;

use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use env_logger;

#[tokio::main]
async fn main() {
    init_logging();

    let config = Config::from_dotenv();

    let args = Args::parse();

    let client = Arc::new(RpcClient::new(
        String::from_str(&config.http_rpc_url).unwrap(),
    ));

    match args.command {
        Subcommands::Listen(_) => {
            listen(config);
        }
        Subcommands::InstantSwap(args) => instant_swap(client, config, args).await,
    }
}

fn listen(config: Config) {
    let listener = Listener::from_config(config);
}

async fn instant_swap(client: Arc<RpcClient>, config: Config, args: InstantSwapSubcommand) {
    let market_id = get_market_id(
        &client,
        &args.input_token_address,
        &args.output_token_address,
    )
    .await;

    let swapper = Swapper::new(client, market_id, config).await;
    swapper
        .swap(
            &Pubkey::from_str(&args.input_token_address)
                .expect("Enter correct input token address"),
            args.amount_in,
            args.slippage as f64,
        )
        .await;

    log::info!("sell how much?");
    let mut amount = String::new();
    let _ = std::io::stdin().read_line(&mut amount).unwrap();
    let amount_in: f64 = amount.trim().parse().unwrap();

    swapper
        .swap(
            &Pubkey::from_str(&args.output_token_address)
                .expect("Enter correct output token address"),
            amount_in,
            args.slippage as f64,
        )
        .await;
}
