use std::str::FromStr;
use std::sync::Arc;

use eyre::{eyre, OptionExt};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::{commitment_config::CommitmentConfig, signature::Signature};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiMessage, UiTransactionEncoding,
};

use crate::actors::swapper::actor::PoolInitTxInfos;
use crate::constants::{
    AMM_ID_INDEX_IN_INIT_INSTRUCTION, AMM_V4, BASE_MINT_INDEX_IN_INIT_INSTRUCTION,
    MARKET_ID_INDEX_IN_INIT_INSTRUCTION, QUOTE_MINT_INDEX_IN_INIT_INSTRUCTION,
};
use crate::utils::get_transaction_from_signature;
use crate::websocket::LogsSubscribeResponse;

/// Get the market_id and amm_id from the log response
pub(super) async fn get_pool_init_infos(
    client: Arc<RpcClient>,
    log: LogsSubscribeResponse,
) -> Result<PoolInitTxInfos, eyre::Error> {
    if log.params.result.value.err.is_some() {
        return Err(eyre!("received transaction is a reverted tx"));
    }

    let signature = get_transaction_signature(log)?;

    tracing::info!("Found initialize2 transaction (sig: {:?})", signature);

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

    let pool_init_tx_infos_indexes =
        get_useful_account_indexes_from_transaction(&pool_creation_tx)?;

    let account_keys = get_account_keys(pool_creation_tx)?;

    get_pool_init_tx_infos_from_account_keys_and_indexes(account_keys, pool_init_tx_infos_indexes)
        .await
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
pub(super) async fn get_pool_init_tx_infos_from_account_keys_and_indexes(
    account_keys: Vec<String>,
    indexes: (usize, usize, usize, usize),
) -> Result<PoolInitTxInfos, eyre::Error> {
    let amm_id = account_keys
        .get(indexes.0)
        .ok_or_eyre("Failed to get amm_id from account keys")?;
    let amm_id = Pubkey::from_str(amm_id)?;

    let market_id = account_keys
        .get(indexes.1)
        .ok_or_eyre("Failed to get market_id from account keys")?;
    let market_id = Pubkey::from_str(market_id)?;

    let base_mint = account_keys
        .get(indexes.2)
        .ok_or_eyre("Failed to get base_mint from account keys")?;
    let base_mint = Pubkey::from_str(base_mint)?;

    let quote_mint = account_keys
        .get(indexes.3)
        .ok_or_eyre("Failed to get quote_mint from account_keys")?;
    let quote_mint = Pubkey::from_str(quote_mint)?;

    Ok(PoolInitTxInfos {
        amm_id,
        market_id,
        base_mint,
        quote_mint,
    })
}

fn get_useful_account_indexes_from_transaction(
    transaction: &EncodedConfirmedTransactionWithStatusMeta,
) -> Result<(usize, usize, usize, usize), eyre::Error> {
    let table = &transaction.transaction.transaction;
    match table {
        solana_transaction_status::EncodedTransaction::Json(json_message) => {
            match &json_message.message {
                solana_transaction_status::UiMessage::Raw(ui_msg_raw) => {
                    let initialize2_instruction = ui_msg_raw
                        .instructions
                        .iter()
                        .find(|val| {
                            ui_msg_raw
                                .account_keys
                                .get(val.program_id_index as usize)
                                .ok_or_eyre("Failed to get program id index in account keys")
                                .unwrap()
                                == &AMM_V4.to_string()
                        })
                        .ok_or_eyre("Failed to get instruction")?;
                    let accounts = &initialize2_instruction.accounts;
                    return Ok((
                        accounts
                            .get(AMM_ID_INDEX_IN_INIT_INSTRUCTION)
                            .ok_or_eyre("Failed to get AMM ID in the instruction accounts")
                            .cloned()? as usize,
                        accounts
                            .get(MARKET_ID_INDEX_IN_INIT_INSTRUCTION)
                            .ok_or_eyre("Failed to get AMM ID in the instruction accounts")
                            .cloned()? as usize,
                        accounts
                            .get(BASE_MINT_INDEX_IN_INIT_INSTRUCTION)
                            .ok_or_eyre("Failed to get AMM ID in the instruction accounts")
                            .cloned()? as usize,
                        accounts
                            .get(QUOTE_MINT_INDEX_IN_INIT_INSTRUCTION)
                            .ok_or_eyre("Failed to get AMM ID in the instruction accounts")
                            .cloned()? as usize,
                    ));
                }
                _ => {
                    tracing::warn!("Unimplemented format of transaction received")
                }
            };
        }
        _ => return Err(eyre!("Wrong format of transaction")),
    };

    Err(eyre!("Failed to get a parsable transaction"))
}
