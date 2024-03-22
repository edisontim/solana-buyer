use std::sync::Arc;

use clap::Args;
use coerce::actor::{system::ActorSystem, IntoActor};
use once_cell::sync::Lazy;
use solana_client::nonblocking::rpc_client::RpcClient;
use tokio::sync::Notify;

use crate::{
    actors::{guard::GuardActor, listener::actor::Listener},
    types::ProgramConfig,
};

static NOTIFY: Lazy<Arc<Notify>> = Lazy::new(|| Arc::new(Notify::new()));

#[derive(Debug, Args)]
pub struct ListenSubcommand;

impl ListenSubcommand {
    pub async fn run(self, client: Arc<RpcClient>, config: ProgramConfig) {
        // Init the actor system
        let system = ActorSystem::new();

        let listener = Listener::new(client, config)
            .into_actor(Some("listener".to_string()), &system)
            .await
            .expect("failed to start listener");

        let guard = GuardActor::new(listener, NOTIFY.clone());

        let guard = guard
            .into_actor(Some("guard".to_string()), &system)
            .await
            .expect("failed to start guard");

        // Wait for the notification
        NOTIFY.notified().await;
        guard.stop().await.expect("failed to stop guard");
    }
}
