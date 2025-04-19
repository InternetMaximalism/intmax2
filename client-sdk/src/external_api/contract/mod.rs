pub mod block_builder_registry;
pub mod data_decoder;
pub mod erc1155_contract;
pub mod erc20_contract;
pub mod erc721_contract;
pub mod error;
pub mod handlers;
pub mod liquidity_contract;

pub mod proxy_contract;
pub mod rollup_contract;
pub mod utils;
pub mod withdrawal_contract;

pub const EVENT_BLOCK_RANGE: u64 = 10000;
