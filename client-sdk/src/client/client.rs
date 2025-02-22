use core::fmt;

use intmax2_interfaces::{
    api::{
        balance_prover::interface::BalanceProverClientInterface,
        block_builder::interface::{BlockBuilderClientInterface, Fee},
        store_vault_server::{
            interface::{DataType, SaveDataEntry, StoreVaultClientInterface},
            types::{MetaDataCursor, MetaDataCursorResponse},
        },
        validity_prover::interface::ValidityProverClientInterface,
        withdrawal_server::interface::{
            ClaimInfo, WithdrawalInfo, WithdrawalServerClientInterface,
        },
    },
    data::{
        deposit_data::{DepositData, TokenType},
        encryption::BlsEncryption as _,
        meta_data::MetaData,
        proof_compression::{CompressedBalanceProof, CompressedSpentProof},
        sender_proof_set::SenderProofSet,
        transfer_data::TransferData,
        tx_data::TxData,
        user_data::{Balances, ProcessStatus},
    },
};
use intmax2_zkp::{
    common::{
        block_builder::BlockProposal, deposit::get_pubkey_salt_hash,
        generic_address::GenericAddress, signature::key_set::KeySet, transfer::Transfer,
        trees::transfer_tree::TransferTree, tx::Tx, witness::spent_witness::SpentWitness,
    },
    constants::{NUM_TRANSFERS_IN_TX, TRANSFER_TREE_HEIGHT},
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256},
};

use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

use crate::{
    client::{
        fee_payment::generate_withdrawal_transfers,
        strategy::mining::validate_mining_deposit_criteria, sync::utils::generate_salt,
    },
    external_api::{
        contract::{
            liquidity_contract::LiquidityContract, rollup_contract::RollupContract,
            withdrawal_contract::WithdrawalContract,
        },
        utils::time::sleep_for,
    },
};

use super::{
    config::ClientConfig,
    error::ClientError,
    fee_payment::{quote_claim_fee, quote_withdrawal_fee, WithdrawalTransfers},
    fee_proof::{generate_fee_proof, quote_transfer_fee},
    history::{fetch_deposit_history, fetch_transfer_history, fetch_tx_history, HistoryEntry},
    misc::payment_memo::PaymentMemo,
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
    pub withdrawal_contract: WithdrawalContract,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMemoEntry {
    pub transfer_index: u32,
    pub topic: Bytes32,
    pub memo: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxRequestMemo {
    pub is_registration_block: bool,
    pub tx: Tx,
    pub transfers: Vec<Transfer>,
    pub spent_witness: SpentWitness,
    pub sender_proof_set_ephemeral_key: U256,
    pub payment_memos: Vec<PaymentMemoEntry>,
    pub fee_index: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositResult {
    pub deposit_data: DepositData,
    pub deposit_uuid: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeQuote {
    pub beneficiary: Option<U256>,
    pub fee: Option<Fee>,
    pub collateral_fee: Option<Fee>,
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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum TxStatus {
    Pending,
    Success,
    Failed(String),
}

impl fmt::Display for TxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxStatus::Pending => write!(f, "pending"),
            TxStatus::Success => write!(f, "success"),
            TxStatus::Failed(_) => write!(f, "failed"),
        }
    }
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
    #[allow(clippy::too_many_arguments)]
    pub async fn send_tx_request(
        &self,
        block_builder_url: &str,
        key: KeySet,
        transfers: Vec<Transfer>,
        payment_memos: Vec<PaymentMemoEntry>,
        fee_beneficiary: Option<U256>,
        fee: Option<Fee>,
        collateral_fee: Option<Fee>,
    ) -> Result<TxRequestMemo, ClientError> {
        log::info!(
            "send_tx_request: pubkey {}, transfers {}, fee_beneficiary {:?}, fee {:?}, collateral_fee {:?}",
            key.pubkey,
            transfers.len(),
            fee_beneficiary,
            fee,
            collateral_fee
        );

        // input validation
        if transfers.is_empty() {
            return Err(ClientError::TransferLenError(
                "transfers is empty".to_string(),
            ));
        }
        if transfers.len() > NUM_TRANSFERS_IN_TX - 1 {
            return Err(ClientError::TransferLenError(
                "transfers is too many".to_string(),
            ));
        }
        if fee.is_some() && fee_beneficiary.is_none() {
            return Err(ClientError::BlockBuilderFeeError(
                "fee_beneficiary is required".to_string(),
            ));
        }
        for e in &payment_memos {
            if e.transfer_index as usize >= transfers.len() {
                return Err(ClientError::PaymentMemoError(
                    "memo.transfer_index is out of range".to_string(),
                ));
            }
        }

        // fetch if this is first time tx
        let account_info = self.validity_prover.get_account_info(key.pubkey).await?;
        let is_registration_block = account_info.account_id.is_none();

        let fee_transfer = fee.map(|fee| Transfer {
            recipient: GenericAddress::from_pubkey(fee_beneficiary.unwrap()),
            amount: fee.amount,
            token_index: fee.token_index,
            salt: generate_salt(),
        });
        let collateral_transfer = collateral_fee.map(|fee| Transfer {
            recipient: GenericAddress::from_pubkey(fee_beneficiary.unwrap()),
            amount: fee.amount,
            token_index: fee.token_index,
            salt: generate_salt(),
        });

        // add fee transfer to the end
        let transfers = if let Some(fee_transfer) = fee_transfer {
            transfers
                .into_iter()
                .chain(std::iter::once(fee_transfer))
                .collect()
        } else {
            transfers
        };
        let fee_index = if fee_transfer.is_some() {
            Some(transfers.len() as u32 - 1)
        } else {
            None
        };

        // sync balance proof
        self.sync(key).await?;

        let user_data = self.get_user_data(key).await?;

        let balance_proof =
            get_balance_proof(&user_data)?.ok_or(ClientError::CannotSendTxByZeroBalanceAccount)?;

        // balance check
        let balances = user_data.balances();
        balance_check(&balances, &transfers)?;

        // balance check for collateral transfer
        if let Some(collateral_transfer) = collateral_transfer.as_ref() {
            balance_check(&balances, &[*collateral_transfer])?;
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

        let fee_proof = if let Some(fee_index) = fee_index {
            let (fee_proof, collateral_spent_witness) = generate_fee_proof(
                &self.store_vault_server,
                &self.balance_prover,
                self.config.tx_timeout,
                key,
                &user_data,
                sender_proof_set_ephemeral_key,
                tx_nonce,
                fee_index,
                &transfers,
                collateral_transfer,
            )
            .await?;
            // save tx data for collateral block
            if let Some(collateral_block) = &fee_proof.collateral_block {
                let transfer_data = &collateral_block.fee_transfer_data;
                let tx_data = TxData {
                    tx_index: transfer_data.tx_index,
                    tx_merkle_proof: transfer_data.tx_merkle_proof.clone(),
                    tx_tree_root: transfer_data.tx_tree_root,
                    spent_witness: collateral_spent_witness.unwrap(),
                    sender_proof_set_ephemeral_key: collateral_block.sender_proof_set_ephemeral_key,
                };
                let entry = SaveDataEntry {
                    data_type: DataType::Tx,
                    pubkey: key.pubkey,
                    encrypted_data: tx_data.encrypt(key.pubkey),
                };
                self.store_vault_server
                    .save_data_batch(key, &[entry])
                    .await?;
            }
            Some(fee_proof)
        } else {
            None
        };
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
                    fee_proof.clone(),
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
            fee_index,
            payment_memos,
        };
        Ok(memo)
    }

    pub async fn query_proposal(
        &self,
        block_builder_url: &str,
        key: KeySet,
        is_registration_block: bool,
        tx: Tx,
    ) -> Result<BlockProposal, ClientError> {
        let mut tries = 0;
        let proposal = loop {
            let proposal = self
                .block_builder
                .query_proposal(block_builder_url, is_registration_block, key.pubkey, tx)
                .await?;
            if let Some(proposal) = proposal {
                break proposal;
            }
            if tries > self.config.block_builder_query_limit {
                return Err(ClientError::FailedToGetProposal(
                    "block builder query limit exceeded".to_string(),
                ));
            }
            tries += 1;
            log::info!(
                "Failed to get proposal, retrying in {} seconds",
                self.config.block_builder_query_interval
            );
            sleep_for(self.config.block_builder_query_interval).await;
        };
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

        // verify expiry
        let current_time = chrono::Utc::now().timestamp() as u64;
        if proposal.expiry == 0 {
            return Err(ClientError::InvalidBlockProposal(
                "expiry 0 is not allowed".to_string(),
            ));
        } else if proposal.expiry < current_time {
            return Err(ClientError::InvalidBlockProposal(
                "proposal expired".to_string(),
            ));
        } else if proposal.expiry > current_time + self.config.tx_timeout {
            return Err(ClientError::InvalidBlockProposal(
                "proposal expiry too far".to_string(),
            ));
        }

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
            if Some(i as u32) == memo.fee_index {
                // ignore fee transfer because it will be saved on block builder side
                continue;
            }
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

        let transfer_uuids: Vec<String> = uuids
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

        // Save payment memo after posting signature because it's not critical data,
        // and we should reduce the time before posting the signature.
        for memo_entry in memo.payment_memos.iter() {
            let (position, transfer_data) = transfer_data_vec
                .iter()
                .enumerate()
                .find(|(_position, transfer_data)| {
                    transfer_data.transfer_index == memo_entry.transfer_index
                })
                .ok_or(ClientError::UnexpectedError(
                    "transfer_data not found".to_string(),
                ))?;
            let transfer_uuid = transfer_uuids[position].clone();
            let payment_memo = PaymentMemo {
                meta: MetaData {
                    // todo: use response from store-vault-server
                    timestamp: chrono::Utc::now().timestamp() as u64,
                    uuid: transfer_uuid.clone(),
                },
                transfer_data: transfer_data.clone(),
                memo: memo_entry.memo.clone(),
            };
            // todo: batch save
            self.store_vault_server
                .save_misc(key, memo_entry.topic, &payment_memo.encrypt(key.pubkey))
                .await?;
        }

        let result = TxResult {
            tx_tree_root: proposal.tx_tree_root,
            transfer_uuids,
            withdrawal_uuids,
            transfer_data_vec,
            withdrawal_data_vec,
        };

        Ok(result)
    }

    pub async fn get_tx_status(
        &self,
        sender: U256,
        tx_tree_root: Bytes32,
    ) -> Result<TxStatus, ClientError> {
        // get onchain info
        let block_number = self
            .validity_prover
            .get_block_number_by_tx_tree_root(tx_tree_root)
            .await?;
        if block_number.is_none() {
            return Ok(TxStatus::Pending);
        }
        let block_number = block_number.unwrap();
        let validity_witness = self
            .validity_prover
            .get_validity_witness(block_number)
            .await?;
        let validity_pis = validity_witness.to_validity_pis().map_err(|e| {
            ClientError::UnexpectedError(format!("failed to convert to validity pis: {}", e))
        })?;

        // get sender leaf
        let sender_leaf = validity_witness
            .block_witness
            .get_sender_tree()
            .leaves()
            .into_iter()
            .find(|leaf| leaf.sender == sender);
        let sender_leaf = match sender_leaf {
            Some(leaf) => leaf,
            None => return Ok(TxStatus::Failed("sender leaf not found".to_string())),
        };

        if !sender_leaf.did_return_sig {
            return Ok(TxStatus::Failed(
                "sender did'nt returned signature".to_string(),
            ));
        }

        if !validity_pis.is_valid_block {
            return Ok(TxStatus::Failed("block is not valid".to_string()));
        }

        Ok(TxStatus::Success)
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
            &ProcessStatus::default(),
            self.config.tx_timeout,
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
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<HistoryEntry<DepositData>>, MetaDataCursorResponse), ClientError> {
        fetch_deposit_history(self, key, cursor).await
    }

    pub async fn fetch_transfer_history(
        &self,
        key: KeySet,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<HistoryEntry<TransferData>>, MetaDataCursorResponse), ClientError> {
        fetch_transfer_history(self, key, cursor).await
    }

    pub async fn fetch_tx_history(
        &self,
        key: KeySet,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<HistoryEntry<TxData>>, MetaDataCursorResponse), ClientError> {
        fetch_tx_history(self, key, cursor).await
    }

    pub async fn quote_transfer_fee(
        &self,
        block_builder_url: &str,
        pubkey: U256,
        fee_token_index: u32,
    ) -> Result<FeeQuote, ClientError> {
        let account_info = self.validity_prover.get_account_info(pubkey).await?;
        let is_registration_block = account_info.account_id.is_none();
        let fee_info = self.block_builder.get_fee_info(block_builder_url).await?;
        let (fee, collateral_fee) =
            quote_transfer_fee(is_registration_block, fee_token_index, &fee_info)?;
        if fee_info.beneficiary.is_none() && fee.is_some() {
            return Err(ClientError::BlockBuilderFeeError(
                "beneficiary is required".to_string(),
            ));
        }
        if fee.is_none() && collateral_fee.is_some() {
            return Err(ClientError::BlockBuilderFeeError(
                "collateral fee is required but fee is not found".to_string(),
            ));
        }
        Ok(FeeQuote {
            beneficiary: fee_info.beneficiary,
            fee,
            collateral_fee,
        })
    }

    pub async fn quote_withdrawal_fee(
        &self,
        withdrawal_token_index: u32,
        fee_token_index: u32,
    ) -> Result<FeeQuote, ClientError> {
        let (beneficiary, fee) = quote_withdrawal_fee(
            &self.withdrawal_server,
            &self.withdrawal_contract,
            withdrawal_token_index,
            fee_token_index,
        )
        .await?;
        Ok(FeeQuote {
            beneficiary,
            fee,
            collateral_fee: None,
        })
    }

    pub async fn quote_claim_fee(&self, fee_token_index: u32) -> Result<FeeQuote, ClientError> {
        let (beneficiary, fee) = quote_claim_fee(&self.withdrawal_server, fee_token_index).await?;
        Ok(FeeQuote {
            beneficiary,
            fee,
            collateral_fee: None,
        })
    }

    pub async fn generate_withdrawal_transfers(
        &self,
        withdrawal_transfer: &Transfer,
        fee_token_index: u32,
        with_claim_fee: bool,
    ) -> Result<WithdrawalTransfers, ClientError> {
        let withdrawal_transfers = generate_withdrawal_transfers(
            &self.withdrawal_server,
            &self.withdrawal_contract,
            withdrawal_transfer,
            fee_token_index,
            with_claim_fee,
        )
        .await?;
        Ok(withdrawal_transfers)
    }
}

fn balance_check(balances: &Balances, transfers: &[Transfer]) -> Result<(), ClientError> {
    let mut balances = balances.clone();
    for transfer in transfers {
        let prev_balance = balances.get(transfer.token_index);
        let is_insufficient = balances.sub_transfer(transfer);
        if is_insufficient {
            return Err(ClientError::BalanceError(format!(
                "Insufficient balance: {} < {} for token #{}",
                prev_balance, transfer.amount, transfer.token_index
            )));
        }
    }
    Ok(())
}
