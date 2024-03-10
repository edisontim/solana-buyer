use std::str::FromStr;

use eyre::eyre;
use solana_client::{
    pubsub_client::PubsubClient,
    rpc_client::RpcClient,
    rpc_config::{
        RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcTransactionLogsConfig,
        RpcTransactionLogsFilter,
    },
};

use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::Keypair,
    signer::{EncodableKeypair, Signer},
};

use solana_transaction_status::{TransactionDetails, UiTransactionEncoding};

const RPC_URL: &str = "https://solana-mainnet.g.alchemy.com/v2/_-qkQprvyNYiqW5hBWRlNgNPwAOSObpU";
const WS_URL: &str = "wss://solana-mainnet.g.alchemy.com/v2/_-qkQprvyNYiqW5hBWRlNgNPwAOSObpU";

fn subscribe_to_txs() -> Result<(), eyre::Error> {
    let (mut log_subscription, log_subscription_receiver) = PubsubClient::program_subscribe(
        WS_URL,
        &Pubkey::from_str("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8")?,
        Some(RpcProgramAccountsConfig {
            account_config: RpcAccountInfoConfig,
        }),
    )
    .unwrap();

    loop {
        match log_subscription_receiver.recv() {
            Ok(response) => {
                println!("logs : {:?}", response);
            }
            Err(e) => {
                println!("error : {:?}", e);
            }
        }
    }
}

fn main() {
    subscribe_to_txs();
    let keypair = Keypair::from_base58_string(
        "cgzhM6dQr6BtfrC78QucfirPbud13ethe8fv37egUP262Nnx1WPkDaabxNtiLKBdn6fnH7TbPhhh1SwgCAhEtif",
    );
    let rpc_client = RpcClient::new(RPC_URL);
}
