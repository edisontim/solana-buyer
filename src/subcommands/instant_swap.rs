use std::{str::FromStr, sync::Arc};

use clap::Args;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

use crate::{actors::swapper::actor::Swapper, types::ProgramConfig, utils::get_market_id};

#[derive(Debug, Args)]
pub struct InstantSwapSubcommand {
    /// Input token address
    #[arg(short, long)]
    pub input_token_address: String,

    /// Output token address
    #[arg(short, long)]
    pub output_token_address: String,

    /// Amount in decimals in
    #[arg(short, long)]
    pub amount_in: f64,
}

impl InstantSwapSubcommand {
    pub async fn run(self, client: Arc<RpcClient>, config: ProgramConfig) {
        let market_id = get_market_id(
            &client,
            &self.input_token_address,
            &self.output_token_address,
        )
        .await;

        let swapper = Swapper::new(client, config, market_id, self.amount_in)
            .await
            .expect("failed to swap");
        swapper
            .swap(
                &Pubkey::from_str(&self.input_token_address)
                    .expect("Enter correct input token address"),
                self.amount_in,
            )
            .await;

        tracing::info!("sell how much?");
        let mut amount = String::new();
        let _ = std::io::stdin().read_line(&mut amount).unwrap();
        let amount_in: f64 = amount.trim().parse().unwrap();

        swapper
            .swap(
                &Pubkey::from_str(&self.output_token_address)
                    .expect("Enter correct output token address"),
                amount_in,
            )
            .await;
    }
}
