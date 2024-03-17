use raydium_contract_instructions::amm_instruction as amm;
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig, instruction::Instruction, pubkey::Pubkey,
    signature::Keypair, signer::Signer, transaction::Transaction,
};
use spl_associated_token_account::instruction::create_associated_token_account;
use std::{str::FromStr, sync::Arc};

use crate::{
    constants::{AMM_V4, RAYDIUM_AUTHORITY_V4, TOKEN_PROGRAM},
    types::{Config, MarketInfo, PoolInfo},
    utils::{
        get_associated_authority, get_market_info, get_pool_info, get_prio_fee_instructions,
        get_user_token_accounts,
    },
};

pub struct Swapper {
    client: Arc<RpcClient>,
    user_keypair: Keypair,
    pool_info: PoolInfo,
    market_info: MarketInfo,
    amm_id: Pubkey,
    user_base_token_account: Pubkey,
    user_quote_token_account: Pubkey,
    associated_authority: Pubkey,
    account_to_create: Option<Pubkey>,
}

impl Swapper {
    pub async fn new(market_id: Pubkey, config: Config) -> Self {
        let client: Arc<RpcClient> = Arc::new(RpcClient::new(
            String::from_str(&config.http_rpc_url).unwrap(),
        ));
        let user_keypair = Keypair::from_base58_string(&config.buyer_private_key);

        let amm_id = Pubkey::find_program_address(
            &[AMM_V4.as_ref(), market_id.as_ref(), b"amm_associated_seed"],
            &AMM_V4,
        )
        .0;

        let pool_info = get_pool_info(&client, &amm_id).await;

        let associated_authority =
            get_associated_authority(pool_info.market_program_id, pool_info.market_id).unwrap();

        let market_info = get_market_info(&client, &pool_info.market_id).await;

        let (user_base_token_account, user_quote_token_account, account_to_create) =
            get_user_token_accounts(
                &client,
                &user_keypair,
                pool_info.base_mint,
                pool_info.quote_mint,
            )
            .await
            .unwrap();

        Self {
            client,
            user_keypair,
            pool_info,
            amm_id,
            user_base_token_account,
            user_quote_token_account,
            market_info,
            associated_authority,
            account_to_create,
        }
    }

    pub async fn swap(self: &Self, in_token: &Pubkey, mut amount_in: f64, slippage: f64) {
        let mut instructions = vec![];
        let (out_token, user_out_token_account, user_in_token_account) =
            if *in_token == self.pool_info.base_mint {
                (
                    self.pool_info.quote_mint,
                    self.user_quote_token_account,
                    self.user_base_token_account,
                )
            } else {
                (
                    self.pool_info.base_mint,
                    self.user_base_token_account,
                    self.user_quote_token_account,
                )
            };

        let (compute_unit_limit_instruction, compute_unit_price_instruction) =
            get_prio_fee_instructions(&self.client).await;
        instructions.push(compute_unit_limit_instruction);
        instructions.push(compute_unit_price_instruction);

        if self.account_to_create.is_some() {
            let associated_token_account_create_instruction = create_associated_token_account(
                &self.user_keypair.pubkey(),
                &self.user_keypair.pubkey(),
                &out_token,
                &TOKEN_PROGRAM,
            );
            instructions.push(associated_token_account_create_instruction);
        }

        let base_vault_balance_info = self
            .client
            .get_token_account_balance(&self.pool_info.base_vault)
            .await
            .unwrap();

        let quote_vault_balance_info = self
            .client
            .get_token_account_balance(&self.pool_info.quote_vault)
            .await
            .unwrap();

        let in_token_balance = self
            .client
            .get_token_account_balance(&user_in_token_account)
            .await
            .unwrap()
            .amount
            .parse::<f64>()
            .unwrap();

        let base_vault_balance = base_vault_balance_info.amount.parse::<f64>().unwrap();
        let quote_vault_balance = quote_vault_balance_info.amount.parse::<f64>().unwrap();

        if self.pool_info.base_mint == *in_token {
            if amount_in == -1.0 {
                amount_in = in_token_balance as f64;
            }
            let amount_out = Swapper::get_amount_out(
                amount_in,
                slippage,
                base_vault_balance,
                quote_vault_balance,
                base_vault_balance_info.decimals,
            );
            println!("trading {} in for minimum {} out", amount_in, amount_out);
            debug_assert!(self.pool_info.quote_mint == out_token);
            instructions.push(self.build_swap_base_in_instruction(
                amount_in,
                amount_out,
                user_in_token_account,
                user_out_token_account,
            ));
        } else {
            if amount_in == -1.0 {
                amount_in = in_token_balance as f64;
            }
            let amount_out = Swapper::get_amount_out(
                amount_in,
                slippage,
                quote_vault_balance,
                base_vault_balance,
                quote_vault_balance_info.decimals,
            );
            println!("trading {} in for minimum {} out", amount_in, amount_out);
            instructions.push(self.build_swap_base_out_instruction(
                amount_in,
                amount_out,
                user_in_token_account,
                user_out_token_account,
            ));
        }

        self.sign_and_send_instructions(instructions).await;
    }

    fn build_swap_base_in_instruction(
        self: &Self,
        amount_in: f64,
        amount_out: f64,
        user_in_token_account: Pubkey,
        user_out_token_account: Pubkey,
    ) -> Instruction {
        amm::swap_base_in(
            &amm::ID,
            &self.amm_id,
            &RAYDIUM_AUTHORITY_V4,
            &self.pool_info.open_orders,
            &self.pool_info.target_orders,
            &self.pool_info.base_vault,
            &self.pool_info.quote_vault,
            &self.pool_info.market_program_id,
            &self.pool_info.market_id,
            &self.market_info.bids,
            &self.market_info.asks,
            &self.market_info.event_queue,
            &self.market_info.base_vault,
            &self.market_info.quote_vault,
            &self.associated_authority,
            &user_in_token_account,
            &user_out_token_account,
            &self.user_keypair.pubkey(),
            amount_in as u64,
            amount_out as u64,
        )
        .unwrap()
    }

    fn build_swap_base_out_instruction(
        self: &Self,
        amount_in: f64,
        amount_out: f64,
        user_in_token_account: Pubkey,
        user_out_token_account: Pubkey,
    ) -> Instruction {
        amm::swap_base_out(
            &amm::ID,
            &self.amm_id,
            &RAYDIUM_AUTHORITY_V4,
            &self.pool_info.open_orders,
            &self.pool_info.target_orders,
            &self.pool_info.base_vault,
            &self.pool_info.quote_vault,
            &self.pool_info.market_program_id,
            &self.pool_info.market_id,
            &self.market_info.bids,
            &self.market_info.asks,
            &self.market_info.event_queue,
            &self.market_info.base_vault,
            &self.market_info.quote_vault,
            &self.associated_authority,
            &user_in_token_account,
            &user_out_token_account,
            &self.user_keypair.pubkey(),
            amount_in as u64,
            amount_out as u64,
        )
        .unwrap()
    }

    fn get_amount_out(
        amount_in: f64,
        slippage: f64,
        in_pool_liquidity: f64,
        out_pool_liquidity: f64,
        in_decimals: u8,
    ) -> f64 {
        let price_per_in_token: f64 = out_pool_liquidity / in_pool_liquidity;
        let mut amount_in = amount_in;
        amount_in = amount_in * 10_f64.powi(in_decimals.into());
        let amount_out: f64 = Into::<f64>::into(amount_in) * price_per_in_token;
        let amount_out: f64 = amount_out * (100.0 - slippage) / 100.0;
        amount_out
    }

    async fn sign_and_send_instructions(self: &Self, instructions: Vec<Instruction>) {
        let recent_blockhash = self
            .client
            .get_latest_blockhash_with_commitment(solana_sdk::commitment_config::CommitmentConfig {
                commitment: solana_sdk::commitment_config::CommitmentLevel::Finalized,
            })
            .await
            .unwrap()
            .0;

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.user_keypair.pubkey()),
            &vec![&self.user_keypair],
            recent_blockhash,
        );

        if let Err(e) = self
            .client
            .send_and_confirm_transaction_with_spinner_and_config(
                &transaction,
                CommitmentConfig::finalized(),
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
}
