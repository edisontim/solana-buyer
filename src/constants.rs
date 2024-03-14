use std::str::FromStr;

use lazy_static::lazy_static;
use solana_sdk::pubkey::Pubkey;

pub const RPC_URL: &str = "https://solana-mainnet.api.syndica.io/api-token/2abxeyBzL1sJfLQA4gNkSiRU8x3qNfPKog1fBzvjqgtMcbNs9zWUM3Q4z2Rct9yERcbsrowMpskkTm8qbmZNDLonhZPT4p5dhxF5W3s2jKRE4GpGBKG4x99fS1XMyrd7SpLuRQYWTMWZ6hWYRZmshrS2GJVAQRk4VdXQvCKWbyDbdP3kfM7N9W4Q1UxDDTXJzzvZm4FWtjQUydKvXAF2dcTR9CNEpowJYKrPPLmfwiw9Pq4zooBihgnNzqHYL5FSctBarxQYLzUvR6MGjbdeRrRofjBxabRAuUy1ws7JwENYAh4EXZQGpuVmebZLVnSTKVFdHdMKt2PzvkQrti1jvdQKwFN7AHuajEajRvqcvkrKQopYw6e4cXbCTznrHf5uenKn3U8AMsswyrtxG6zVdnWGQUS4QfQm3qaqET1DbxrbdooMXXFcfxYXmc8v3vp1dzamZbp3Af43ZDFCELaUFaiWZdYkLAk46TDstgXCeeUTRjs53rtJycMXuowtv";

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
    pub static ref IN_TOKEN: Pubkey = Pubkey::from_str(WSOL_ADDRESS).unwrap();
}
