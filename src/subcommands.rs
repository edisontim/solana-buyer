use clap::{Parser, Subcommand};

pub mod index;
pub mod instant_swap;
pub mod listen;

use instant_swap::InstantSwapSubcommand;
use listen::ListenSubcommand;

use self::index::IndexSubcommand;

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
    Index(IndexSubcommand),
}
