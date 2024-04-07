#![allow(clippy::blocks_in_conditions)]
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::actors::listener::utils::get_pool_info;
use crate::actors::listener::utils::get_pool_init_infos;
use crate::actors::swapper::actor::Swapper;
use crate::constants::SOL;
use crate::entities::{prelude::*, *};
use crate::message;
use crate::{
    constants::CREATE_POOL_FEE_ACCOUNT_ADDRESS,
    types::ProgramConfig,
    websocket::{LogsSubscribeResponse, WebSocket, WebSocketConfig},
};
use async_trait::async_trait;
use coerce::actor::context::ActorContext;
use coerce::actor::message::Handler;
use coerce::actor::{Actor, ActorId, IntoActorId, LocalActorRef};
use eyre::Result;
use sea_orm::*;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter},
};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;

pub struct Listener {
    config: ProgramConfig,
    client: Arc<RpcClient>,
    max_swappers: u8,
    trade_amount: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct PoolInitTxInfos {
    pub amm_id: Pubkey,
    pub market_id: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
}

#[derive(Debug, Clone)]
struct PoolInitialized(PoolInitTxInfos);
message!(PoolInitialized, Result<(), eyre::Error>);

#[async_trait]
impl Handler<PoolInitialized> for Listener {
    #[tracing::instrument(skip_all, err)]
    async fn handle(
        &mut self,
        message: PoolInitialized,
        ctx: &mut ActorContext,
    ) -> Result<(), eyre::Error> {
        let amount_swappers = ctx.supervised_count();
        if amount_swappers >= self.max_swappers as usize {
            tracing::info!("max swappers reached");
            return Ok(());
        }

        let init_pool_tx_infos = message.0;
        if self.trade_amount.is_none() {
            self.add_pool_to_db(init_pool_tx_infos).await
        } else {
            let swapper = Swapper::from_pool_params(
                Arc::clone(&self.client),
                self.config.clone(),
                init_pool_tx_infos,
                self.trade_amount
                    .expect("Expected a value for trade amount"),
            )
            .await?;
            let id = format!(
                "swapper-{}{}",
                &init_pool_tx_infos.market_id.to_string()[..3],
                &init_pool_tx_infos.amm_id.to_string()[..3],
            );
            tracing::info!(
                "spawned swapper ({}): base_mint {:?} - quote_mint {:?}",
                id,
                init_pool_tx_infos.base_mint,
                init_pool_tx_infos.quote_mint,
            );
            ctx.spawn_deferred(id.into_actor_id(), swapper)?;

            Ok(())
        }
    }
}

#[async_trait]
impl Actor for Listener {
    #[tracing::instrument(skip_all)]
    async fn started(&mut self, ctx: &mut ActorContext) {
        tracing::info!("listener");

        self.listen_and_notify_spawn_swappers(ctx);
    }

    #[tracing::instrument(skip_all, fields(id = %_id))]
    async fn on_child_stopped(&mut self, _id: &ActorId, _ctx: &mut ActorContext) {
        tracing::info!("listener child stopped");
    }
}

impl Listener {
    pub fn new(
        client: Arc<RpcClient>,
        config: ProgramConfig,
        max_swappers: u8,
        trade_amount: Option<f64>,
    ) -> Self {
        Self {
            client,
            config,
            max_swappers,
            trade_amount,
        }
    }

    /// Listen to the logs and notify self to spawn swappers
    /// when the create pool fee account address is mentioned
    /// in the logs.
    ///
    /// # Panic
    ///
    /// Panics if the websocket subscription fails
    pub fn listen_and_notify_spawn_swappers(&self, ctx: &mut ActorContext) {
        let config = self.config.clone();
        let client = Arc::clone(&self.client);
        let self_ref: LocalActorRef<Listener> = ctx.actor_ref().clone();

        tokio::task::spawn(async move { listen_routine(client, self_ref, config).await });
    }

    async fn add_pool_to_db(&self, init_pool_tx_infos: PoolInitTxInfos) -> Result<()> {
        let database = Database::connect(self.config.database_url.clone()).await?;
        let pool_info = get_pool_info(&self.client, init_pool_tx_infos.amm_id).await?;

        let (sol_vault, target_token_vault, target_token_mint) =
            match (pool_info.base_mint, pool_info.quote_mint) {
                (base_mint, quote_mint) if *SOL == base_mint => {
                    (pool_info.base_vault, pool_info.quote_vault, quote_mint)
                }
                (base_mint, quote_mint) if *SOL == quote_mint => {
                    (pool_info.quote_vault, pool_info.base_vault, base_mint)
                }
                _ => {
                    tracing::error!("not adding to indexer: can only trade SOL");
                    return Ok(());
                }
            };

        let now = SystemTime::now();
        let ts = now.duration_since(UNIX_EPOCH).expect("Time went backwards");

        let new_pool = pool::ActiveModel {
            started_indexing_at: ActiveValue::Set(ts.as_secs() as i64),
            target_token_mint: ActiveValue::Set(target_token_mint.to_string()),
            target_token_pool_vault: ActiveValue::Set(target_token_vault.to_string()),
            sol_pool_vault: ActiveValue::Set(sol_vault.to_string()),
            rugged: ActiveValue::Set(false),
            done_indexing: ActiveValue::Set(false),
            ..Default::default()
        };
        let ret = Pool::insert(new_pool).exec(&database).await;
        if ret.is_err() {
            tracing::debug!("Error logging into DB: {:?}", ret.unwrap());
        }
        return Ok(());
    }
}

/// # Panic
///
/// Panics if the websocket subscription fails
async fn listen_routine(
    client: Arc<RpcClient>,
    listener_reference: LocalActorRef<Listener>,
    config: ProgramConfig,
) {
    // Subscribes to any logs that mention the create pool fee account address.
    // Waits for the logs to reach the required commitment.
    let mut ws = WebSocket::create_new_logs_subscription(
        WebSocketConfig {
            num_retries: 5,
            url: config.ws_rpc_url.clone(),
        },
        RpcTransactionLogsFilter::Mentions(vec![CREATE_POOL_FEE_ACCOUNT_ADDRESS.to_string()]),
        RpcTransactionLogsConfig {
            commitment: Some(CommitmentConfig::confirmed()),
        },
    )
    .await
    .expect("failed to create a ws subscription");

    loop {
        let maybe_log = ws.read::<LogsSubscribeResponse>().await;

        if maybe_log.is_err() {
            tracing::debug!("failed to read: {:?}", maybe_log.err());
            continue;
        }

        let log = maybe_log.unwrap();
        let maybe_pool_init_tx_infos = get_pool_init_infos(Arc::clone(&client), log).await;
        if maybe_pool_init_tx_infos.is_err() {
            tracing::debug!(
                "error with log: {}",
                maybe_pool_init_tx_infos.unwrap_err().to_string()
            );
            continue;
        }

        let pool_init_tx_infos = maybe_pool_init_tx_infos.unwrap();
        let _ = listener_reference
            .notify(PoolInitialized(pool_init_tx_infos))
            .inspect_err(|err| tracing::error!("failed to spawn swapper: {:?}", err));
    }
}
