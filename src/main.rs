use std::str::FromStr;

mod constants;
mod types;
mod utils;

use crate::{
    constants::{RAYDIUM_AUTHORITY_V4, TOKEN_PROGRAM},
    types::{MarketInfo, PoolInfo},
    utils::{get_associated_authority, get_market_info, get_pool_info, get_user_accounts},
};
use constants::{AMM_V4, IN_TOKEN, RPC_URL, WSOL_ADDRESS};
use raydium_contract_instructions::amm_instruction as amm;

use spl_associated_token_account::instruction::create_associated_token_account;

use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig};

use utils::{get_market_id, get_prio_fee};

use std::sync::Arc;

use solana_sdk::{
    commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair, signer::Signer,
    transaction::Transaction,
};

use clap::Parser;

/// Buy and sell memecoins
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Output token address
    #[arg(short, long)]
    output_token_address: String,

    /// Amount in decimals in (1Sol = 1000000000 lamports)
    #[arg(short, long, default_value_t = 1.0)]
    amount_in: f64,

    /// Slipage in %
    #[arg(short, long, default_value_t = 4)]
    slipage: u64,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let client = Arc::new(RpcClient::new(String::from_str(RPC_URL).unwrap()));
    let user_keypair = Keypair::from_base58_string(
        "cgzhM6dQr6BtfrC78QucfirPbud13ethe8fv37egUP262Nnx1WPkDaabxNtiLKBdn6fnH7TbPhhh1SwgCAhEtif",
    );

    let out_token = Pubkey::from_str(&args.output_token_address).expect("Invalid in token address");

    let market_id = get_market_id(&client, WSOL_ADDRESS, &args.output_token_address).await;

    let amm_id = Pubkey::find_program_address(
        &[AMM_V4.as_ref(), market_id.as_ref(), b"amm_associated_seed"],
        &AMM_V4,
    )
    .0;

    let pool_info = get_pool_info(&client, &amm_id).await;
    let associated_authority =
        get_associated_authority(pool_info.market_program_id, pool_info.market_id).unwrap();

    let market_info = get_market_info(&client, &pool_info.market_id).await;

    swap(
        &client,
        &user_keypair,
        amm_id,
        pool_info,
        market_info,
        &associated_authority,
        &out_token,
        args.amount_in,
        args.slipage as f64,
    )
    .await;
}

async fn swap(
    client: &Arc<RpcClient>,
    user_keypair: &Keypair,
    amm_id: Pubkey,
    pool_info: PoolInfo,
    market_info: MarketInfo,
    associated_authority: &Pubkey,
    out_token: &Pubkey,
    amount_in: f64,
    slipage: f64,
) {
    let (user_in_token_account, user_out_token_account) =
        get_user_accounts(&client, &user_keypair, *IN_TOKEN, *out_token, amount_in)
            .await
            .unwrap();

    let base_vault_balance_info = client
        .get_token_account_balance(&pool_info.base_vault)
        .await
        .unwrap();

    let quote_vault_balance_info = client
        .get_token_account_balance(&pool_info.quote_vault)
        .await
        .unwrap();

    let base_vault_balance = base_vault_balance_info.amount.parse::<f64>().unwrap();
    let quote_vault_balance = quote_vault_balance_info.amount.parse::<f64>().unwrap();
    println!("base reserves {:?}", base_vault_balance);
    println!("quote reserves {:?}", quote_vault_balance);

    let (compute_unit_limit_instruction, compute_unit_price_instruction) =
        get_prio_fee(client).await;
    let mut instructions = vec![];

    instructions.push(compute_unit_limit_instruction);
    instructions.push(compute_unit_price_instruction);

    let associated_token_account_create_instruction = create_associated_token_account(
        &user_keypair.pubkey(),
        &user_keypair.pubkey(),
        out_token,
        &TOKEN_PROGRAM,
    );
    instructions.push(associated_token_account_create_instruction);

    if pool_info.base_mint == *IN_TOKEN {
        let price_per_in_token: f64 = quote_vault_balance / base_vault_balance;
        println!("price_per_lamport {}", price_per_in_token);
        println!(
            "number of decimals for NUDE {}",
            quote_vault_balance_info.decimals
        );
        let amount_in: f64 = amount_in * 10_f64.powi(quote_vault_balance_info.decimals.into());

        let amount_out: f64 = Into::<f64>::into(amount_in) * price_per_in_token;
        let amount_out: f64 = amount_out * (100.0 - slipage) / 100.0;

        let amount_out: u64 = amount_out as u64;
        let amount_in: u64 = amount_in as u64;
        println!("Initializing swap with output tokens as pool base token");
        println!("trading {} in for minimum {} out", amount_in, amount_out);
        debug_assert!(pool_info.quote_mint == *out_token);
        let swap_instruction = amm::swap_base_in(
            &amm::ID,
            &amm_id,
            &RAYDIUM_AUTHORITY_V4,
            &pool_info.open_orders,
            &pool_info.target_orders,
            &pool_info.base_vault,
            &pool_info.quote_vault,
            &pool_info.market_program_id,
            &pool_info.market_id,
            &market_info.bids,
            &market_info.asks,
            &market_info.event_queue,
            &market_info.base_vault,
            &market_info.quote_vault,
            associated_authority,
            &user_in_token_account,
            &user_out_token_account,
            &user_keypair.pubkey(),
            amount_in as u64,
            amount_out as u64,
        )
        .unwrap();
        instructions.push(swap_instruction);
    } else {
        let price_per_lamport: f64 = base_vault_balance / quote_vault_balance;
        println!("price_per_lamport {}", price_per_lamport);
        println!(
            "number of decimals for NUDE {}",
            quote_vault_balance_info.decimals
        );
        let lamports_in_amount: f64 =
            amount_in * 10_f64.powi(quote_vault_balance_info.decimals.into());

        let amount_out: f64 = Into::<f64>::into(lamports_in_amount) * price_per_lamport;
        let amount_out: f64 = amount_out * (100.0 - slipage) / 100.0;
        let amount_out: u64 = amount_out as u64;
        let lamports_in: u64 = lamports_in_amount as u64;
        println!("trading {} in for minimum {} out", lamports_in, amount_out);
        debug_assert!(pool_info.quote_mint == *IN_TOKEN && pool_info.base_mint == *out_token);
        let swap_instruction = amm::swap_base_out(
            &amm::ID,
            &amm_id,
            &RAYDIUM_AUTHORITY_V4,
            &pool_info.open_orders,
            &pool_info.target_orders,
            &pool_info.base_vault,
            &pool_info.quote_vault,
            &pool_info.market_program_id,
            &pool_info.market_id,
            &market_info.bids,
            &market_info.asks,
            &market_info.event_queue,
            &market_info.base_vault,
            &market_info.quote_vault,
            associated_authority,
            &user_in_token_account,
            &user_out_token_account,
            &user_keypair.pubkey(),
            lamports_in,
            amount_out,
        )
        .unwrap();
        instructions.push(swap_instruction);
    }

    let recent_blockhash = client
        .get_latest_blockhash_with_commitment(solana_sdk::commitment_config::CommitmentConfig {
            commitment: solana_sdk::commitment_config::CommitmentLevel::Finalized,
        })
        .await
        .unwrap()
        .0;

    println!("recent_blockhash {}", recent_blockhash);
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&user_keypair.pubkey()),
        &vec![user_keypair],
        recent_blockhash,
    );

    if let Err(e) = client
        .send_and_confirm_transaction_with_spinner_and_config(
            &transaction,
            CommitmentConfig::confirmed(),
            RpcSendTransactionConfig {
                skip_preflight: true,
                ..RpcSendTransactionConfig::default()
            },
        )
        .await
    {
        println!("{e}");
    };
}
