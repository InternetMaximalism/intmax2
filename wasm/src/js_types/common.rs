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
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256, u32limb_trait::U32LimbTrait},
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use super::{
    data::JsDepositData,
    utils::{parse_address, parse_bytes32, parse_salt, parse_u256},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
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

impl TryFrom<&JsGenericAddress> for GenericAddress {
    type Error = JsError;

    fn try_from(js_generic_address: &JsGenericAddress) -> Result<Self, Self::Error> {
        if js_generic_address.is_pubkey {
            let pubkey = U256::from_hex(&js_generic_address.data)
                .map_err(|_| JsError::new("Failed to parse pubkey"))?;
            Ok(pubkey.into())
        } else {
            let address = Address::from_hex(&js_generic_address.data)
                .map_err(|_| JsError::new("Failed to parse address"))?;
            Ok(address.into())
        }
    }
}

impl TryFrom<JsGenericAddress> for GenericAddress {
    type Error = JsError;

    fn try_from(js_generic_address: JsGenericAddress) -> Result<Self, Self::Error> {
        (&js_generic_address).try_into()
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

#[derive(Debug, Clone, Deserialize, Serialize)]
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
        let transfer: Transfer = self.try_into()?;
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

impl TryFrom<&JsTransfer> for Transfer {
    type Error = JsError;

    fn try_from(js_transfer: &JsTransfer) -> Result<Self, Self::Error> {
        let recipient = (&js_transfer.recipient).try_into()?;
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

impl TryFrom<&JsContractWithdrawal> for ContractWithdrawal {
    type Error = JsError;

    fn try_from(js: &JsContractWithdrawal) -> Result<Self, Self::Error> {
        let recipient = parse_address(&js.recipient)?;
        let amount = parse_u256(&js.amount)?;
        let nullifier = parse_bytes32(&js.nullifier)?;
        Ok(ContractWithdrawal {
            recipient,
            token_index: js.token_index,
            amount,
            nullifier,
        })
    }
}

impl TryFrom<JsContractWithdrawal> for ContractWithdrawal {
    type Error = JsError;

    fn try_from(js_contract_withdrawal: JsContractWithdrawal) -> Result<Self, Self::Error> {
        Self::try_from(&js_contract_withdrawal)
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
        let contract_withdrawal: ContractWithdrawal = self.try_into()?;
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
    pub digest: String,
}

impl From<MetaData> for JsMetaData {
    fn from(meta_data: MetaData) -> Self {
        Self {
            timestamp: meta_data.timestamp,
            digest: meta_data.digest.to_hex(),
        }
    }
}

impl TryFrom<&JsMetaData> for MetaData {
    type Error = JsError;

    fn try_from(js_meta_data: &JsMetaData) -> Result<MetaData, Self::Error> {
        let digest = Bytes32::from_hex(&js_meta_data.digest)
            .map_err(|_| JsError::new("Failed to parse digest"))?;
        Ok(MetaData {
            timestamp: js_meta_data.timestamp,
            digest,
        })
    }
}

impl TryFrom<JsMetaData> for MetaData {
    type Error = JsError;

    fn try_from(js_meta_data: JsMetaData) -> Result<MetaData, Self::Error> {
        Self::try_from(&js_meta_data)
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsWithdrawalInfo {
    pub status: String,
    pub contract_withdrawal: JsContractWithdrawal,
    pub l1_tx_hash: Option<String>,
}

impl From<WithdrawalInfo> for JsWithdrawalInfo {
    fn from(withdrawal_info: WithdrawalInfo) -> Self {
        Self {
            status: withdrawal_info.status.to_string(),
            contract_withdrawal: withdrawal_info.contract_withdrawal.into(),
            l1_tx_hash: withdrawal_info.l1_tx_hash.map(|hash| hash.to_hex()),
        }
    }
}

#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct JsClaimInfo {
    pub status: String,
    pub claim: JsClaim,
    pub submit_claim_proof_tx_hash: Option<String>,
    pub l1_tx_hash: Option<String>,
}

impl From<ClaimInfo> for JsClaimInfo {
    fn from(claim_info: ClaimInfo) -> Self {
        Self {
            status: claim_info.status.to_string(),
            claim: claim_info.claim.into(),
            submit_claim_proof_tx_hash: claim_info
                .submit_claim_proof_tx_hash
                .map(|hash| hash.to_hex()),
            l1_tx_hash: claim_info.l1_tx_hash.map(|hash| hash.to_hex()),
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
    pub block: Option<JsBlock>,
    pub maturity: Option<u64>,
    pub status: String,
}

impl From<Mining> for JsMining {
    fn from(mining: Mining) -> Self {
        Self {
            meta: mining.meta.into(),
            deposit_data: mining.deposit_data.into(),
            block: mining.block.map(|b| b.into()),
            maturity: mining.maturity,
            status: mining.status.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intmax2_zkp::ethereum_types::u256::U256;

    #[test]
    fn test_js_generic_address_pubkey_conversion() {
        let hex = "0x01";
        let js = JsGenericAddress {
            is_pubkey: true,
            data: hex.to_string(),
        };

        let generic: GenericAddress = (&js).try_into().expect("Conversion failed");
        assert!(generic.is_pubkey);
        assert_eq!(generic.to_pubkey().unwrap(), U256::from(1));
    }

    #[test]
    fn test_js_generic_address_address_conversion() {
        let hex = "0x000000000000000000000000000000000000dead";
        let js = JsGenericAddress {
            is_pubkey: false,
            data: hex.to_string(),
        };

        let generic: GenericAddress = (&js).try_into().expect("Conversion failed");
        assert!(!generic.is_pubkey);
        assert_eq!(generic.to_address().unwrap().to_hex(), hex);
    }

    #[test]
    fn test_js_transfer_conversion_success() {
        let js_transfer_old = JsTransfer {
            recipient: JsGenericAddress {
                is_pubkey: true,
                data: "0x02".to_string(),
            },
            token_index: 1,
            amount: "1000".to_string(),
            salt: "0x1234".to_string(),
        };

        let result: Result<Transfer, JsError> = js_transfer_old.clone().try_into();
        assert!(result.is_ok());

        let js_transfer_new = JsTransfer::from(result.unwrap());

        // Check string fields match
        assert_eq!(js_transfer_new.amount, js_transfer_old.amount);
        assert_eq!(js_transfer_new.token_index, js_transfer_old.token_index);
        assert_eq!(
            js_transfer_new.recipient.is_pubkey,
            js_transfer_old.recipient.is_pubkey
        );

        let left = U256::from_hex(&js_transfer_old.recipient.data).unwrap();
        let right = U256::from_hex(&js_transfer_new.recipient.data).unwrap();
        assert_eq!(left, right);

        // Check salt hex values (as Bytes32) are equal
        let left = Bytes32::from_hex(&js_transfer_old.salt).unwrap();
        let right = Bytes32::from_hex(&js_transfer_new.salt).unwrap();

        assert_eq!(left, right);
    }

    #[test]
    fn test_contract_withdrawal_hash() {
        let js = JsContractWithdrawal {
            recipient: "0x000000000000000000000000000000000000dead".to_string(),
            token_index: 1,
            amount: "1000".to_string(),
            nullifier: "0x0101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
        };

        let hash = js.hash();
        assert!(hash.is_ok());
        assert_eq!(hash.unwrap().len(), 66); // 0x + 64 hex digits
    }
}
