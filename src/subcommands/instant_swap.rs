use clap::Args;
#[derive(Debug, Args)]
pub struct InstantSwapSubcommand {
    /// Input token address
    #[arg(short, long)]
    pub input_token_address: String,

    /// Output token address
    #[arg(short, long)]
    pub output_token_address: String,

    /// Amount in decimals in (-1 for max)
    #[arg(short, long)]
    pub amount_in: f64,

    /// Slippage in %
    #[arg(short, long)]
    pub slippage: u64,
}
