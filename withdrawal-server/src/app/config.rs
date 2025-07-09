use intmax2_interfaces::utils::{fee::Fee, key::ViewPair, network::Network};

use super::error::WithdrawalServerError;
use crate::Env;
use intmax2_client_sdk::client::config::network_from_env;

pub struct Config {
    pub network: Network,
    pub is_faster_mining: bool,
    pub withdrawal_beneficiary_key: ViewPair,
    pub claim_beneficiary_key: ViewPair,
    pub direct_withdrawal_fee: Option<Vec<Fee>>,
    pub claimable_withdrawal_fee: Option<Vec<Fee>>,
    pub claim_fee: Option<Vec<Fee>>,
}

impl Config {
    pub fn from_env(env: &Env) -> Result<Self, WithdrawalServerError> {
        let network = network_from_env();
        Ok(Self {
            network,
            is_faster_mining: env.is_faster_mining,
            withdrawal_beneficiary_key: env.withdrawal_beneficiary_view_pair,
            claim_beneficiary_key: env.claim_beneficiary_view_pair,
            direct_withdrawal_fee: env.direct_withdrawal_fee.clone().map(|l| l.0),
            claimable_withdrawal_fee: env.claimable_withdrawal_fee.clone().map(|l| l.0),
            claim_fee: env.claim_fee.clone().map(|l| l.0),
        })
    }
}
