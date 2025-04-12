use ethers::{
    abi::{Function, Param, ParamType, StateMutability, Token},
    types::{Address as EtherAddress, H256 as EtherH256, U256 as EtherU256},
};
use intmax2_interfaces::api::error::ServerError;
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
    pub fn to_encoded_data(&self) -> Vec<u8> {
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
                        internal_type: None,
                    }],
                    constant: None,
                    outputs: vec![],
                    state_mutability: StateMutability::NonPayable,
                };

                function
                    .encode_input(&[Token::FixedBytes(
                        recipient_salt_hash.to_fixed_bytes().to_vec(),
                    )])
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

                function
                    .encode_input(&[
                        Token::Address(*token_address),
                        Token::FixedBytes(recipient_salt_hash.to_fixed_bytes().to_vec()),
                        Token::Uint(*amount),
                    ])
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

                function
                    .encode_input(&[
                        Token::Address(*token_address),
                        Token::FixedBytes(recipient_salt_hash.to_fixed_bytes().to_vec()),
                        Token::Uint(*token_id),
                    ])
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

                function
                    .encode_input(&[
                        Token::Address(*token_address),
                        Token::FixedBytes(recipient_salt_hash.to_fixed_bytes().to_vec()),
                        Token::Uint(*token_id),
                        Token::Uint(*amount),
                    ])
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
    pub fn new(base_url: String) -> Self {
        PredicateClient { base_url }
    }

    pub async fn get_deposit_permission(
        &self,
        from: EtherAddress,
        to: EtherAddress,
        value: EtherU256,
        request: PermissionRequest,
    ) -> Result<Vec<u8>, ServerError> {
        let encoded_data = request.to_encoded_data();
        self.get_permission(from, to, value, &encoded_data).await
    }

    async fn get_permission(
        &self,
        from: EtherAddress,
        to: EtherAddress,
        value: EtherU256,
        encoded_data: &[u8],
    ) -> Result<Vec<u8>, ServerError> {
        let body = serde_json::json!({
            "from": format!("{:?}", from),
            "to": format!("{:?}", to),
            "data": "0x".to_string() + &hex::encode(encoded_data),
            "msg_value":format!("{:?}", value),
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
    ethers::abi::encode(&[tokens])
}
