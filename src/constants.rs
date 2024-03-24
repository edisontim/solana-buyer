use std::str::FromStr;

use lazy_static::lazy_static;
use solana_sdk::pubkey::Pubkey;

/// Wrapped Solana token address. WSOL is a wrapped version of SOL that enables it to be easily used within DeFi
pub const WSOL_ADDRESS: &str = "So11111111111111111111111111111111111111112";

/// Account address that receives the fees when someone creates a Raydium
pub const CREATE_POOL_FEE_ACCOUNT_ADDRESS: &str = "7YttLkHDoNj9wyDur5pM1ejNaAvT9X4eqaYcHQqtj2G5";

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
    pub static ref MIN_LIQUIDITY: u64 = 25_000_000_000; // 25 billion lamports ~ 4200$
    pub static ref MAX_LIQUIDITY: u64 = 150_000_000_000; // 150 billion lamports ~ 25000$
}

pub const AMM_ID_INDEX_IN_INIT_INSTRUCTION: usize = 4;
pub const MARKET_ID_INDEX_IN_INIT_INSTRUCTION: usize = 16;
pub const BASE_MINT_INDEX_IN_INIT_INSTRUCTION: usize = 8;
pub const QUOTE_MINT_INDEX_IN_INIT_INSTRUCTION: usize = 9;
