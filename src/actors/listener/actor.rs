#![allow(clippy::blocks_in_conditions)]
use std::sync::Arc;

use async_trait::async_trait;
use coerce::actor::context::ActorContext;
use coerce::actor::message::Handler;
use coerce::actor::{Actor, ActorId, IntoActorId, LocalActorRef};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter},
};
use solana_sdk::commitment_config::CommitmentConfig;

use crate::actors::listener::utils::get_pool_init_infos;
use crate::actors::swapper::actor::{PoolInitTxInfos, Swapper};
use crate::message;
use crate::{
    constants::CREATE_POOL_FEE_ACCOUNT_ADDRESS,
    types::ProgramConfig,
    websocket::{LogsSubscribeResponse, WebSocket, WebSocketConfig},
};

pub struct Listener {
    config: ProgramConfig,
    client: Arc<RpcClient>,
    max_swappers: u8,
    trade_amount: f64,
}

#[derive(Debug, Clone)]
struct SpawnSwapper(PoolInitTxInfos);
message!(SpawnSwapper, Result<(), eyre::Error>);

#[async_trait]
impl Handler<SpawnSwapper> for Listener {
    #[tracing::instrument(skip_all, err)]
    async fn handle(
        &mut self,
        message: SpawnSwapper,
        ctx: &mut ActorContext,
    ) -> Result<(), eyre::Error> {
        let amount_swappers = ctx.supervised_count();
        if amount_swappers >= self.max_swappers as usize {
            tracing::info!("max swappers reached");
            return Ok(());
        }

        let init_pool_tx_infos = message.0;
        let swapper = Swapper::from_pool_params(
            Arc::clone(&self.client),
            self.config.clone(),
            init_pool_tx_infos,
            self.trade_amount,
        )
        .await?;

        let id = format!(
            "swapper-{}{}",
            &init_pool_tx_infos.market_id.to_string()[..6],
            &init_pool_tx_infos.amm_id.to_string()[..6],
        );
        tracing::info!(
            "spawned swapper with id {}, market id {:?}, amm id {:?}, base_mint {:?}, quote_mint {:?}",
            id,
            init_pool_tx_infos.market_id,
            init_pool_tx_infos.amm_id,
            init_pool_tx_infos.base_mint,
            init_pool_tx_infos.quote_mint,
        );

        ctx.spawn_deferred(id.into_actor_id(), swapper)?;

        Ok(())
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
        trade_amount: f64,
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
                "error with log: {:?}",
                maybe_pool_init_tx_infos.unwrap_err()
            );
            continue;
        }

        let pool_init_tx_infos = maybe_pool_init_tx_infos.unwrap();
        let _ = listener_reference
            .notify(SpawnSwapper(pool_init_tx_infos))
            .inspect_err(|err| tracing::error!("failed to spawn swapper: {:?}", err));
    }
}
