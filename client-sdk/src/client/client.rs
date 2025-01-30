use intmax2_interfaces::{
    api::{
        balance_prover::interface::BalanceProverClientInterface,
        block_builder::interface::BlockBuilderClientInterface,
        store_vault_server::interface::{DataType, SaveDataEntry, StoreVaultClientInterface},
        validity_prover::interface::ValidityProverClientInterface,
        withdrawal_server::interface::{
            ClaimInfo, WithdrawalInfo, WithdrawalServerClientInterface,
        },
    },
    data::{
        deposit_data::{DepositData, TokenType},
        encryption::Encryption as _,
        proof_compression::{CompressedBalanceProof, CompressedSpentProof},
        sender_proof_set::SenderProofSet,
        transfer_data::TransferData,
        tx_data::TxData,
    },
};
use intmax2_zkp::{
    common::{
        block_builder::BlockProposal, deposit::get_pubkey_salt_hash, signature::key_set::KeySet,
        transfer::Transfer, trees::transfer_tree::TransferTree, tx::Tx,
        witness::spent_witness::SpentWitness,
    },
    constants::{NUM_TRANSFERS_IN_TX, TRANSFER_TREE_HEIGHT},
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256},
};

use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

use crate::{
    client::{strategy::mining::validate_mining_deposit_criteria, sync::utils::generate_salt},
    external_api::{
        contract::{liquidity_contract::LiquidityContract, rollup_contract::RollupContract},
        utils::time::sleep_for,
    },
};

use super::{
    config::ClientConfig,
    error::ClientError,
    history::{fetch_deposit_history, fetch_transfer_history, fetch_tx_history, HistoryEntry},
    strategy::mining::{fetch_mining_info, Mining},
    sync::utils::{generate_spent_witness, get_balance_proof},
};

pub struct Client<
    BB: BlockBuilderClientInterface,
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
    B: BalanceProverClientInterface,
    W: WithdrawalServerClientInterface,
> {
    pub config: ClientConfig,

    pub block_builder: BB,
    pub store_vault_server: S,
    pub validity_prover: V,
    pub balance_prover: B,
    pub withdrawal_server: W,

    pub liquidity_contract: LiquidityContract,
    pub rollup_contract: RollupContract,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxRequestMemo {
    pub is_registration_block: bool,
    pub tx: Tx,
    pub transfers: Vec<Transfer>,
    pub spent_witness: SpentWitness,
    pub sender_proof_set_ephemeral_key: U256,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositResult {
    pub deposit_data: DepositData,
    pub deposit_uuid: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxResult {
    pub tx_tree_root: Bytes32,
    pub transfer_uuids: Vec<String>,
    pub withdrawal_uuids: Vec<String>,
    pub transfer_data_vec: Vec<TransferData>,
    pub withdrawal_data_vec: Vec<TransferData>,
}

impl<BB, S, V, B, W> Client<BB, S, V, B, W>
where
    BB: BlockBuilderClientInterface,
    S: StoreVaultClientInterface,
    V: ValidityProverClientInterface,
    B: BalanceProverClientInterface,
    W: WithdrawalServerClientInterface,
{
    /// Back up deposit information before calling the contract's deposit function
    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_deposit(
        &self,
        depositor: Address,
        pubkey: U256,
        amount: U256,
        token_type: TokenType,
        token_address: Address,
        token_id: U256,
        is_mining: bool,
    ) -> Result<DepositResult, ClientError> {
        log::info!(
            "prepare_deposit: pubkey {}, amount {}, token_type {:?}, token_address {}, token_id {}",
            pubkey,
            amount,
            token_type,
            token_address,
            token_id
        );
        if is_mining && !validate_mining_deposit_criteria(token_type, amount) {
            return Err(ClientError::InvalidMiningDepositCriteria);
        }

        let deposit_salt = generate_salt();

        // backup before contract call
        let pubkey_salt_hash = get_pubkey_salt_hash(pubkey, deposit_salt);
        let deposit_data = DepositData {
            deposit_salt,
            depositor,
            pubkey_salt_hash,
            amount,
            is_eligible: true, // always true before determined by predicate
            token_type,
            token_address,
            token_id,
            is_mining,
            token_index: None,
        };
        let save_entry = SaveDataEntry {
            data_type: DataType::Deposit,
            pubkey,
            encrypted_data: deposit_data.encrypt(pubkey),
        };
        let ephemeral_key = KeySet::rand(&mut rand::thread_rng());
        let uuids = self
            .store_vault_server
            .save_data_batch(ephemeral_key, &[save_entry])
            .await?;
        let deposit_uuid = uuids
            .first()
            .ok_or(ClientError::UnexpectedError(
                "deposit_uuid not found".to_string(),
            ))?
            .clone();
        let result = DepositResult {
            deposit_data,
            deposit_uuid,
        };

        Ok(result)
    }

    /// Send a transaction request to the block builder
    pub async fn send_tx_request(
        &self,
        block_builder_url: &str,
        key: KeySet,
        transfers: Vec<Transfer>,
    ) -> Result<TxRequestMemo, ClientError> {
        // input validation
        if transfers.is_empty() {
            return Err(ClientError::TransferLenError(
                "transfers is empty".to_string(),
            ));
        }
        if transfers.len() > NUM_TRANSFERS_IN_TX {
            return Err(ClientError::TransferLenError(
                "transfers is too long".to_string(),
            ));
        }

        // sync balance proof
        self.sync(key).await?;

        let user_data = self.get_user_data(key).await?;

        let balance_proof =
            get_balance_proof(&user_data)?.ok_or(ClientError::CannotSendTxByZeroBalanceAccount)?;

        // balance check
        let balances = user_data.balances();
        for transfer in &transfers {
            let balance = balances
                .0
                .get(&transfer.token_index)
                .cloned()
                .unwrap_or_default();
            if balance.is_insufficient {
                return Err(ClientError::BalanceError(format!(
                    "Already insufficient: token index {}",
                    transfer.token_index
                )));
            }
            if balance.amount < transfer.amount {
                return Err(ClientError::BalanceError(format!(
                    "Insufficient balance: {} < {} for token #{}",
                    balance.amount, transfer.amount, transfer.token_index
                )));
            }
        }

        // generate spent proof
        let tx_nonce = user_data.full_private_state.nonce;
        let spent_witness =
            generate_spent_witness(&user_data.full_private_state, tx_nonce, &transfers).await?;
        let spent_proof = self.balance_prover.prove_spent(key, &spent_witness).await?;
        let tx = spent_witness.tx;

        // save sender proof set in advance to avoid delay
        let spent_proof = CompressedSpentProof::new(&spent_proof)?;
        let prev_balance_proof = CompressedBalanceProof::new(&balance_proof)?;
        let sender_proof_set = SenderProofSet {
            spent_proof,
            prev_balance_proof,
        };
        let ephemeral_key = KeySet::rand(&mut rand::thread_rng());
        self.store_vault_server
            .save_sender_proof_set(
                ephemeral_key,
                &sender_proof_set.encrypt(ephemeral_key.pubkey),
            )
            .await?;
        let sender_proof_set_ephemeral_key: U256 =
            BigUint::from(ephemeral_key.privkey).try_into().unwrap();

        // fetch if this is first time tx
        let account_info = self.validity_prover.get_account_info(key.pubkey).await?;
        let is_registration_block = account_info.account_id.is_none();

        // send tx request
        let mut retries = 0;
        loop {
            let result = self
                .block_builder
                .send_tx_request(
                    block_builder_url,
                    is_registration_block,
                    key.pubkey,
                    tx,
                    None,
                )
                .await;
            match result {
                Ok(_) => break,
                Err(e) => {
                    if retries >= self.config.block_builder_request_limit {
                        return Err(ClientError::SendTxRequestError(format!(
                            "failed to send tx request: {}",
                            e
                        )));
                    }
                    retries += 1;
                    log::info!(
                        "Failed to send tx request, retrying in {} seconds. error: {}",
                        self.config.block_builder_request_interval,
                        e
                    );
                    sleep_for(self.config.block_builder_request_interval).await;
                }
            }
        }
        let memo = TxRequestMemo {
            is_registration_block,
            tx,
            transfers,
            spent_witness,
            sender_proof_set_ephemeral_key,
        };
        Ok(memo)
    }

    pub async fn query_proposal(
        &self,
        block_builder_url: &str,
        key: KeySet,
        is_registration_block: bool,
        tx: Tx,
    ) -> Result<Option<BlockProposal>, ClientError> {
        let proposal = self
            .block_builder
            .query_proposal(block_builder_url, is_registration_block, key.pubkey, tx)
            .await?;
        Ok(proposal)
    }

    /// Verify the proposal, and send the signature to the block builder
    pub async fn finalize_tx(
        &self,
        block_builder_url: &str,
        key: KeySet,
        memo: &TxRequestMemo,
        proposal: &BlockProposal,
    ) -> Result<TxResult, ClientError> {
        // verify proposal
        proposal
            .verify(memo.tx)
            .map_err(|e| ClientError::InvalidBlockProposal(format!("{}", e)))?;

        let mut entries = vec![];

        let tx_data = TxData {
            tx_index: proposal.tx_index,
            tx_merkle_proof: proposal.tx_merkle_proof.clone(),
            tx_tree_root: proposal.tx_tree_root,
            spent_witness: memo.spent_witness.clone(),
            sender_proof_set_ephemeral_key: memo.sender_proof_set_ephemeral_key,
        };

        entries.push(SaveDataEntry {
            data_type: DataType::Tx,
            pubkey: key.pubkey,
            encrypted_data: tx_data.encrypt(key.pubkey),
        });

        // save transfer data
        let mut transfer_tree = TransferTree::new(TRANSFER_TREE_HEIGHT);
        for transfer in &memo.transfers {
            transfer_tree.push(*transfer);
        }

        let mut transfer_data_vec = Vec::new();
        let mut withdrawal_data_vec = Vec::new();
        for (i, transfer) in memo.transfers.iter().enumerate() {
            let transfer_merkle_proof = transfer_tree.prove(i as u64);
            let transfer_data = TransferData {
                sender: key.pubkey,
                transfer: *transfer,
                transfer_index: i as u32,
                transfer_merkle_proof,
                sender_proof_set_ephemeral_key: memo.sender_proof_set_ephemeral_key,
                sender_proof_set: None,
                tx: memo.tx,
                tx_index: proposal.tx_index,
                tx_merkle_proof: proposal.tx_merkle_proof.clone(),
                tx_tree_root: proposal.tx_tree_root,
            };
            let data_type = if transfer.recipient.is_pubkey {
                DataType::Transfer
            } else {
                DataType::Withdrawal
            };
            let pubkey = if transfer.recipient.is_pubkey {
                transfer_data_vec.push(transfer_data.clone());
                transfer.recipient.to_pubkey().unwrap()
            } else {
                withdrawal_data_vec.push(transfer_data.clone());
                key.pubkey
            };
            entries.push(SaveDataEntry {
                data_type,
                pubkey,
                encrypted_data: transfer_data.encrypt(pubkey),
            });
        }

        let uuids = self
            .store_vault_server
            .save_data_batch(key, &entries)
            .await?;

        // sign and post signature
        let signature = proposal.sign(key);
        self.block_builder
            .post_signature(
                block_builder_url,
                memo.is_registration_block,
                signature.pubkey,
                memo.tx,
                signature.signature,
            )
            .await?;

        let transfer_uuids = uuids
            .iter()
            .zip(entries.iter())
            .filter_map(|(uuid, entry)| {
                if entry.data_type == DataType::Transfer {
                    Some(uuid.clone())
                } else {
                    None
                }
            })
            .collect();
        let withdrawal_uuids = uuids
            .iter()
            .zip(entries.iter())
            .filter_map(|(uuid, entry)| {
                if entry.data_type == DataType::Withdrawal {
                    Some(uuid.clone())
                } else {
                    None
                }
            })
            .collect();

        let result = TxResult {
            tx_tree_root: proposal.tx_tree_root,
            transfer_uuids,
            withdrawal_uuids,
            transfer_data_vec,
            withdrawal_data_vec,
        };

        Ok(result)
    }

    pub async fn get_withdrawal_info(
        &self,
        key: KeySet,
    ) -> Result<Vec<WithdrawalInfo>, ClientError> {
        let withdrawal_info = self.withdrawal_server.get_withdrawal_info(key).await?;
        Ok(withdrawal_info)
    }

    pub async fn get_withdrawal_info_by_recipient(
        &self,
        recipient: Address,
    ) -> Result<Vec<WithdrawalInfo>, ClientError> {
        let withdrawal_info = self
            .withdrawal_server
            .get_withdrawal_info_by_recipient(recipient)
            .await?;
        Ok(withdrawal_info)
    }

    pub async fn get_mining_list(&self, key: KeySet) -> Result<Vec<Mining>, ClientError> {
        let minings = fetch_mining_info(
            &self.store_vault_server,
            &self.validity_prover,
            &self.liquidity_contract,
            key,
            self.config.deposit_timeout,
        )
        .await?;
        Ok(minings)
    }

    pub async fn get_claim_info(&self, key: KeySet) -> Result<Vec<ClaimInfo>, ClientError> {
        let claim_info = self.withdrawal_server.get_claim_info(key).await?;
        Ok(claim_info)
    }

    pub async fn fetch_deposit_history(
        &self,
        key: KeySet,
    ) -> Result<Vec<HistoryEntry<DepositData>>, ClientError> {
        fetch_deposit_history(self, key).await
    }

    pub async fn fetch_transfer_history(
        &self,
        key: KeySet,
    ) -> Result<Vec<HistoryEntry<TransferData>>, ClientError> {
        fetch_transfer_history(self, key).await
    }

    pub async fn fetch_tx_history(
        &self,
        key: KeySet,
    ) -> Result<Vec<HistoryEntry<TxData>>, ClientError> {
        fetch_tx_history(self, key).await
    }
}
