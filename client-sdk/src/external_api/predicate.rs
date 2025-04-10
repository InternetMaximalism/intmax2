use ethers::{
    abi::{Function, Param, ParamType, StateMutability, Token, Tokenizable},
    contract::encode_function_data,
    types::{Address as EtherAddress, Bytes, H256 as EtherH256, U256 as EtherU256},
};
use intmax2_interfaces::api::error::ServerError;
use intmax2_zkp::ethereum_types::{address::Address, bytes32::Bytes32, u256::U256};
use serde::Deserialize;

use crate::external_api::utils::query::post_request;

#[derive(Debug, Clone)]
pub enum PermissionRequest {
    Native {
        recipient_salt_hash: EtherH256,
        amount: EtherU256,
    },
    ERC20 {
        token_address: EtherAddress,
        recipient_salt_hash: EtherH256,
        amount: EtherU256,
    },
    ERC721 {
        token_address: EtherAddress,
        recipient_salt_hash: EtherH256,
        token_id: EtherU256,
    },
    ERC1155 {
        token_address: EtherAddress,
        recipient_salt_hash: EtherH256,
        token_id: EtherU256,
        amount: EtherU256,
    },
}

impl PermissionRequest {
    pub fn to_encoded_data(&self) -> Bytes {
        match self {
            PermissionRequest::Native {
                recipient_salt_hash,
                ..
            } => {
                #[allow(deprecated)]
                let function = Function {
                    name: "depositNativeToken".to_string(),
                    inputs: vec![Param {
                        name: "recipientSaltHash".to_string(),
                        kind: ParamType::FixedBytes(32),
                        internal_type: Some("bytes32".to_string()),
                    }],
                    constant: None,
                    outputs: vec![],
                    state_mutability: StateMutability::NonPayable,
                };

                encode_function_data(
                    &function,
                    [Token::FixedBytes(
                        recipient_salt_hash.to_fixed_bytes().to_vec(),
                    )],
                )
                .unwrap()
            }
            PermissionRequest::ERC20 {
                token_address,
                recipient_salt_hash,
                amount,
            } => {
                #[allow(deprecated)]
                let function = Function {
                    name: "depositERC20".to_string(),
                    inputs: vec![
                        Param {
                            name: "tokenAddress".to_string(),
                            kind: ParamType::Address,
                            internal_type: None,
                        },
                        Param {
                            name: "recipientSaltHash".to_string(),
                            kind: ParamType::FixedBytes(32),
                            internal_type: None,
                        },
                        Param {
                            name: "amount".to_string(),
                            kind: ParamType::Uint(256),
                            internal_type: None,
                        },
                    ],
                    constant: None,
                    outputs: vec![],
                    state_mutability: StateMutability::NonPayable,
                };

                encode_function_data(
                    &function,
                    [
                        Token::Address(*token_address),
                        Token::FixedBytes(recipient_salt_hash.to_fixed_bytes().to_vec()),
                        Token::Uint(*amount),
                    ],
                )
                .unwrap()
            }
            PermissionRequest::ERC721 {
                token_address,
                recipient_salt_hash,
                token_id,
            } => {
                #[allow(deprecated)]
                let function = Function {
                    name: "depositERC721".to_string(),
                    inputs: vec![
                        Param {
                            name: "tokenAddress".to_string(),
                            kind: ParamType::Address,
                            internal_type: None,
                        },
                        Param {
                            name: "recipientSaltHash".to_string(),
                            kind: ParamType::FixedBytes(32),
                            internal_type: None,
                        },
                        Param {
                            name: "tokenId".to_string(),
                            kind: ParamType::Uint(256),
                            internal_type: None,
                        },
                    ],
                    constant: None,
                    outputs: vec![],
                    state_mutability: StateMutability::NonPayable,
                };

                encode_function_data(
                    &function,
                    [
                        Token::Address(*token_address),
                        Token::FixedBytes(recipient_salt_hash.to_fixed_bytes().to_vec()),
                        Token::Uint(*token_id),
                    ],
                )
                .unwrap()
            }
            PermissionRequest::ERC1155 {
                token_address,
                recipient_salt_hash,
                token_id,
                amount,
            } => {
                #[allow(deprecated)]
                let function = Function {
                    name: "depositERC1155".to_string(),
                    inputs: vec![
                        Param {
                            name: "tokenAddress".to_string(),
                            kind: ParamType::Address,
                            internal_type: None,
                        },
                        Param {
                            name: "recipientSaltHash".to_string(),
                            kind: ParamType::FixedBytes(32),
                            internal_type: None,
                        },
                        Param {
                            name: "tokenId".to_string(),
                            kind: ParamType::Uint(256),
                            internal_type: None,
                        },
                        Param {
                            name: "amount".to_string(),
                            kind: ParamType::Uint(256),
                            internal_type: None,
                        },
                    ],
                    constant: None,
                    outputs: vec![],
                    state_mutability: StateMutability::NonPayable,
                };

                encode_function_data(
                    &function,
                    [
                        Token::Address(*token_address),
                        Token::FixedBytes(recipient_salt_hash.to_fixed_bytes().to_vec()),
                        Token::Uint(*token_id),
                        Token::Uint(*amount),
                    ],
                )
                .unwrap()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PredicateClient {
    base_url: String,
}

impl PredicateClient {
    pub async fn get_deposit_permission(
        &self,
        from: Address,
        to: Address,
        value: U256,
        request: PermissionRequest,
    ) -> Result<Vec<u8>, ServerError> {
        let encoded_data = request.to_encoded_data();
        let body = serde_json::json!({
            "from": from.to_string(),
            "to": to.to_string(),
            "data": "0x".to_string() + &hex::encode(encoded_data),
            "msg_value": value.to_string(),
        });
        let response: PredicateResponse =
            post_request(&self.base_url, "/v1/predicate/evaluate-policy", Some(&body)).await?;
        Ok(encode_predicate_message(response))
    }

    async fn get_permission(
        &self,
        from: Address,
        to: Address,
        value: U256,
        encoded_data: &[u8],
    ) -> Result<Vec<u8>, ServerError> {
        let body = serde_json::json!({
            "from": from.to_string(),
            "to": to.to_string(),
            "data": "0x".to_string() + &hex::encode(encoded_data),
            "msg_value": value.to_string(),
        });
        let response: PredicateResponse =
            post_request(&self.base_url, "/v1/predicate/evaluate-policy", Some(&body)).await?;
        Ok(encode_predicate_message(response))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PredicateResponse {
    pub task_id: String,
    pub is_compliant: bool,
    pub signers: Vec<String>,
    pub signature: Vec<String>,
    pub expiry_block: u64,
}

fn encode_predicate_message(message: PredicateResponse) -> Vec<u8> {
    let tokens = Token::Tuple(vec![
        Token::String(message.task_id),
        Token::Uint(message.expiry_block.into()),
        Token::Array(
            message
                .signers
                .into_iter()
                .map(|address| Token::Address(address.parse().unwrap()))
                .collect(),
        ),
        Token::Array(
            message
                .signature
                .into_iter()
                .map(|signature| {
                    Token::Bytes(hex::decode(signature.strip_prefix("0x").unwrap()).unwrap())
                })
                .collect(),
        ),
    ]);
    ethers::abi::encode(&[tokens]).into()
}

#[cfg(test)]
mod tests {

    use super::*;
    use intmax2_zkp::ethereum_types::{address::Address, u32limb_trait::U32LimbTrait};

    #[test]
    fn test_encode_request() {
        let request = PermissionRequest::Native {
            recipient_salt_hash: EtherH256::zero(),
            amount: EtherU256::zero(),
        };
        let _encoded_data = request.to_encoded_data();
    }

    #[tokio::test]
    async fn test_get_deposit_permission() {
        let client = PredicateClient {
            base_url: "https://sandbox.mainnet.api.predicate.intmax.xyz".to_string(),
        };
        let from = Address::zero();
        let to = Address::from_hex("0x026bC6D4deec82cFF6450bc1D5bEb3B62724217b").unwrap();
        let value = U256::zero();
        let result = client
            .get_deposit_permission(
                from,
                to,
                value,
                PermissionRequest::Native {
                    recipient_salt_hash: EtherH256::zero(),
                    amount: EtherU256::zero(),
                },
            )
            .await
            .unwrap();
        assert!(!result.is_empty());
    }
}
