use std::path::PathBuf;

use clap::{Parser, Subcommand};
use ethers::types::{Address as EthAddress, H256};
use intmax2_interfaces::{
    api::store_vault_server::types::CursorOrder, data::deposit_data::TokenType,
};
use intmax2_zkp::ethereum_types::{address::Address, bytes32::Bytes32, u256::U256};

#[derive(Parser)]
#[clap(name = "intmax2_cli")]
#[clap(about = "Intmax2 CLI tool")]
pub struct Args {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Transfer {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        to: Bytes32,
        #[clap(long)]
        amount: U256,
        #[clap(long)]
        token_index: u32,
        #[clap(long)]
        fee_token_index: Option<u32>,
        #[clap(long, default_value = "false")]
        wait: bool,
    },
    Withdrawal {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        to: Address,
        #[clap(long)]
        amount: U256,
        #[clap(long)]
        token_index: u32,
        #[clap(long)]
        fee_token_index: Option<u32>,
        #[clap(long, default_value = "false")]
        with_claim_fee: bool,
        #[clap(long, default_value = "false")]
        wait: bool,
    },
    BatchTransfer {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        csv_path: String,
        #[clap(long)]
        fee_token_index: Option<u32>,
        #[clap(long, default_value = "false")]
        wait: bool,
    },
    Deposit {
        #[clap(long)]
        eth_private_key: H256,
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        token_type: TokenType,
        #[clap(long)]
        amount: Option<U256>,
        #[clap(long)]
        token_address: Option<EthAddress>,
        #[clap(long)]
        token_id: Option<U256>,
        #[clap(long, default_value = "false")]
        mining: bool,
    },
    SyncWithdrawals {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        fee_token_index: Option<u32>,
    },
    SyncClaims {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        recipient: EthAddress,
        #[clap(long)]
        fee_token_index: Option<u32>,
    },
    Balance {
        #[clap(long)]
        private_key: H256,
    },
    History {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        order: Option<CursorOrder>, // asc or desc
        #[clap(long)]
        from: Option<u64>,
    },
    WithdrawalStatus {
        #[clap(long)]
        private_key: H256,
    },
    MiningList {
        #[clap(long)]
        private_key: H256,
    },
    ClaimStatus {
        #[clap(long)]
        private_key: H256,
    },
    ClaimWithdrawals {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        eth_private_key: H256,
    },
    Resync {
        #[clap(long)]
        private_key: H256,
        #[clap(long, default_value = "false")]
        deep: bool,
    },
    MakeBackup {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        dir: Option<PathBuf>,
        #[clap(long)]
        from: Option<u64>,
    },
    IncorporateBackup {
        #[clap(long)]
        path: PathBuf,
    },
    GenerateKey,
    GenerateFromEthKey {
        #[clap(long)]
        eth_private_key: H256,
    },
}
