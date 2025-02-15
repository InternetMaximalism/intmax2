use intmax2_zkp::ethereum_types::u256::U256;

pub trait Validation {
    fn validate(&self, pubkey: U256) -> anyhow::Result<()>;
}
