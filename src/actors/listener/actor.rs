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
use solana_sdk::pubkey::Pubkey;

use crate::actors::listener::utils::get_market_id_and_amm_id;
use crate::actors::swapper::actor::Swapper;
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
struct SpawnSwapper(Pubkey, Pubkey);
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
            tracing::debug!("max swappers reached");
            return Ok(());
        }

        let swapper = Swapper::from_pool_params(
            Arc::clone(&self.client),
            self.config.clone(),
            message.0,
            message.1,
            self.trade_amount,
        )
        .await?;

        let id = format!(
            "swapper-{}{}",
            &message.0.to_string()[..6],
            &message.1.to_string()[..6]
        );
        tracing::info!(
            "spawned swapper with id {}, market id {:?} and amm id {:?}",
            id,
            message.0,
            message.1
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
    .expect("failed to create a ws subscription");

    loop {
        let maybe_log = ws.read::<LogsSubscribeResponse>();

        if maybe_log.is_err() {
            tracing::debug!("failed to read: {:?}", maybe_log.err());
            continue;
        }

        let log = maybe_log.unwrap();
        let maybe_market_and_amm_id = get_market_id_and_amm_id(Arc::clone(&client), log).await;
        if maybe_market_and_amm_id.is_err() {
            tracing::debug!("error with log: {:?}", maybe_market_and_amm_id.unwrap_err());
            continue;
        }

        let (market_id, amm_id) = maybe_market_and_amm_id.unwrap();

        let _ = listener_reference
            .notify(SpawnSwapper(amm_id, market_id))
            .inspect_err(|err| tracing::error!("failed to spawn swapper: {:?}", err));
    }
}
