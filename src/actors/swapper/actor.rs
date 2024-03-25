use std::sync::Arc;

use async_trait::async_trait;
use coerce::actor::context::ActorContext;
use coerce::actor::Actor;
use eyre::Result;
use raydium_contract_instructions::amm_instruction as amm;
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig};
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    instruction::Instruction,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};
use spl_associated_token_account::instruction::create_associated_token_account;

use crate::{
    constants::{AMM_V4, MAX_LIQUIDITY, MIN_LIQUIDITY, RAYDIUM_AUTHORITY_V4, SOL, TOKEN_PROGRAM},
    types::{MarketInfo, PoolInfo, ProgramConfig},
    utils::{
        get_accounts_for_swap, get_associated_authority, get_pool_and_market_info,
        get_prio_fee_instructions, get_token_account,
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
    trade_amount: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct PoolInitTxInfos {
    pub amm_id: Pubkey,
    pub market_id: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
}

#[async_trait]
impl Actor for Swapper {
    #[tracing::instrument(skip_all)]
    async fn started(&mut self, ctx: &mut ActorContext) {
        tracing::info!("swapper now running");

        let sol_vault = match (self.pool_info.base_mint, self.pool_info.quote_mint) {
            (base, _) if *SOL == base => self.pool_info.base_vault,
            (_, quote) if *SOL == quote => self.pool_info.quote_vault,
            _ => {
                tracing::error!("stopping swapper: can only trade SOL");
                ctx.stop(None);
                return;
            }
        };
        tracing::info!("solana vault: {}", sol_vault);

        let maybe_vault_sol_account = get_token_account(&self.client, &sol_vault).await;
        if maybe_vault_sol_account.is_err() {
            tracing::error!(
                "stopping swapper: failed to get token account: {:?}",
                maybe_vault_sol_account.err()
            );
            ctx.stop(None);
            return;
        }

        let vault_sol_token_account = maybe_vault_sol_account.unwrap();
        if vault_sol_token_account.amount < *MIN_LIQUIDITY
            || vault_sol_token_account.amount > *MAX_LIQUIDITY
        {
            tracing::warn!(
                "stopping swapper: liquidity not in bound to swap: {}",
                vault_sol_token_account.amount
            );
            ctx.stop(None);
            return;
        }

        // We await here because we don't want the actor to do
        // anything else until the swap is complete.
        self.swap(&SOL, self.trade_amount).await;

        // Then we can kill the swapper
        tracing::info!("stopping swapper after swap");
        ctx.stop(None);
    }
}

impl Swapper {
    pub async fn new(
        client: Arc<RpcClient>,
        config: ProgramConfig,
        market_id: Pubkey,
        trade_amount: f64,
    ) -> Result<Self> {
        let amm_id = Pubkey::find_program_address(
            &[AMM_V4.as_ref(), market_id.as_ref(), b"amm_associated_seed"],
            &AMM_V4,
        )
        .0;
        let (pool_info, _) = get_pool_and_market_info(&client, &amm_id, &market_id).await?;

        Swapper::from_pool_params(
            client,
            config,
            PoolInitTxInfos {
                amm_id,
                market_id,
                base_mint: pool_info.base_mint,
                quote_mint: pool_info.quote_mint,
            },
            trade_amount,
        )
        .await
    }

    pub async fn from_pool_params(
        client: Arc<RpcClient>,
        config: ProgramConfig,
        pool_init_tx_infos: PoolInitTxInfos,
        trade_amount: f64,
    ) -> Result<Self> {
        let user_keypair = Keypair::from_base58_string(&config.buyer_private_key);

        let (pool_info, market_info, user_token_accounts) =
            get_accounts_for_swap(&client, &user_keypair, pool_init_tx_infos).await?;

        let associated_authority =
            get_associated_authority(pool_info.market_program_id, pool_info.market_id).unwrap();

        Ok(Self {
            client,
            user_keypair,
            pool_info,
            amm_id: pool_init_tx_infos.amm_id,
            user_base_token_account: user_token_accounts.user_base_token_account,
            user_quote_token_account: user_token_accounts.user_quote_token_account,
            market_info,
            associated_authority,
            account_to_create: user_token_accounts.account_to_create,
            trade_amount,
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

        let amount_in = if self.pool_info.base_mint == *in_token {
            amount_in * 10_f64.powi(self.pool_info.base_decimal.try_into().unwrap())
        } else {
            amount_in * 10_f64.powi(self.pool_info.quote_decimal.try_into().unwrap())
        };
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
                CommitmentConfig::confirmed(),
                RpcSendTransactionConfig {
                    skip_preflight: true,
                    preflight_commitment: Some(CommitmentLevel::Processed),
                    ..RpcSendTransactionConfig::default()
                },
            )
            .await
        {
            tracing::error!("failed to send transaction: {:?}", e);
        };
    }
}
