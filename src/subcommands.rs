use clap::{Parser, Subcommand};

pub mod instant_swap;
pub mod listen;

use instant_swap::InstantSwapSubcommand;
use listen::ListenSubcommand;

/// Buy and sell memecoins
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Subcommands,
}

#[derive(Debug, Subcommand)]
pub enum Subcommands {
    InstantSwap(InstantSwapSubcommand),
    Listen(ListenSubcommand),
}
