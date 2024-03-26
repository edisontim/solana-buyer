use borsh::BorshDeserialize;
use eyre::Result;
use eyre::{eyre, OptionExt};
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcTransactionConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType},
};
use solana_sdk::account::{Account, ReadableAccount};
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
use tracing_subscriber::{filter, FmtSubscriber};

use crate::actors::swapper::actor::PoolInitTxInfos;
use crate::types::{TokenAccount, UserTokenAccounts};
use crate::{
    constants::OPENBOOK,
    types::{MarketInfo, PoolInfo},
};

pub fn init_logging() {
    let filter = if let Ok(filter) = std::env::var("RUST_LOG") {
        filter
    } else {
        "solana_buyer=info".to_string()
    };
    let filter = filter::EnvFilter::new(filter);
    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting tracing default failed");
}

pub fn get_prio_fee_instructions() -> (Instruction, Instruction) {
    let prio_fee = 130_000;
    tracing::debug!("priority fee {:?}", prio_fee);
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

pub async fn get_accounts_for_swap(
    client: &RpcClient,
    user_keypair: &Keypair,
    pool_init_tx_infos: PoolInitTxInfos,
) -> Result<(PoolInfo, MarketInfo, UserTokenAccounts)> {
    let mut account_to_create: Option<Pubkey> = None;

    let user_base_token_account =
        get_associated_token_address(&user_keypair.pubkey(), &pool_init_tx_infos.base_mint);
    let user_quote_token_account =
        get_associated_token_address(&user_keypair.pubkey(), &pool_init_tx_infos.quote_mint);

    let rpc_response = client
        .get_multiple_accounts_with_config(
            &[
                pool_init_tx_infos.amm_id,
                pool_init_tx_infos.market_id,
                user_base_token_account,
                user_quote_token_account,
            ],
            RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                data_slice: None,
                commitment: Some(CommitmentConfig::processed()),
                ..RpcAccountInfoConfig::default()
            },
        )
        .await?;

    let pool_info_account = rpc_response
        .value
        .first()
        .cloned()
        .flatten()
        .ok_or_eyre("pool account not found")?;
    let pool_info = PoolInfo::deserialize(&mut pool_info_account.data())?;

    let market_account = rpc_response
        .value
        .get(1)
        .cloned()
        .flatten()
        .ok_or_eyre("market account not found")?;
    let market_info = MarketInfo::deserialize(&mut market_account.data())?;

    match rpc_response.value.get(2).unwrap() {
        Some(_) => tracing::info!("User's ATA for base token exists. Skipping creation.."),
        None => {
            tracing::info!("User's ATA for base token does not exist. Need to create..");
            account_to_create = Some(pool_init_tx_infos.base_mint);
        }
    };

    match rpc_response.value.get(3).unwrap() {
        Some(_) => tracing::info!("User's ATA for quote tokens exists. Skipping creation.."),
        None => {
            tracing::info!("User's ATA for quote token does not exist. Need to create..");
            account_to_create = Some(pool_init_tx_infos.quote_mint);
        }
    }

    Ok((
        pool_info,
        market_info,
        UserTokenAccounts {
            user_base_token_account,
            user_quote_token_account,
            account_to_create,
        },
    ))
}

pub async fn get_pool_and_market_info(
    client: &RpcClient,
    amm_id: &Pubkey,
    market_id: &Pubkey,
) -> Result<(PoolInfo, MarketInfo)> {
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
        .await?;

    let pool_account = rpc_response
        .value
        .remove(0)
        .ok_or_eyre("pool account not found")?;

    let pool_info = PoolInfo::deserialize(&mut &pool_account.data[..])?;
    let market_account = rpc_response
        .value
        .pop()
        .flatten()
        .ok_or_eyre("market account not found")?;
    let market_info = MarketInfo::deserialize(&mut &market_account.data[..])?;
    Ok((pool_info, market_info))
}

pub async fn get_token_accounts(
    client: &RpcClient,
    accounts_pub_keys: &[Pubkey],
) -> Result<Vec<TokenAccount>, eyre::Error> {
    let accounts: Vec<Account> = client
        .get_multiple_accounts_with_config(
            accounts_pub_keys,
            RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                data_slice: None,
                commitment: Some(CommitmentConfig::confirmed()),
                ..RpcAccountInfoConfig::default()
            },
        )
        .await
        .unwrap()
        .value
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| eyre!("Token accounts not found"))?;

    if accounts_pub_keys.len() != accounts.len() {
        return Err(eyre!("Token accounts not found"));
    }

    let account = accounts
        .into_iter()
        .map(|a| TokenAccount::deserialize(&mut a.data.as_slice()))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(account)
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
            "failed to get transaction: {:?}",
            get_transaction_result.err()
        ));
    }

    let transaction = get_transaction_result.unwrap();
    Ok(transaction)
}
