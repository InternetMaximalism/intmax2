use intmax2_client_sdk::client::strategy::mining::Mining;
use intmax2_interfaces::{
    api::withdrawal_server::interface::{ClaimInfo, ContractWithdrawal, WithdrawalInfo},
    data::meta_data::MetaData,
};
use intmax2_zkp::{
    common::{
        block::Block, claim::Claim, generic_address::GenericAddress, transfer::Transfer, tx::Tx,
        withdrawal::get_withdrawal_nullifier,
    },
    ethereum_types::{address::Address, u256::U256, u32limb_trait::U32LimbTrait},
};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use super::{
    data::JsDepositData,
    utils::{parse_address, parse_bytes32, parse_salt, parse_u256},
};

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsGenericAddress {
    /// true if pubkey, false if ethereum address
    pub is_pubkey: bool,
    /// hex string of 32 bytes (pubkey) or 20 bytes (ethereum address)
    pub data: String,
}

impl From<GenericAddress> for JsGenericAddress {
    fn from(generic_address: GenericAddress) -> Self {
        let is_pubkey = generic_address.is_pubkey;
        let data = if is_pubkey {
            generic_address.to_pubkey().unwrap().to_hex()
        } else {
            generic_address.to_address().unwrap().to_string()
        };
        Self { is_pubkey, data }
    }
}

impl TryFrom<JsGenericAddress> for GenericAddress {
    type Error = JsError;

    fn try_from(js_generic_address: JsGenericAddress) -> Result<Self, Self::Error> {
        if js_generic_address.is_pubkey {
            let pubkey = U256::from_hex(&js_generic_address.data)
                .map_err(|_| JsError::new("Failed to parse pubkey"))?;
            Ok(GenericAddress::from_pubkey(pubkey))
        } else {
            let address = Address::from_hex(&js_generic_address.data)
                .map_err(|_| JsError::new("Failed to parse address"))?;
            Ok(GenericAddress::from_address(address))
        }
    }
}

#[wasm_bindgen]
impl JsGenericAddress {
    #[wasm_bindgen(constructor)]
    pub fn new(is_pubkey: bool, data: String) -> Result<Self, JsError> {
        // validation
        if is_pubkey {
            U256::from_hex(&data).map_err(|_| JsError::new("Invalid pubkey"))?;
        } else {
            Address::from_hex(&data).map_err(|_| JsError::new("Invalid address"))?;
        }
        Ok(Self { is_pubkey, data })
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsTransfer {
    pub recipient: JsGenericAddress,
    pub token_index: u32,
    pub amount: String, // 10 base string
    pub salt: String,   // hex string
}

#[wasm_bindgen]
impl JsTransfer {
    #[wasm_bindgen(constructor)]
    pub fn new(
        recipient: JsGenericAddress,
        token_index: u32,
        amount: String,
        salt: String,
    ) -> Self {
        Self {
            recipient,
            token_index,
            amount,
            salt,
        }
    }

    pub fn to_withdrawal(&self) -> Result<JsContractWithdrawal, JsError> {
        let transfer: Transfer = self.clone().try_into()?;
        if transfer.recipient.is_pubkey {
            return Err(JsError::new("Recipient must be an ethereum address"));
        }
        let recipient = transfer.recipient.to_address().unwrap();
        let nullifier = get_withdrawal_nullifier(&transfer);
        let withdrawal = ContractWithdrawal {
            recipient,
            token_index: transfer.token_index,
            amount: transfer.amount,
            nullifier,
        };
        Ok(withdrawal.into())
    }
}

impl From<Transfer> for JsTransfer {
    fn from(transfer: Transfer) -> JsTransfer {
        Self {
            recipient: transfer.recipient.into(),
            token_index: transfer.token_index,
            amount: transfer.amount.to_string(),
            salt: transfer.salt.to_string(),
        }
    }
}

impl TryFrom<JsTransfer> for Transfer {
    type Error = JsError;

    fn try_from(js_transfer: JsTransfer) -> Result<Transfer, Self::Error> {
        let recipient = js_transfer.recipient.try_into()?;
        let amount =
            parse_u256(&js_transfer.amount).map_err(|_| JsError::new("Failed to parse amount"))?;
        let salt = parse_salt(&js_transfer.salt)?;
        Ok(Transfer {
            recipient,
            token_index: js_transfer.token_index,
            amount,
            salt,
        })
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsTx {
    pub transfer_tree_root: String, // hex string
    pub nonce: u32,
}

impl From<Tx> for JsTx {
    fn from(tx: Tx) -> Self {
        Self {
            transfer_tree_root: tx.transfer_tree_root.to_string(),
            nonce: tx.nonce,
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsContractWithdrawal {
    pub recipient: String,
    pub token_index: u32,
    pub amount: String,
    pub nullifier: String,
}

impl From<ContractWithdrawal> for JsContractWithdrawal {
    fn from(contract_withdrawal: ContractWithdrawal) -> Self {
        Self {
            recipient: contract_withdrawal.recipient.to_hex(),
            token_index: contract_withdrawal.token_index,
            amount: contract_withdrawal.amount.to_string(),
            nullifier: contract_withdrawal.nullifier.to_hex(),
        }
    }
}

impl TryFrom<JsContractWithdrawal> for ContractWithdrawal {
    type Error = JsError;

    fn try_from(
        js_contract_withdrawal: JsContractWithdrawal,
    ) -> Result<ContractWithdrawal, Self::Error> {
        let recipient = parse_address(&js_contract_withdrawal.recipient)?;
        let amount = parse_u256(&js_contract_withdrawal.amount)?;
        let nullifier = parse_bytes32(&js_contract_withdrawal.nullifier)?;
        Ok(ContractWithdrawal {
            recipient,
            token_index: js_contract_withdrawal.token_index,
            amount,
            nullifier,
        })
    }
}

#[wasm_bindgen]
impl JsContractWithdrawal {
    #[wasm_bindgen(constructor)]
    pub fn new(recipient: String, token_index: u32, amount: String, nullifier: String) -> Self {
        Self {
            recipient,
            token_index,
            amount,
            nullifier,
        }
    }

    pub fn hash(&self) -> Result<String, JsError> {
        let contract_withdrawal: ContractWithdrawal = self.clone().try_into()?;
        let hash = contract_withdrawal.withdrawal_hash().to_hex();
        Ok(hash)
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsClaim {
    pub recipient: String,
    pub amount: String,
    pub nullifier: String,
    pub block_hash: String,
    pub block_number: u32,
}

impl From<Claim> for JsClaim {
    fn from(claim: Claim) -> Self {
        Self {
            recipient: claim.recipient.to_hex(),
            amount: claim.amount.to_string(),
            nullifier: claim.nullifier.to_hex(),
            block_hash: claim.block_hash.to_hex(),
            block_number: claim.block_number,
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsMetaData {
    pub timestamp: u64,
    pub uuid: String,
}

impl From<MetaData> for JsMetaData {
    fn from(meta_data: MetaData) -> Self {
        Self {
            timestamp: meta_data.timestamp,
            uuid: meta_data.uuid.to_string(),
        }
    }
}

impl From<JsMetaData> for MetaData {
    fn from(js_meta_data: JsMetaData) -> Self {
        Self {
            timestamp: js_meta_data.timestamp,
            uuid: js_meta_data.uuid,
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsWithdrawalInfo {
    pub status: String,
    pub contract_withdrawal: JsContractWithdrawal,
}

impl From<WithdrawalInfo> for JsWithdrawalInfo {
    fn from(withdrawal_info: WithdrawalInfo) -> Self {
        Self {
            status: withdrawal_info.status.to_string(),
            contract_withdrawal: withdrawal_info.contract_withdrawal.into(),
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsClaimInfo {
    pub status: String,
    pub claim: JsClaim,
}

impl From<ClaimInfo> for JsClaimInfo {
    fn from(claim_info: ClaimInfo) -> Self {
        Self {
            status: claim_info.status.to_string(),
            claim: claim_info.claim.into(),
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsBlock {
    pub prev_block_hash: String,
    pub deposit_tree_root: String,
    pub signature_hash: String,
    pub timestamp: u64,
    pub block_number: u32,
}

impl From<Block> for JsBlock {
    fn from(block: Block) -> Self {
        Self {
            prev_block_hash: block.prev_block_hash.to_hex(),
            deposit_tree_root: block.deposit_tree_root.to_hex(),
            signature_hash: block.signature_hash.to_hex(),
            timestamp: block.timestamp,
            block_number: block.block_number,
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsMining {
    pub meta: JsMetaData,
    pub deposit_data: JsDepositData,
    pub block: JsBlock,
    pub maturity: u64,
    pub status: String,
}

impl From<Mining> for JsMining {
    fn from(mining: Mining) -> Self {
        Self {
            meta: mining.meta.meta.into(),
            deposit_data: mining.deposit_data.into(),
            block: mining.block.into(),
            maturity: mining.maturity,
            status: mining.status.to_string(),
        }
    }
}
