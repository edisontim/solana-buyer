use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use coerce::actor::{context::ActorContext, Actor};
use eyre::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use tokio::time;

use crate::constants::RUG_AMOUNT;
use crate::utils::get_token_accounts;

use crate::entities::{prelude::Pool as DatabasePool, prelude::*, *};
use sea_orm::*;

pub struct Indexer {
    pub client: Arc<RpcClient>,
    pub database_url: String,
    pub pool_minimum_indexing_time: Duration,
}

impl Indexer {
    pub fn new(
        client: Arc<RpcClient>,
        database_url: String,
        pool_minimum_indexing_time: Duration,
    ) -> Self {
        Self {
            client,
            database_url: database_url.clone(),
            pool_minimum_indexing_time,
        }
    }

    pub async fn record_prices(&self) -> Result<()> {
        let database = Database::connect(self.database_url.clone()).await?;

        loop {
            tokio::time::sleep(time::Duration::from_secs(2)).await;

            let mut accounts = Vec::new();
            let mut target_token_mints = Vec::new();

            let maybe_unrugged_pools = DatabasePool::find()
                .filter(
                    pool::Column::Rugged
                        .eq(false)
                        .and(pool::Column::DoneIndexing.eq(false)),
                )
                .all(&database)
                .await;

            if maybe_unrugged_pools.is_err() {
                tracing::info!(
                    "Err with database when retrieving pools, continuing... {:?}",
                    maybe_unrugged_pools.unwrap()
                );
                continue;
            }

            let unrugged_pools = maybe_unrugged_pools.unwrap();

            for pool in unrugged_pools.iter() {
                let has_been_indexed_for = Duration::from_secs(pool.started_indexing_at as u64);

                if self.pool_minimum_indexing_time >= has_been_indexed_for {
                    let pool_updated = pool::ActiveModel {
                        id: ActiveValue::Set(pool.id),
                        done_indexing: ActiveValue::Set(true),
                        rugged: ActiveValue::unchanged(pool.rugged),
                        started_indexing_at: ActiveValue::unchanged(pool.started_indexing_at),
                        target_token_mint: ActiveValue::unchanged(pool.target_token_mint.clone()),
                        target_token_pool_vault: ActiveValue::unchanged(
                            pool.target_token_pool_vault.clone(),
                        ),
                        sol_pool_vault: ActiveValue::unchanged(pool.sol_pool_vault.clone()),
                    };

                    let _ = pool_updated.update(&database).await;

                    tracing::info!(
                        "Pool {} has been indexed for the required amount of time, removing",
                        pool.target_token_mint
                    );
                    continue;
                }
                accounts.push(Pubkey::from_str(&pool.target_token_pool_vault).unwrap());
                accounts.push(Pubkey::from_str(&pool.sol_pool_vault).unwrap());
                target_token_mints.push(pool.target_token_mint.clone());
            }

            let maybe_token_accounts = get_token_accounts(&self.client, &accounts).await;
            if let Err(e) = maybe_token_accounts {
                tracing::error!("failed to get token accounts: {:?}", e);
                continue;
            }
            let token_accounts = maybe_token_accounts.unwrap();

            let now = SystemTime::now();
            let ts = now.duration_since(UNIX_EPOCH).expect("Time went backwards");

            let mut rugged_pools = Vec::new();
            let mut i: i32 = -2;
            loop {
                i += 2;
                if i as usize >= token_accounts.len() {
                    break;
                }
                let target_token_mint = target_token_mints[i as usize / 2].clone();

                let target_token_liquidity = token_accounts.get(i as usize).unwrap().amount;
                let sol_liquidity = token_accounts.get(i as usize + 1).unwrap().amount;

                if sol_liquidity <= RUG_AMOUNT as u64 {
                    rugged_pools.push(target_token_mint.clone());
                    tracing::info!("Pool {} got RUGGED", target_token_mint);
                    continue;
                }

                let maybe_database_pool = DatabasePool::find()
                    .filter(pool::Column::TargetTokenMint.eq(target_token_mint.to_string()))
                    .one(&database)
                    .await;

                if maybe_database_pool.is_err() {
                    tracing::info!(
                        "Err with database when retrieving pool {}, continuing... {:?}",
                        target_token_mint,
                        maybe_database_pool.unwrap()
                    );
                    continue;
                }
                let database_pool = maybe_database_pool.unwrap();
                if database_pool.is_none() {
                    tracing::info!(
                        "Pool {} wasn't found in database, continuing... {:?}",
                        target_token_mint,
                        database_pool.unwrap()
                    );
                    continue;
                }

                let new_liquidity = liquidity::ActiveModel {
                    ts: ActiveValue::Set(ts.as_secs() as i64),
                    target_token_liquidity: ActiveValue::Set(target_token_liquidity as i64),
                    sol_liquidity: ActiveValue::Set(sol_liquidity as i64),
                    pool_id: ActiveValue::Set(database_pool.unwrap().id as i64),
                    ..Default::default()
                };
                let ret = Liquidity::insert(new_liquidity).exec(&database).await;
                if ret.is_err() {
                    tracing::debug!("Error logging into DB: {:?}", ret.unwrap());
                }
            }

            let maybe_pool_rugged = DatabasePool::find()
                .filter(Condition::any().add(pool::Column::TargetTokenMint.is_in(&rugged_pools)))
                .all(&database)
                .await;

            if maybe_pool_rugged.is_err() {
                tracing::info!(
                    "Err with database when retrieving pools, continuing... {:?}",
                    maybe_pool_rugged.unwrap()
                );
                continue;
            }

            let rugged_pools = maybe_pool_rugged.unwrap();
            for pool in rugged_pools {
                let pool_updated = pool::ActiveModel {
                    id: ActiveValue::unchanged(pool.id),
                    done_indexing: ActiveValue::unchanged(pool.done_indexing),
                    rugged: ActiveValue::Set(true),
                    started_indexing_at: ActiveValue::unchanged(pool.started_indexing_at),
                    target_token_mint: ActiveValue::unchanged(pool.target_token_mint.clone()),
                    target_token_pool_vault: ActiveValue::unchanged(
                        pool.target_token_pool_vault.clone(),
                    ),
                    sol_pool_vault: ActiveValue::unchanged(pool.sol_pool_vault.clone()),
                };

                let _ = pool_updated.update(&database).await;
            }
        }
    }
}

#[async_trait]
impl Actor for Indexer {
    #[tracing::instrument(skip_all)]
    async fn started(&mut self, ctx: &mut ActorContext) {
        tracing::info!("indexer now running");
        let res = self.record_prices().await;
        if res.is_err() {
            tracing::error!("Stopped indexer because of an error: {:?}", res.unwrap());
        }
        ctx.stop(None);
    }
}
