use clap::{Parser, Subcommand};
use ethers::types::{Address as EthAddress, H256};
use intmax2_interfaces::data::deposit_data::TokenType;

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
        to: String,
        #[clap(long)]
        amount: u128,
        #[clap(long)]
        token_index: u32,
        #[clap(long)]
        fee_token_index: Option<u32>,
    },
    BatchTransfer {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        csv_path: String,
        #[clap(long)]
        fee_token_index: Option<u32>,
    },
    Deposit {
        #[clap(long)]
        eth_private_key: H256,
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        token_type: TokenType,
        #[clap(long)]
        amount: Option<u128>,
        #[clap(long)]
        token_address: Option<EthAddress>,
        #[clap(long)]
        token_id: Option<u128>,
        #[clap(long)]
        is_mining: Option<bool>,
    },
    SyncWithdrawals {
        #[clap(long)]
        private_key: H256,
    },
    SyncClaims {
        #[clap(long)]
        private_key: H256,
        #[clap(long)]
        recipient: EthAddress,
    },
    Balance {
        #[clap(long)]
        private_key: Option<H256>,
    },
    History {
        #[clap(long)]
        private_key: Option<H256>,
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
    GenerateKey,
    GenerateFromEthKey {
        #[clap(long)]
        eth_private_key: H256,
    },
}
