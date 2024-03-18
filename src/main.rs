use std::str::FromStr;

mod constants;
mod listener;
mod swapper;
mod types;
mod utils;
mod websocket;

use crate::{swapper::Swapper, types::Args};

use constants::CREATE_POOL_FEE_ACCOUNT_ADDRESS;
use listener::Listener;
use websocket::{LogsSubscribeResponse, WebSocket, WebSocketConfig};

use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcTransactionLogsConfig};

use types::{Command, Config};
use utils::get_market_id;

use std::sync::Arc;

use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use clap::Parser;
use env_logger;

#[tokio::main]
async fn main() {
    env_logger::init();

    let config = Config::from_dotenv();

    let args = Args::parse();

    let client = Arc::new(RpcClient::new(
        String::from_str(&config.http_rpc_url).unwrap(),
    ));

    match args.command {
        Command::Listen => {
            listen(config);
        }
        Command::InstantSwap {
            input_token_address,
            output_token_address,
            amount_in,
            slippage,
        } => {
            instant_swap(
                client,
                config,
                input_token_address,
                output_token_address,
                amount_in,
                slippage,
            )
            .await
        }
    }
}

fn listen(config: Config) {
    let listener = Listener::from_config(config);
}

async fn instant_swap(
    client: Arc<RpcClient>,
    config: Config,
    input_token_address: String,
    output_token_address: String,
    amount_in: f64,
    slippage: u64,
) {
    let market_id = get_market_id(&client, &input_token_address, &output_token_address).await;

    let swapper = Swapper::new(client, market_id, config).await;
    swapper
        .swap(
            &Pubkey::from_str(&input_token_address).expect("Enter correct input token address"),
            amount_in,
            slippage as f64,
        )
        .await;

    log::info!("sell how much?");
    let mut amount = String::new();
    let _ = std::io::stdin().read_line(&mut amount).unwrap();
    let amount_in: f64 = amount.trim().parse().unwrap();

    swapper
        .swap(
            &Pubkey::from_str(&output_token_address).expect("Enter correct output token address"),
            amount_in,
            slippage as f64,
        )
        .await;
}
