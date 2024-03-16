use std::str::FromStr;

use lazy_static::lazy_static;
use solana_sdk::pubkey::Pubkey;

pub const WSOL_ADDRESS: &str = "So11111111111111111111111111111111111111112";

lazy_static! {
    pub static ref OPENBOOK: Pubkey =
        Pubkey::from_str("srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX").unwrap();
    pub static ref SERUM_MARKET: Pubkey =
        Pubkey::from_str("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin").unwrap();
    pub static ref AMM_V4: Pubkey =
        Pubkey::from_str("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8").unwrap();
    pub static ref RAYDIUM_AUTHORITY_V4: Pubkey =
        Pubkey::from_str("5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1").unwrap();
    pub static ref TOKEN_PROGRAM: Pubkey =
        Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
    pub static ref SOL: Pubkey = Pubkey::from_str(WSOL_ADDRESS).unwrap();
}
