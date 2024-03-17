use solana_account_decoder::UiAccountEncoding;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, pubkey::Pubkey,
    signature::Keypair, signer::Signer,
};

use crate::{
    constants::OPENBOOK,
    types::{MarketInfo, PoolInfo},
};

use borsh::BorshDeserialize;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType},
};

use spl_token_client::{
    client::{ProgramClient, ProgramRpcClient, ProgramRpcClientSendTransaction},
    token::{Token, TokenError},
};

use std::{str::FromStr, sync::Arc};

pub async fn get_prio_fee_instructions(client: &RpcClient) -> (Instruction, Instruction) {
    let mut recent_prio_fees = client.get_recent_prioritization_fees(&[]).await.unwrap();
    recent_prio_fees.retain(|x| x.prioritization_fee != 0);

    let total_fees: u64 = recent_prio_fees
        .iter()
        .fold(0, |acc, x| acc + x.prioritization_fee);
    let mut average_prio_fee = total_fees / recent_prio_fees.len() as u64;
    if average_prio_fee < 12000 {
        average_prio_fee = 100_000;
    }
    println!("avg prio fee {:?}", average_prio_fee);
    let compute_unit_limit_instruction = ComputeBudgetInstruction::set_compute_unit_limit(70_000);
    let compute_unit_price_instruction =
        ComputeBudgetInstruction::set_compute_unit_price(average_prio_fee);
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
    let user = user_keypair.pubkey();

    let program_client = get_program_rpc(Arc::clone(client));
    let base_token_client = Token::new(
        Arc::clone(&program_client),
        &spl_token::ID,
        &base_token,
        None,
        Arc::new(Keypair::from_bytes(&user_keypair.to_bytes()).expect("failed to copy keypair")),
    );
    let quote_token_client = Token::new(
        Arc::clone(&program_client),
        &spl_token::ID,
        &quote_token,
        None,
        Arc::new(Keypair::from_bytes(&user_keypair.to_bytes()).expect("failed to copy keypair")),
    );

    let user_base_token_account = base_token_client.get_associated_token_address(&user);
    match base_token_client
        .get_account_info(&user_base_token_account)
        .await
    {
        Ok(_) => println!("User's ATA for input tokens exists. Skipping creation.."),
        Err(TokenError::AccountNotFound) | Err(TokenError::AccountInvalidOwner) => {
            println!("User's input-tokens ATA does not exist. Creating..");
            account_to_create = Some(base_token);
        }
        Err(error) => println!("Error retrieving user's input-tokens ATA: {}", error),
    };

    let user_quote_token_account = quote_token_client.get_associated_token_address(&user);
    match quote_token_client
        .get_account_info(&user_quote_token_account)
        .await
    {
        Ok(_) => println!("User's ATA for output tokens exists. Skipping creation.."),
        Err(TokenError::AccountNotFound) | Err(TokenError::AccountInvalidOwner) => {
            account_to_create = Some(quote_token);
        }
        Err(error) => println!("Error retrieving user's output-tokens ATA: {}", error),
    }
    return Ok((
        user_base_token_account,
        user_quote_token_account,
        account_to_create,
    ));
}

fn get_program_rpc(rpc: Arc<RpcClient>) -> Arc<dyn ProgramClient<ProgramRpcClientSendTransaction>> {
    let program_client: Arc<dyn ProgramClient<ProgramRpcClientSendTransaction>> = Arc::new(
        ProgramRpcClient::new(rpc.clone(), ProgramRpcClientSendTransaction),
    );
    program_client
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
        MemcmpEncodedBytes::Base58(String::from_str(base_mint_address).unwrap()),
    ));

    const TARGETMINT_OFFSET: usize = 85;
    let target_mint_memcmp = RpcFilterType::Memcmp(Memcmp::new(
        TARGETMINT_OFFSET, // offset of 'TargetMint'
        MemcmpEncodedBytes::Base58(String::from_str(target_mint_address).unwrap()),
    ));

    rpc_client
        .get_program_accounts_with_config(
            &OPENBOOK,
            RpcProgramAccountsConfig {
                filters: Some(vec![base_mint_memcmp, target_mint_memcmp]),
                account_config: RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    data_slice: None,
                    commitment: None,
                    min_context_slot: None,
                },
                with_context: Some(true),
            },
        )
        .await
        .unwrap()
        .pop()
}
