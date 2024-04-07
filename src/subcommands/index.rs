use std::{sync::Arc, time::Duration};

use clap::Args;
use coerce::actor::{system::ActorSystem, IntoActor};
use once_cell::sync::Lazy;
use solana_client::nonblocking::rpc_client::RpcClient;
use tokio::sync::Notify;

use crate::{
    actors::{guard::GuardActor, indexer::actor::Indexer, listener::actor::Listener},
    types::ProgramConfig,
};

static NOTIFY: Lazy<Arc<Notify>> = Lazy::new(|| Arc::new(Notify::new()));

const SECONDS_PER_DAY: u64 = 60 * 60 * 24;

#[derive(Debug, Args)]
pub struct IndexSubcommand {
    /// Input max indexers
    #[arg(short, long)]
    #[arg(default_value = "1")]
    max_indexers: u8,
    /// Input number of days to index each pool
    #[arg(short, long)]
    #[arg(default_value = "7")]
    indexing_times: u8,
}

impl IndexSubcommand {
    pub async fn run(self, client: Arc<RpcClient>, config: ProgramConfig) {
        let system = ActorSystem::new();

        let database_url = config.database_url.clone();
        let listener = Listener::new(client.clone(), config, self.max_indexers, None)
            .into_actor(Some("listener".to_string()), &system)
            .await
            .expect("failed to start listener");

        let _indexer = Indexer::new(
            client,
            database_url,
            Duration::from_secs(self.indexing_times as u64 * SECONDS_PER_DAY),
        )
        .into_actor(Some("indexer".to_string()), &system)
        .await
        .expect("failed to start indexer");

        let guard = GuardActor::new(listener, NOTIFY.clone());

        let guard = guard
            .into_actor(Some("guard".to_string()), &system)
            .await
            .expect("failed to start guard");

        NOTIFY.notified().await;
        guard.stop().await.expect("failed to stop guard");
    }
}
