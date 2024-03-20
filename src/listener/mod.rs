use std::str::FromStr;
use std::sync::Arc;

use crate::constants::WSOL_ADDRESS;
use crate::swapper::Swapper;
use crate::utils::get_transaction_from_signature;
use crate::{
    constants::CREATE_POOL_FEE_ACCOUNT_ADDRESS,
    types::ProgramConfig,
    websocket::{LogsSubscribeResponse, WebSocket, WebSocketConfig},
};
use eyre::eyre;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter},
};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel::Confirmed},
    signature::Signature,
};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiMessage, UiTransactionEncoding,
};

pub struct Listener {
    config: ProgramConfig,
    client: Arc<RpcClient>,
}

impl Listener {
    pub fn from_config(client: Arc<RpcClient>, config: ProgramConfig) -> Self {
        Self { client, config }
    }
    pub async fn listen(self: Self) {
        let mut ws = WebSocket::create_new_logs_subscription(
            WebSocketConfig {
                num_retries: 5,
                url: self.config.ws_rpc_url.clone(),
            },
            RpcTransactionLogsFilter::Mentions(vec![String::from_str(
                CREATE_POOL_FEE_ACCOUNT_ADDRESS,
            )
            .unwrap()]),
            RpcTransactionLogsConfig {
                commitment: Some(CommitmentConfig {
                    commitment: Confirmed,
                }),
            },
        )
        .expect("Failed to create a ws subscription");

        let (market_id, amm_id) = loop {
            let potential_log = ws.read::<LogsSubscribeResponse>();

            if potential_log.is_err() {
                log::debug!("Failed to read: {:?}", potential_log.err());
                continue;
            }

            let potential_values = self
                .get_market_id_and_amm_id_from_subscribe_logs(potential_log.unwrap())
                .await;
            if potential_values.is_err() {
                log::debug!("error with log: {:?}", potential_values.unwrap_err());
                continue;
            }

            let (market_id, amm_id) = potential_values.unwrap();
            break (market_id, amm_id);
        };

        log::debug!(
            "Initializing swapper with market_id {:?} and amm_id {:?}",
            market_id,
            amm_id
        );

        self.launch_swapper(amm_id, market_id).await;
    }

    async fn launch_swapper(self, amm_id: Pubkey, market_id: Pubkey) {
        let swapper = Swapper::new_from_pool_initialization_params(
            self.client,
            self.config,
            amm_id,
            market_id,
        )
        .await;
        swapper
            .swap(&Pubkey::from_str(WSOL_ADDRESS).unwrap(), 0.001)
            .await;
    }

    async fn get_market_id_and_amm_id_from_subscribe_logs(
        &self,
        log: LogsSubscribeResponse,
    ) -> Result<(Pubkey, Pubkey), eyre::Error> {
        if log.params.result.value.err.is_some() {
            return Err(eyre!("Received transaction is a reverted tx"));
        }

        let signature = Self::get_transaction_signature(log)?;

        let pool_creation_tx = get_transaction_from_signature(
            &self.client,
            signature,
            RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Json),
                max_supported_transaction_version: Some(0),
                commitment: Some(CommitmentConfig::confirmed()),
            },
        )
        .await?;

        let account_keys = Self::get_account_keys_from_transaction(pool_creation_tx)?;

        Self::get_market_id_amm_id_from_account_keys(account_keys)
    }

    fn get_transaction_signature(log: LogsSubscribeResponse) -> Result<Signature, eyre::Error> {
        let signature = log.params.result.value.signature;
        log::debug!("signature {:?}", signature);

        let signature = Signature::from_str(&signature);
        if signature.is_err() {
            return Err(eyre!(
                "Failed to create signature object from signature received: {:?}",
                signature.err()
            ));
        }
        return Ok(signature.unwrap());
    }

    fn get_account_keys_from_transaction(
        tx: EncodedConfirmedTransactionWithStatusMeta,
    ) -> Result<Vec<String>, eyre::Error> {
        let ui_message = match tx.transaction.transaction {
            EncodedTransaction::Json(val) => val.message,
            _ => {
                return Err(eyre!("Unexpected format!!"));
            }
        };

        let account_keys = match ui_message {
            UiMessage::Parsed(msg_parsed) => msg_parsed
                .account_keys
                .iter()
                .map(|account_key| account_key.pubkey.to_owned())
                .collect(),
            UiMessage::Raw(msg_raw) => msg_raw.account_keys,
        };

        Ok(account_keys)
    }

    fn get_market_id_amm_id_from_account_keys(
        mut account_keys: Vec<String>,
    ) -> Result<(Pubkey, Pubkey), eyre::Error> {
        let market_id = account_keys.pop().unwrap();
        let market_id = Pubkey::from_str(&market_id);
        if market_id.is_err() {
            return Err(eyre!(
                "Invalid market_id as public key: {:?}",
                market_id.err()
            ));
        }

        let amm_id = account_keys.remove(2);
        let amm_id = Pubkey::from_str(&amm_id);
        if market_id.is_err() {
            return Err(eyre!("Invalid amm_id as public key: {:?}", amm_id.err()));
        }

        Ok((market_id.unwrap(), amm_id.unwrap()))
    }
}
