use eyre::eyre;
use solana_account_decoder::UiAccountEncoding;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
};
use solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta;
use spl_associated_token_account::get_associated_token_address;

use crate::{
    constants::OPENBOOK,
    types::{MarketInfo, PoolInfo},
};

use borsh::BorshDeserialize;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcTransactionConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType},
};

use std::sync::Arc;

pub fn init_logging() {
    if std::env::var("RUST_LOG").is_ok() {
        std::env::set_var(
            "RUST_LOG",
            "solana_buyer=".to_owned() + &std::env::var("RUST_LOG").unwrap(),
        )
    }

    env_logger::init();
}

pub fn get_prio_fee_instructions() -> (Instruction, Instruction) {
    let prio_fee = 130_000;
    log::debug!("avg prio fee {:?}", prio_fee);
    let compute_unit_limit_instruction = ComputeBudgetInstruction::set_compute_unit_limit(70_000);
    let compute_unit_price_instruction = ComputeBudgetInstruction::set_compute_unit_price(prio_fee);
    (
        compute_unit_limit_instruction,
        compute_unit_price_instruction,
    )
}

pub fn get_associated_authority(program_id: Pubkey, market_id: Pubkey) -> Option<Pubkey> {
    let seeds = market_id.to_bytes();
    for nonce in 0..100 {
        let seeds: Vec<u8> = seeds.to_vec();
        let nonce_as_array: Vec<u8> = vec![nonce];
        let padding: Vec<u8> = vec![0, 0, 0, 0, 0, 0, 0];
        let key = Pubkey::create_program_address(&[&seeds, &nonce_as_array, &padding], &program_id);
        if let Ok(k) = key {
            return Some(k);
        }
    }
    None
}

pub async fn get_pool_and_market_info(
    client: &RpcClient,
    amm_id: &Pubkey,
    market_id: &Pubkey,
) -> (PoolInfo, MarketInfo) {
    let mut rpc_response = client
        .get_multiple_accounts_with_config(
            &[*amm_id, *market_id],
            RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                data_slice: None,
                commitment: Some(CommitmentConfig::processed()),
                ..RpcAccountInfoConfig::default()
            },
        )
        .await
        .unwrap();
    let pool_account = rpc_response.value.remove(0).unwrap();
    let pool_info = PoolInfo::deserialize(&mut &pool_account.data[..]).unwrap();
    let market_account = rpc_response.value.pop().unwrap().unwrap();
    let market_info = MarketInfo::deserialize(&mut &market_account.data[..]).unwrap();
    (pool_info, market_info)
}

pub async fn get_pool_info(client: &RpcClient, amm_id: &Pubkey) -> PoolInfo {
    let pool_info = client.get_account_data(amm_id).await.unwrap();
    PoolInfo::deserialize(&mut &pool_info[..]).unwrap()
}

pub async fn get_market_info(client: &RpcClient, market_id: &Pubkey) -> MarketInfo {
    let market_info = client.get_account_data(market_id).await.unwrap();
    MarketInfo::deserialize(&mut &market_info[..]).unwrap()
}

pub async fn get_user_token_accounts(
    client: &Arc<RpcClient>,
    user_keypair: &Keypair,
    base_token: Pubkey,
    quote_token: Pubkey,
) -> Result<(Pubkey, Pubkey, Option<Pubkey>), eyre::Error> {
    let mut account_to_create: Option<Pubkey> = None;

    let user_base_token_account = get_associated_token_address(&user_keypair.pubkey(), &base_token);
    let user_quote_token_account =
        get_associated_token_address(&user_keypair.pubkey(), &quote_token);

    let mut user_token_accounts = client
        .get_multiple_accounts_with_config(
            &[user_base_token_account, user_quote_token_account],
            RpcAccountInfoConfig {
                commitment: Some(CommitmentConfig::processed()),
                ..RpcAccountInfoConfig::default()
            },
        )
        .await?
        .value;

    match user_token_accounts.swap_remove(0) {
        Some(_) => log::debug!("User's ATA for base token exists. Skipping creation.."),
        None => {
            log::debug!("User's ATA for base token does not exist. Creating..");
            account_to_create = Some(base_token);
        }
    };

    match user_token_accounts.swap_remove(0) {
        Some(_) => log::debug!("User's ATA for quote tokens exists. Skipping creation.."),
        None => {
            log::debug!("User's ATA for quote token does not exist. Creating..");
            account_to_create = Some(quote_token);
        }
    }

    log::debug!("account to create: {:?}", account_to_create);
    Ok((
        user_base_token_account,
        user_quote_token_account,
        account_to_create,
    ))
}

/// Fetches the serum marketID of the pool
pub async fn get_market_id(
    rpc_client: &RpcClient,
    base_mint_address: &str,
    target_mint_address: &str,
) -> Pubkey {
    let candidate_market_id =
        get_candidate_market_id(rpc_client, base_mint_address, target_mint_address).await;
    if let Some((market_id, _)) = candidate_market_id {
        market_id
    } else {
        get_candidate_market_id(rpc_client, target_mint_address, base_mint_address)
            .await
            .unwrap()
            .0
    }
}

async fn get_candidate_market_id(
    rpc_client: &RpcClient,
    base_mint_address: &str,
    target_mint_address: &str,
) -> Option<(Pubkey, solana_sdk::account::Account)> {
    const BASEMINT_OFFSET: usize = 53; // offset of 'BaseMint'
    let base_mint_memcmp = RpcFilterType::Memcmp(Memcmp::new(
        BASEMINT_OFFSET,
        MemcmpEncodedBytes::Base58(base_mint_address.to_string()),
    ));

    const TARGETMINT_OFFSET: usize = 85;
    let target_mint_memcmp = RpcFilterType::Memcmp(Memcmp::new(
        TARGETMINT_OFFSET, // offset of 'TargetMint'
        MemcmpEncodedBytes::Base58(target_mint_address.to_string()),
    ));

    rpc_client
        .get_program_accounts_with_config(
            &OPENBOOK,
            RpcProgramAccountsConfig {
                filters: Some(vec![base_mint_memcmp, target_mint_memcmp]),
                account_config: RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    ..RpcAccountInfoConfig::default()
                },
                with_context: Some(true),
            },
        )
        .await
        .unwrap()
        .pop()
}

pub async fn get_transaction_from_signature(
    client: &RpcClient,
    signature: Signature,
    rpc_transaction_config: RpcTransactionConfig,
) -> Result<EncodedConfirmedTransactionWithStatusMeta, eyre::Error> {
    let get_transaction_result = client
        .get_transaction_with_config(&signature, rpc_transaction_config)
        .await;

    if get_transaction_result.is_err() {
        return Err(eyre!(
            "Failed to get transaction: {:?}",
            get_transaction_result.err()
        ));
    }

    let transaction = get_transaction_result.unwrap();
    Ok(transaction)
}
