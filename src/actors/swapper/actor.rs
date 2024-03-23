use std::sync::Arc;

use async_trait::async_trait;
use coerce::actor::context::ActorContext;
use coerce::actor::Actor;
use eyre::Result;
use raydium_contract_instructions::amm_instruction as amm;
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig, instruction::Instruction, pubkey::Pubkey,
    signature::Keypair, signer::Signer, transaction::Transaction,
};
use spl_associated_token_account::instruction::create_associated_token_account;

use crate::{
    constants::{AMM_V4, RAYDIUM_AUTHORITY_V4, TOKEN_PROGRAM},
    types::{MarketInfo, PoolInfo, ProgramConfig},
    utils::{
        get_associated_authority, get_pool_and_market_info, get_prio_fee_instructions,
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

#[async_trait]
impl Actor for Swapper {
    #[tracing::instrument(skip_all)]
    async fn started(&mut self, _ctx: &mut ActorContext) {
        tracing::info!("Swapper now running!");
    }
}

impl Swapper {
    pub async fn new(
        client: Arc<RpcClient>,
        market_id: Pubkey,
        config: ProgramConfig,
    ) -> Result<Self> {
        let amm_id = Pubkey::find_program_address(
            &[AMM_V4.as_ref(), market_id.as_ref(), b"amm_associated_seed"],
            &AMM_V4,
        )
        .0;

        Swapper::from_pool_params(client, config, amm_id, market_id).await
    }

    pub async fn from_pool_params(
        client: Arc<RpcClient>,
        config: ProgramConfig,
        amm_id: Pubkey,
        market_id: Pubkey,
    ) -> Result<Self> {
        let user_keypair = Keypair::from_base58_string(&config.buyer_private_key);

        let (pool_info, market_info) =
            get_pool_and_market_info(&client, &amm_id, &market_id).await?;

        let associated_authority =
            get_associated_authority(pool_info.market_program_id, pool_info.market_id).unwrap();

        let (user_base_token_account, user_quote_token_account, account_to_create) =
            get_user_token_accounts(
                &client,
                &user_keypair,
                pool_info.base_mint,
                pool_info.quote_mint,
            )
            .await
            .unwrap();

        Ok(Self {
            client,
            user_keypair,
            pool_info,
            amm_id,
            user_base_token_account,
            user_quote_token_account,
            market_info,
            associated_authority,
            account_to_create,
        })
    }

    pub async fn swap(&self, in_token: &Pubkey, amount_in: f64) {
        let mut instructions = vec![];
        let (user_out_token_account, user_in_token_account) =
            if *in_token == self.pool_info.base_mint {
                (self.user_quote_token_account, self.user_base_token_account)
            } else {
                (self.user_base_token_account, self.user_quote_token_account)
            };

        let (compute_unit_limit_instruction, compute_unit_price_instruction) =
            get_prio_fee_instructions();
        instructions.push(compute_unit_limit_instruction);
        instructions.push(compute_unit_price_instruction);

        if self.account_to_create.is_some() {
            let associated_token_account_create_instruction = create_associated_token_account(
                &self.user_keypair.pubkey(),
                &self.user_keypair.pubkey(),
                &self.account_to_create.unwrap(),
                &TOKEN_PROGRAM,
            );
            instructions.push(associated_token_account_create_instruction);
        }

        let base_vault_balance_info = self
            .client
            .get_token_account_balance_with_commitment(
                &self.pool_info.base_vault,
                CommitmentConfig::confirmed(),
            )
            .await
            .unwrap()
            .value;

        let quote_vault_balance_info = self
            .client
            .get_token_account_balance_with_commitment(
                &self.pool_info.quote_vault,
                CommitmentConfig::confirmed(),
            )
            .await
            .unwrap()
            .value;

        let in_token_balance = self
            .client
            .get_token_account_balance(&user_in_token_account)
            .await
            .unwrap()
            .amount
            .parse::<f64>()
            .unwrap();

        let amount_in = if self.pool_info.base_mint == *in_token {
            Swapper::get_swap_amounts(
                amount_in,
                base_vault_balance_info.decimals,
                in_token_balance,
            )
        } else {
            Swapper::get_swap_amounts(
                amount_in,
                quote_vault_balance_info.decimals,
                in_token_balance,
            )
        };
        tracing::debug!(
            "user_in_token_account {} user_out_token_account {}",
            user_in_token_account,
            user_out_token_account
        );
        tracing::debug!("swap base in: {} for minimum 0 out", amount_in);
        let instruction = self.build_swap_base_in_instruction(
            amount_in,
            0.,
            user_in_token_account,
            user_out_token_account,
        );

        instructions.push(instruction);
        self.sign_and_send_instructions(instructions).await;
    }

    fn build_swap_base_in_instruction(
        &self,
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

    fn get_swap_amounts(mut amount_in: f64, in_decimals: u8, in_token_balance: f64) -> f64 {
        if amount_in == 0. {
            amount_in = in_token_balance;
        } else {
            amount_in *= 10_f64.powi(in_decimals.into());
        }
        amount_in
    }

    async fn sign_and_send_instructions(&self, instructions: Vec<Instruction>) {
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
            tracing::error!("Failed to send transaction: {:?}", e);
        };
    }
}
