use std::sync::Arc;

use async_trait::async_trait;
use coerce::actor::context::ActorContext;
use coerce::actor::message::Handler;
use coerce::actor::{Actor, ActorId, IntoActorId, LocalActorRef};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter},
};
use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel::Confirmed};
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
    amount_swappers: u16,
}

#[derive(Debug, Clone)]
struct SpawnSwapper(Pubkey, Pubkey);
message!(SpawnSwapper, Result<(), eyre::Error>);

#[async_trait]
impl Handler<SpawnSwapper> for Listener {
    // #[tracing::instrument(skip_all, err)]
    async fn handle(
        &mut self,
        message: SpawnSwapper,
        ctx: &mut ActorContext,
    ) -> Result<(), eyre::Error> {
        let swapper = Swapper::from_pool_params(
            Arc::clone(&self.client),
            self.config.clone(),
            message.0,
            message.1,
        )
        .await?;

        let swapper_id = self.amount_swappers;
        ctx.spawn_deferred(format!("swapper-{}", swapper_id).into_actor_id(), swapper)?;

        tracing::info!(
            "spawned swapper with id {}, market id {:?} and amm id {:?}",
            swapper_id,
            message.0,
            message.1
        );

        self.amount_swappers += 1;
        Ok(())
    }
}

/// Implements the actor trait for the listener
#[async_trait]
impl Actor for Listener {
    #[tracing::instrument(skip_all)]
    async fn started(&mut self, ctx: &mut ActorContext) {
        tracing::info!("listener");

        // Listen to the current pending transactions
        self.listen(ctx);
    }

    #[tracing::instrument(skip_all, fields(id = %_id))]
    async fn on_child_stopped(&mut self, _id: &ActorId, _ctx: &mut ActorContext) {
        tracing::info!("listener child stopped");

        self.amount_swappers -= 1;
    }
}

impl Listener {
    /// Create a new listener from the client and config
    pub fn new(client: Arc<RpcClient>, config: ProgramConfig) -> Self {
        Self {
            client,
            config,
            amount_swappers: 0,
        }
    }

    /// Listen to the logs and swap
    ///
    /// # Panic
    ///
    /// Panics if the websocket subscription fails
    pub fn listen(&self, ctx: &mut ActorContext) {
        let config = self.config.clone();
        let client = Arc::clone(&self.client);
        let self_ref: LocalActorRef<Listener> = ctx.actor_ref().clone();

        tokio::task::spawn(async move {
            // Start the websocket. Currently uses 5 retries.
            // Subscribes to any logs that mention the create pool fee account address.
            // Waits for the logs to be confirmed.
            let mut ws = WebSocket::create_new_logs_subscription(
                WebSocketConfig {
                    num_retries: 5,
                    url: config.ws_rpc_url.clone(),
                },
                RpcTransactionLogsFilter::Mentions(vec![
                    CREATE_POOL_FEE_ACCOUNT_ADDRESS.to_string()
                ]),
                RpcTransactionLogsConfig {
                    commitment: Some(CommitmentConfig {
                        commitment: Confirmed,
                    }),
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
                let maybe_market_and_amm_id =
                    get_market_id_and_amm_id(Arc::clone(&client), log).await;
                if maybe_market_and_amm_id.is_err() {
                    tracing::debug!("error with log: {:?}", maybe_market_and_amm_id.unwrap_err());
                    continue;
                }

                let (market_id, amm_id) = maybe_market_and_amm_id.unwrap();

                let _ = self_ref
                    .notify(SpawnSwapper(amm_id, market_id))
                    .inspect_err(|err| tracing::error!("failed to spawn swapper: {:?}", err));
            }
        });
    }
}
