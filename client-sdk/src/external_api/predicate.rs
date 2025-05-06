use alloy::{
    primitives::{Address, Bytes, B256, U256},
    sol,
    sol_types::{SolCall, SolValue},
};
use intmax2_interfaces::api::error::ServerError;
use serde::Deserialize;

use crate::external_api::utils::query::post_request;

sol! {
    function depositNativeToken(bytes32 recipientSaltHash);
    function depositERC20(address tokenAddress, bytes32 recipientSaltHash, uint256 amount);
    function depositERC721(address tokenAddress, bytes32 recipientSaltHash, uint256 tokenId);
    function depositERC1155(address tokenAddress, bytes32 recipientSaltHash, uint256 tokenId, uint256 amount);

    struct PredicateMessage {
        string taskId;
        uint256 expiryBlock;
        address[] signers;
        bytes[] signature;
    }
}

#[derive(Debug, Clone)]
pub enum PermissionRequest {
    Native {
        recipient_salt_hash: B256,
        amount: U256,
    },
    ERC20 {
        token_address: Address,
        recipient_salt_hash: B256,
        amount: U256,
    },
    ERC721 {
        token_address: Address,
        recipient_salt_hash: B256,
        token_id: U256,
    },
    ERC1155 {
        token_address: Address,
        recipient_salt_hash: B256,
        token_id: U256,
        amount: U256,
    },
}

impl PermissionRequest {
    pub fn to_encoded_data(&self) -> Vec<u8> {
        match self {
            PermissionRequest::Native {
                recipient_salt_hash,
                ..
            } => depositNativeTokenCall::new((*recipient_salt_hash,)).abi_encode(),
            PermissionRequest::ERC20 {
                token_address,
                recipient_salt_hash,
                amount,
            } => {
                depositERC20Call::new((*token_address, *recipient_salt_hash, *amount)).abi_encode()
            }
            PermissionRequest::ERC721 {
                token_address,
                recipient_salt_hash,
                token_id,
            } => depositERC721Call::new((*token_address, *recipient_salt_hash, *token_id))
                .abi_encode(),
            PermissionRequest::ERC1155 {
                token_address,
                recipient_salt_hash,
                token_id,
                amount,
            } => {
                depositERC1155Call::new((*token_address, *recipient_salt_hash, *token_id, *amount))
                    .abi_encode()
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
        from: Address,
        to: Address,
        value: U256,
        request: PermissionRequest,
    ) -> Result<Vec<u8>, ServerError> {
        let encoded_data = request.to_encoded_data();
        self.get_permission(from, to, value, &encoded_data).await
    }

    async fn get_permission(
        &self,
        from: Address,
        to: Address,
        value: U256,
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
    let message = PredicateMessage {
        taskId: message.task_id,
        expiryBlock: U256::from(message.expiry_block),
        signers: message
            .signers
            .into_iter()
            .map(|address| address.parse().unwrap())
            .collect(),
        signature: message
            .signature
            .into_iter()
            .map(|signature| {
                hex::decode(signature.strip_prefix("0x").unwrap())
                    .unwrap()
                    .into()
            })
            .collect::<Vec<Bytes>>(),
    };
    message.abi_encode()
}
