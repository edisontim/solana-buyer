use std::str::FromStr;
use std::sync::Arc;

use eyre::eyre;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::{commitment_config::CommitmentConfig, signature::Signature};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiMessage, UiTransactionEncoding,
};

use crate::utils::get_transaction_from_signature;
use crate::websocket::LogsSubscribeResponse;

/// Get the market_id and amm_id from the log response
pub(super) async fn get_market_id_and_amm_id(
    client: Arc<RpcClient>,
    log: LogsSubscribeResponse,
) -> Result<(Pubkey, Pubkey), eyre::Error> {
    if log.params.result.value.err.is_some() {
        return Err(eyre!("Received transaction is a reverted tx"));
    }

    let signature = get_transaction_signature(log)?;

    let pool_creation_tx = get_transaction_from_signature(
        &client,
        signature,
        RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            max_supported_transaction_version: Some(0),
            commitment: Some(CommitmentConfig::confirmed()),
        },
    )
    .await?;

    let account_keys = get_account_keys(pool_creation_tx)?;

    get_market_id_amm_id_from_account_keys(account_keys)
}

/// Get the transaction signature from the log
pub(super) fn get_transaction_signature(
    log: LogsSubscribeResponse,
) -> Result<Signature, eyre::Error> {
    let signature = log.params.result.value.signature;
    tracing::debug!("signature {:?}", signature);

    let signature = Signature::from_str(&signature)?;
    Ok(signature)
}

/// Get the account keys from the transaction
pub(super) fn get_account_keys(
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

/// Get the market_id and amm_id from the account keys
pub(super) fn get_market_id_amm_id_from_account_keys(
    mut account_keys: Vec<String>,
) -> Result<(Pubkey, Pubkey), eyre::Error> {
    let market_id = account_keys.pop().unwrap();
    let market_id = Pubkey::from_str(&market_id)?;

    let amm_id = account_keys.remove(2);
    let amm_id = Pubkey::from_str(&amm_id)?;

    Ok((market_id, amm_id))
}
