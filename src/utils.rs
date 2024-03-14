use solana_account_decoder::UiAccountEncoding;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, pubkey::Pubkey,
    signature::Keypair, signer::Signer, transaction::Transaction,
};

use crate::{
    constants::OPENBOOK,
    types::{MarketInfo, PoolInfo, SplMint},
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

pub async fn get_prio_fee(client: &RpcClient) -> (Instruction, Instruction) {
    let recent_prio_fees = client.get_recent_prioritization_fees(&[]).await.unwrap();
    let mut non_null_occurences = 0;
    let total_fees: u64 = recent_prio_fees.iter().fold(0, |val, &x| {
        if x.prioritization_fee != 0 {
            non_null_occurences += 1;
            val + x.prioritization_fee
        } else {
            val
        }
    });
    let mut average_prio_fee = 0;
    println!("non_null_occurences {}", non_null_occurences);
    if non_null_occurences != 0 {
        average_prio_fee = total_fees / non_null_occurences;
    }
    if average_prio_fee < 6000 {
        average_prio_fee = 6000;
    }
    println!("avg prio fee {:?}", average_prio_fee);
    let compute_unit_limit_instruction = ComputeBudgetInstruction::set_compute_unit_limit(70_000);
    let compute_unit_price_instruction =
        ComputeBudgetInstruction::set_compute_unit_price(average_prio_fee);
    return (
        compute_unit_limit_instruction,
        compute_unit_price_instruction,
    );
}

pub fn get_associated_authority(program_id: Pubkey, market_id: Pubkey) -> Option<Pubkey> {
    let seeds = market_id.to_bytes();
    let mut nonce: u8 = 0;
    while nonce < 100 {
        let seeds: Vec<u8> = seeds.to_vec();
        let nonce_as_array: Vec<u8> = vec![nonce];
        let padding: Vec<u8> = vec![0, 0, 0, 0, 0, 0, 0];
        let key = Pubkey::create_program_address(&[&seeds, &nonce_as_array, &padding], &program_id);
        if key.is_err() {
            nonce += 1;
            continue;
        } else {
            return Some(key.unwrap());
        }
    }
    return None;
}

pub async fn get_pool_info(client: &RpcClient, amm_id: &Pubkey) -> PoolInfo {
    let pool_info = client.get_account_data(&amm_id).await.unwrap();
    let pool_info = PoolInfo::deserialize(&mut &pool_info[..]).unwrap();
    return pool_info;
}

pub async fn get_market_info(client: &RpcClient, market_id: &Pubkey) -> MarketInfo {
    let market_info = client.get_account_data(&market_id).await.unwrap();
    let market_info = MarketInfo::deserialize(&mut &market_info[..]).unwrap();
    return market_info;
}

pub async fn get_lp_mint_info(client: &RpcClient, lp_mint: &Pubkey) -> SplMint {
    let lp_mint_info = client.get_account_data(&lp_mint).await.unwrap();
    let lp_mint_info = SplMint::deserialize(&mut &lp_mint_info[..]).unwrap();
    return lp_mint_info;
}

pub async fn get_user_accounts(
    client: &Arc<RpcClient>,
    user_keypair: &Keypair,
    in_token: Pubkey,
    out_token: Pubkey,
    amount_in: f64,
) -> Result<(Pubkey, Pubkey), eyre::Error> {
    let user = user_keypair.pubkey();

    let program_client = get_program_rpc(Arc::clone(&client));
    let in_token_client = Token::new(
        Arc::clone(&program_client),
        &spl_token::ID,
        &in_token,
        None,
        Arc::new(Keypair::from_bytes(&user_keypair.to_bytes()).expect("failed to copy keypair")),
    );
    let out_token_client = Token::new(
        Arc::clone(&program_client),
        &spl_token::ID,
        &out_token,
        None,
        Arc::new(Keypair::from_bytes(&user_keypair.to_bytes()).expect("failed to copy keypair")),
    );

    let user_in_token_account = in_token_client.get_associated_token_address(&user);
    match in_token_client
        .get_account_info(&user_in_token_account)
        .await
    {
        Ok(_) => println!("User's ATA for input tokens exists. Skipping creation.."),
        Err(TokenError::AccountNotFound) | Err(TokenError::AccountInvalidOwner) => {
            println!("User's input-tokens ATA does not exist. Creating..");
            in_token_client
                .create_associated_token_account(&user)
                .await?;
        }
        Err(error) => println!("Error retrieving user's input-tokens ATA: {}", error),
    };

    let user_in_acct = in_token_client
        .get_account_info(&user_in_token_account)
        .await?;

    // Create the user's out-token ATA if it doesn't exist.
    let user_out_token_account = out_token_client.get_associated_token_address(&user);

    let in_token_decimals = in_token_client.get_mint_info().await?.base.decimals;

    let amount_in = (amount_in * (10_u64.pow(in_token_decimals.into()) as f64)) as u64;

    // TODO: If input tokens is the native mint(wSOL) and the balance is inadequate, attempt to
    // convert SOL to wSOL.
    let balance = user_in_acct.base.amount;
    if in_token_client.is_native() && balance < amount_in {
        let transfer_amt = amount_in - balance;
        let blockhash = client.get_latest_blockhash().await?;
        let transfer_instruction =
            solana_sdk::system_instruction::transfer(&user, &user_in_token_account, transfer_amt);
        let sync_instruction =
            spl_token::instruction::sync_native(&spl_token::ID, &user_in_token_account)?;
        let (compute_unit_limit_instruction, compute_unit_price_instruction) =
            get_prio_fee(client).await;

        let tx = Transaction::new_signed_with_payer(
            &[
                compute_unit_limit_instruction,
                compute_unit_price_instruction,
                transfer_instruction,
                sync_instruction,
            ],
            Some(&user),
            &[&user_keypair],
            blockhash,
        );
        client
            .send_and_confirm_transaction_with_spinner(&tx)
            .await
            .unwrap();
    }
    let balance = user_in_acct.base.amount;
    println!("User input-tokens ATA balance={}", balance);

    return Ok((user_in_token_account, user_out_token_account));
}

fn get_program_rpc(rpc: Arc<RpcClient>) -> Arc<dyn ProgramClient<ProgramRpcClientSendTransaction>> {
    let program_client: Arc<dyn ProgramClient<ProgramRpcClientSendTransaction>> = Arc::new(
        ProgramRpcClient::new(rpc.clone(), ProgramRpcClientSendTransaction),
    );
    program_client
}

/// Fetches the marketID of the pool
pub async fn get_market_id(
    rpc_client: &RpcClient,
    base_mint_address: &str,
    target_mint_address: &str,
) -> Pubkey {
    let candidate_market_id =
        get_candidate_market_id(rpc_client, base_mint_address, target_mint_address).await;
    let market_id = if candidate_market_id.is_none() {
        get_candidate_market_id(&rpc_client, target_mint_address, base_mint_address)
            .await
            .unwrap()
            .0
    } else {
        candidate_market_id.unwrap().0
    };
    return market_id;
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

    let program_accounts = rpc_client
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
        .pop();
    return program_accounts;
}
