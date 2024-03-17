use std::str::FromStr;

mod constants;
mod listener;
mod swapper;
mod types;
mod utils;

use crate::{swapper::Swapper, types::Args};

use listener::Listener;
use solana_client::nonblocking::rpc_client::RpcClient;

use types::{Command, Config};
use utils::get_market_id;

use std::sync::Arc;

use solana_sdk::pubkey::Pubkey;

use clap::Parser;

#[tokio::main]
async fn main() {
    let config = Config::from_dotenv();

    let args = Args::parse();

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

    listener.listen();
}

async fn instant_swap(
    config: Config,
    input_token_address: String,
    output_token_address: String,
    amount_in: f64,
    slippage: u64,
) {
    let client = Arc::new(RpcClient::new(
        String::from_str(&config.http_rpc_url).unwrap(),
    ));

    let market_id = get_market_id(&client, &input_token_address, &output_token_address).await;

    let swapper = Swapper::new(market_id, config).await;
    swapper
        .swap(
            &Pubkey::from_str(&input_token_address).expect("Enter correct input token address"),
            amount_in,
            slippage as f64,
        )
        .await;

    println!("sell how much?");
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
