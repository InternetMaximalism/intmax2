use intmax2_interfaces::{
    api::{
        balance_prover::interface::BalanceProverClientInterface,
        block_builder::interface::BlockBuilderClientInterface,
        store_vault_server::{
            interface::{SaveDataEntry, StoreVaultClientInterface},
            types::{MetaDataCursor, MetaDataCursorResponse},
        },
        validity_prover::interface::ValidityProverClientInterface,
        withdrawal_server::interface::{
            ClaimInfo, WithdrawalInfo, WithdrawalServerClientInterface,
        },
    },
    data::{
        data_type::DataType,
        deposit_data::{DepositData, TokenType},
        encryption::BlsEncryption as _,
        extra_data::FullExtraData,
        meta_data::MetaData,
        proof_compression::{CompressedBalanceProof, CompressedSpentProof},
        sender_proof_set::SenderProofSet,
        transfer_data::TransferData,
        transfer_type::TransferType,
        tx_data::TxData,
        user_data::{Balances, ProcessStatus, UserData},
    },
    utils::{
        address::IntmaxAddress,
        circuit_verifiers::CircuitVerifiers,
        digest::get_digest,
        key::{KeyPair, PrivateKey, PublicKey, PublicKeyPair, ViewPair},
        random::default_rng,
    },
};
use intmax2_zkp::{
    circuits::validity::validity_pis::ValidityPublicInputs,
    common::{
        block_builder::BlockProposal, deposit::get_pubkey_salt_hash, transfer::Transfer,
        trees::transfer_tree::TransferTree,
    },
    constants::{NUM_TRANSFERS_IN_TX, TRANSFER_TREE_HEIGHT},
    ethereum_types::{address::Address, bytes32::Bytes32, u256::U256},
};

use super::{
    backup::make_history_backup,
    config::ClientConfig,
    error::ClientError,
    fee_payment::{
        quote_claim_fee, quote_withdrawal_fee, WithdrawalTransferRequests, CLAIM_FEE_MEMO,
        WITHDRAWAL_FEE_MEMO,
    },
    fee_proof::{generate_fee_proof, quote_transfer_fee},
    history::{fetch_deposit_history, fetch_transfer_history, fetch_tx_history, HistoryEntry},
    misc::payment_memo::{payment_memo_topic, PaymentMemo},
    receipt::validate_transfer_receipt,
    strategy::{
        mining::{fetch_mining_info, Mining},
        strategy::determine_sequence,
        tx::fetch_all_unprocessed_tx_info,
        tx_status::{get_tx_status, TxStatus},
    },
    sync::utils::{generate_spent_witness, get_balance_proof},
};
use crate::{
    client::{
        fee_payment::generate_withdrawal_transfers,
        receipt::generate_transfer_receipt,
        strategy::{
            mining::validate_mining_deposit_criteria, utils::wait_till_validity_prover_synced,
        },
        sync::utils::generate_salt,
        types::{
            DepositResult, FeeQuote, GenericRecipient, PaymentMemoEntry, TransferFeeQuote,
            TransferRequest, TxRequestMemo, TxResult,
        },
    },
    external_api::{
        contract::{
            liquidity_contract::LiquidityContract, rollup_contract::RollupContract,
            withdrawal_contract::WithdrawalContract,
        },
        local_backup_store_vault::diff_data_client::make_backup_csv_from_entries,
        utils::time::sleep_for,
    },
};

// Buffer time for the expiry of the block proposal
// This is to prevent "expiry too far" error when the client time is not synced with the server time
const EXPIRY_BUFFER: u64 = 60;

pub struct Client {
    pub config: ClientConfig,

    pub block_builder: Box<dyn BlockBuilderClientInterface>,
    pub store_vault_server: Box<dyn StoreVaultClientInterface>,
    pub validity_prover: Box<dyn ValidityProverClientInterface>,
    pub balance_prover: Box<dyn BalanceProverClientInterface>,
    pub withdrawal_server: Box<dyn WithdrawalServerClientInterface>,

    pub liquidity_contract: LiquidityContract,
    pub rollup_contract: RollupContract,
    pub withdrawal_contract: WithdrawalContract,
}

impl Client {
    pub fn get_address(&self, public_keypair: PublicKeyPair) -> IntmaxAddress {
        IntmaxAddress::from_public_keypair(self.config.network, &public_keypair)
    }

    /// Back up deposit information before calling the contract's deposit function
    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_deposit(
        &self,
        depositor: Address,
        public_keypair: PublicKeyPair,
        amount: U256,
        token_type: TokenType,
        token_address: Address,
        token_id: U256,
        is_mining: bool,
    ) -> Result<DepositResult, ClientError> {
        log::info!(
            "prepare_deposit: spend pub {}, amount {amount}, token_type {token_type:?}, token_address {token_address}, token_id {token_id}",
            public_keypair.spend,
        );
        if is_mining && !validate_mining_deposit_criteria(token_type, amount) {
            return Err(ClientError::InvalidMiningDepositCriteria);
        }

        let deposit_salt = generate_salt();

        // backup before contract call
        let pubkey_salt_hash = get_pubkey_salt_hash(public_keypair.spend.0, deposit_salt);
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
            topic: DataType::Deposit.to_topic(),
            pubkey: public_keypair.view.0,
            data: deposit_data.encrypt(public_keypair.view, None)?,
        };
        let ephemeral_key = PrivateKey::rand(&mut default_rng());
        let digests = self
            .store_vault_server
            .save_data_batch(ephemeral_key, std::slice::from_ref(&save_entry))
            .await?;
        let deposit_digest = *digests.first().ok_or(ClientError::UnexpectedError(
            "deposit_digest not found".to_string(),
        ))?;
        let backup_csv = make_backup_csv_from_entries(&[save_entry])
            .map_err(|e| ClientError::BackupError(format!("Failed to make backup csv: {e}")))?;
        let result = DepositResult {
            deposit_data,
            deposit_digest,
            backup_csv,
        };
        Ok(result)
    }

    /// Check balance and await for both balance proof and validity proof synced
    pub async fn await_tx_sendable(
        &self,
        view_pair: ViewPair,
        transfer_requests: &[TransferRequest],
        fee_quote: &TransferFeeQuote,
    ) -> Result<UserData, ClientError> {
        // input validation
        if transfer_requests.is_empty() {
            return Err(ClientError::TransferLenError(
                "transfers is empty".to_string(),
            ));
        }
        if transfer_requests.len() > NUM_TRANSFERS_IN_TX - 1 {
            return Err(ClientError::TransferLenError(
                "transfers is too many".to_string(),
            ));
        }
        // network validation
        for transfer_request in transfer_requests {
            match &transfer_request.recipient {
                GenericRecipient::IntmaxAddress(intmax_address) => {
                    if intmax_address.network != self.config.network {
                        return Err(ClientError::NetworkMismatch(format!(
                            "Recipient network {} does not match client network {}",
                            intmax_address.network, self.config.network
                        )));
                    }
                }
                GenericRecipient::Address(_) => {
                    // Address is not network specific, so no check needed
                }
            }
        }
        // fee quote validation
        if fee_quote.beneficiary.network != self.config.network {
            return Err(ClientError::NetworkMismatch(format!(
                "Fee beneficiary network {} does not match client network {}",
                fee_quote.beneficiary.network, self.config.network
            )));
        }
        // balance check
        let mut transfer_amounts = transfer_requests
            .iter()
            .map(|t| (t.token_index, t.amount))
            .collect::<Vec<_>>();
        if let Some(fee) = &fee_quote.fee {
            transfer_amounts.push((fee.token_index, fee.amount));
        }
        let collateral_amounts = if let Some(collateral_fee) = &fee_quote.collateral_fee {
            vec![(collateral_fee.token_index, collateral_fee.amount)]
        } else {
            vec![]
        };
        let mut user_data = self.get_user_data(view_pair).await?;
        let mut already_synced = false;

        match balance_check(&user_data.balances(), &transfer_amounts) {
            Ok(_) => {}
            Err(_) => {
                // if balance check failed, sync and retry
                log::warn!("Balance for transfers is not enough, start to sync");
                self.sync(view_pair).await?;
                already_synced = true;
                user_data = self.get_user_data(view_pair).await?;

                // check again
                balance_check(&user_data.balances(), &transfer_amounts)?;
            }
        }
        match balance_check(&user_data.balances(), &collateral_amounts) {
            Ok(_) => {}
            Err(_) => {
                // if balance check failed, sync and retry
                if !already_synced {
                    log::warn!("Balance for collateral transfer is not enough, start to sync");
                    self.sync(view_pair).await?;
                    user_data = self.get_user_data(view_pair).await?;
                }
                // check again
                balance_check(&user_data.balances(), &collateral_amounts)?;
            }
        }

        // wait for sync
        let onchain_block_number = self.rollup_contract.get_latest_block_number().await?;
        wait_till_validity_prover_synced(
            self.validity_prover.as_ref(),
            false,
            onchain_block_number,
        )
        .await?;

        let current_time = chrono::Utc::now().timestamp() as u64;
        let tx_info = fetch_all_unprocessed_tx_info(
            self.store_vault_server.as_ref(),
            self.validity_prover.as_ref(),
            view_pair,
            current_time,
            &user_data.tx_status,
            self.config.tx_timeout,
        )
        .await?;
        if !tx_info.settled.is_empty() || !tx_info.pending.is_empty() {
            if already_synced {
                return Err(ClientError::UnexpectedError(
                    "There are unsynced txs, but already synced".to_string(),
                ));
            }
            // error here is there are pending txs
            self.sync(view_pair).await?;
            user_data = self.get_user_data(view_pair).await?;
        }
        Ok(user_data)
    }

    /// Send a transaction request to the block builder
    pub async fn send_tx_request(
        &self,
        block_builder_url: &str,
        key_pair: KeyPair,
        transfer_requests: &[TransferRequest],
        payment_memos: &[PaymentMemoEntry],
        fee_quote: &TransferFeeQuote,
    ) -> Result<TxRequestMemo, ClientError> {
        let view_pair: ViewPair = key_pair.into();
        let sender_address = self.get_address(view_pair.into());
        let spend_pub = key_pair.spend.to_public_key();
        log::info!(
            "send_tx_request: spend_pub {}, transfers {}, fee_beneficiary {}, fee {:?}, collateral_fee {:?}",
           spend_pub,
            transfer_requests.len(),
            fee_quote.beneficiary,
            fee_quote.fee,
            fee_quote.collateral_fee
        );
        for e in payment_memos {
            if e.transfer_index as usize >= transfer_requests.len() {
                return Err(ClientError::PaymentMemoError(
                    "memo.transfer_index is out of range".to_string(),
                ));
            }
        }

        let user_data = self
            .await_tx_sendable(view_pair, transfer_requests, fee_quote)
            .await?;

        // fetch if this is first time tx
        let account_info = self
            .validity_prover
            .get_account_info(view_pair.spend.0)
            .await?;
        let is_registration_block = account_info.account_id.is_none();
        let mut transfer_requests = transfer_requests.to_vec();
        if let Some(fee) = &fee_quote.fee {
            // if fee is specified, add fee transfer request
            transfer_requests.push(TransferRequest {
                recipient: GenericRecipient::IntmaxAddress(fee_quote.beneficiary),
                token_index: fee.token_index,
                amount: fee.amount,
                description: None,
            });
        }
        let fee_index = if fee_quote.fee.is_some() {
            // the fee transfer is the last one
            Some(transfer_requests.len() as u32 - 1)
        } else {
            None
        };

        let collateral_transfer = fee_quote.collateral_fee.clone().map(|fee| Transfer {
            recipient: fee_quote.beneficiary.public_spend.0.into(),
            amount: fee.amount,
            token_index: fee.token_index,
            salt: generate_salt(),
        });
        let (transfers, full_extra_data): (Vec<Transfer>, Vec<FullExtraData>) = transfer_requests
            .iter()
            .map(|t| t.to_transfer_and_full_extra_data())
            .unzip();
        let recipients = transfer_requests
            .iter()
            .map(|t| t.recipient.clone())
            .collect::<Vec<_>>();
        let balance_proof =
            get_balance_proof(&user_data)?.ok_or(ClientError::CannotSendTxByZeroBalanceAccount)?;
        // generate spent proof
        let tx_nonce = user_data.full_private_state.nonce;
        let spent_witness =
            generate_spent_witness(&user_data.full_private_state, tx_nonce, &transfers)?;
        let spent_proof = self
            .balance_prover
            .prove_spent(view_pair.view, &spent_witness)
            .await?;
        let tx = spent_witness.tx;

        // save sender proof set in advance to avoid delay
        let spent_proof = CompressedSpentProof::new(&spent_proof)?;
        let prev_balance_proof = CompressedBalanceProof::new(&balance_proof)?;
        let sender_proof_set = SenderProofSet {
            spent_proof,
            prev_balance_proof,
        };
        let ephemeral_key = PrivateKey::rand(&mut default_rng());
        self.store_vault_server
            .save_snapshot(
                ephemeral_key,
                &DataType::SenderProofSet.to_topic(),
                None,
                &sender_proof_set.encrypt(ephemeral_key.to_public_key(), Some(ephemeral_key))?,
            )
            .await?;
        let fee_proof = if let Some(fee_index) = fee_index {
            let fee_proof = generate_fee_proof(
                self.store_vault_server.as_ref(),
                self.balance_prover.as_ref(),
                self.config.tx_timeout,
                key_pair,
                &user_data,
                ephemeral_key,
                tx_nonce,
                fee_index,
                &transfers,
                collateral_transfer,
                is_registration_block,
                fee_quote.block_builder_address,
            )
            .await?;
            Some(fee_proof)
        } else {
            None
        };
        // send tx request
        let request_id = self
            .block_builder
            .send_tx_request(
                block_builder_url,
                is_registration_block,
                sender_address,
                tx,
                fee_proof.clone(),
            )
            .await?;
        let memo = TxRequestMemo {
            request_id,
            is_registration_block,
            tx,
            transfers,
            recipients,
            spent_witness,
            sender_proof_set_ephemeral_key: ephemeral_key.0,
            fee_index,
            payment_memos: payment_memos.to_vec(),
            full_extra_data: full_extra_data.to_vec(),
        };
        Ok(memo)
    }

    pub async fn query_proposal(
        &self,
        block_builder_url: &str,
        request_id: &str,
    ) -> Result<BlockProposal, ClientError> {
        let mut tries = 0;
        let proposal = loop {
            let proposal = self
                .block_builder
                .query_proposal(block_builder_url, request_id)
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
        key_pair: KeyPair,
        memo: &TxRequestMemo,
        proposal: &BlockProposal,
    ) -> Result<TxResult, ClientError> {
        let view_pair: ViewPair = key_pair.into();
        let sender_priv = key_pair.spend;
        let sender_pub = key_pair.spend.to_public_key();

        // verify proposal
        proposal
            .verify(memo.tx)
            .map_err(|e| ClientError::InvalidBlockProposal(format!("{e}")))?;

        // verify expiry
        let current_time = chrono::Utc::now().timestamp() as u64;
        let expiry: u64 = proposal.block_sign_payload.expiry.into();
        if expiry == 0 {
            return Err(ClientError::InvalidBlockProposal(
                "expiry 0 is not allowed".to_string(),
            ));
        } else if expiry < current_time {
            return Err(ClientError::InvalidBlockProposal(
                "proposal expired".to_string(),
            ));
        } else if expiry > current_time + self.config.tx_timeout + EXPIRY_BUFFER {
            return Err(ClientError::InvalidBlockProposal(format!(
                "proposal expiry {} is too far: current time {}, timeout {}, buffer {}",
                expiry, current_time, self.config.tx_timeout, EXPIRY_BUFFER
            )));
        }

        // save transfer data
        let mut transfer_tree = TransferTree::new(TRANSFER_TREE_HEIGHT);
        for transfer in &memo.transfers {
            transfer_tree.push(*transfer);
        }

        let mut transfer_data_and_encrypted_data = Vec::new();
        for i in 0..memo.transfers.len() {
            let transfer = &memo.transfers[i];
            let full_extra_data = &memo.full_extra_data[i];
            let recipient = &memo.recipients[i];
            let transfer_merkle_proof = transfer_tree.prove(i as u64);
            let transfer_data = TransferData {
                sender: view_pair.into(),
                transfer: *transfer,
                transfer_index: i as u32,
                transfer_merkle_proof,
                sender_proof_set_ephemeral_key: memo.sender_proof_set_ephemeral_key,
                sender_proof_set: None,
                extra_data: full_extra_data.to_extra_data(),
                tx: memo.tx,
                tx_index: proposal.tx_index,
                tx_merkle_proof: proposal.tx_merkle_proof.clone(),
                tx_tree_root: proposal.block_sign_payload.tx_tree_root,
            };
            let data_type = if transfer.recipient.is_pubkey {
                DataType::Transfer
            } else {
                DataType::Withdrawal
            };
            let receiver_view_pub = match recipient {
                GenericRecipient::IntmaxAddress(intmax_address) => {
                    // use view key for IntmaxAddress
                    intmax_address.public_view
                }
                GenericRecipient::Address(_) => {
                    // use the sender's view key for withdrawals
                    view_pair.view.to_public_key()
                }
            };
            let sender_key = if data_type == DataType::Withdrawal {
                Some(view_pair.view)
            } else {
                None
            };
            let encrypted_data = transfer_data.encrypt(receiver_view_pub, sender_key)?;
            let digest = get_digest(&encrypted_data);
            transfer_data_and_encrypted_data.push((
                data_type,
                receiver_view_pub,
                transfer_data,
                encrypted_data,
                digest,
            ));
        }

        let transfer_digests = transfer_data_and_encrypted_data
            .iter()
            .map(|(_, _, _, _, digest)| *digest)
            .collect::<Vec<_>>();
        let transfer_data_vec = transfer_data_and_encrypted_data
            .iter()
            .map(|(_, _, transfer_data, _, _)| transfer_data.clone())
            .collect::<Vec<_>>();

        // get transfer types
        let mut transfer_types = transfer_data_and_encrypted_data
            .iter()
            .map(|(data_type, _, _, _, _)| {
                if data_type == &DataType::Withdrawal {
                    TransferType::Withdrawal
                } else {
                    // temporary placement
                    TransferType::Normal
                }
            })
            .collect::<Vec<_>>();
        if let Some(fee_index) = memo.fee_index {
            transfer_types[fee_index as usize] = TransferType::TransferFee;
        }
        for payment_memo in &memo.payment_memos {
            if payment_memo.topic == payment_memo_topic(WITHDRAWAL_FEE_MEMO) {
                transfer_types[payment_memo.transfer_index as usize] = TransferType::WithdrawalFee;
            }
            if payment_memo.topic == payment_memo_topic(CLAIM_FEE_MEMO) {
                transfer_types[payment_memo.transfer_index as usize] = TransferType::ClaimFee;
            }
        }
        let transfer_types = transfer_types
            .into_iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>();

        let mut entries = vec![];
        for (data_type, receiver, transfer_data, encrypted_data, _) in
            &transfer_data_and_encrypted_data
        {
            if Some(transfer_data.transfer_index) == memo.fee_index {
                // ignore fee transfer because it will be saved on block builder side
                continue;
            }
            entries.push(SaveDataEntry {
                topic: data_type.to_topic(),
                pubkey: receiver.0,
                data: encrypted_data.clone(),
            });
        }

        let tx_data = TxData {
            tx_index: proposal.tx_index,
            tx_merkle_proof: proposal.tx_merkle_proof.clone(),
            tx_tree_root: proposal.block_sign_payload.tx_tree_root,
            spent_witness: memo.spent_witness.clone(),
            full_extra_data: memo.full_extra_data.clone(),
            transfer_digests,
            transfer_types,
            sender_proof_set_ephemeral_key: memo.sender_proof_set_ephemeral_key,
        };
        let tx_data_encrypted =
            tx_data.encrypt(view_pair.view.to_public_key(), Some(view_pair.view))?;
        let tx_digest = get_digest(&tx_data_encrypted);
        entries.push(SaveDataEntry {
            topic: DataType::Tx.to_topic(),
            pubkey: view_pair.view.to_public_key().0,
            data: tx_data_encrypted,
        });

        self.store_vault_server
            .save_data_batch(view_pair.view, &entries)
            .await?;

        // sign and post signature
        let signature = proposal.sign(sender_priv.to_key_set());
        self.block_builder
            .post_signature(
                block_builder_url,
                &memo.request_id,
                sender_pub.0,
                signature.signature,
            )
            .await?;

        // Save payment memo after posting signature because it's not critical data,
        // and we should reduce the time before posting the signature.
        let mut misc_entries = Vec::new();
        for memo_entry in memo.payment_memos.iter() {
            let (transfer_data, digest) = transfer_data_and_encrypted_data
                .iter()
                .find_map(|(_, _, transfer_data, _, digest)| {
                    if transfer_data.transfer_index == memo_entry.transfer_index {
                        Some((transfer_data, *digest))
                    } else {
                        None
                    }
                })
                .ok_or(ClientError::UnexpectedError(
                    "transfer_data not found".to_string(),
                ))?;
            let payment_memo = PaymentMemo {
                meta: MetaData {
                    timestamp: chrono::Utc::now().timestamp() as u64,
                    digest,
                },
                transfer_data: transfer_data.clone(),
                memo: memo_entry.memo.clone(),
            };
            let entry = SaveDataEntry {
                topic: memo_entry.topic.clone(),
                pubkey: view_pair.view.to_public_key().0,
                data: payment_memo.encrypt(view_pair.view.to_public_key(), Some(view_pair.view))?,
            };
            misc_entries.push(entry);
        }
        self.store_vault_server
            .save_data_batch(view_pair.view, &misc_entries)
            .await?;

        let all_entries = entries
            .into_iter()
            .chain(misc_entries.into_iter())
            .collect::<Vec<_>>();
        let backup_csv = make_backup_csv_from_entries(&all_entries)
            .map_err(|e| ClientError::BackupError(format!("Failed to make backup csv: {e}")))?;

        let result = TxResult {
            tx_tree_root: proposal.block_sign_payload.tx_tree_root,
            tx_digest,
            tx_data,
            transfer_data_vec,
            backup_csv,
        };

        Ok(result)
    }

    pub async fn get_tx_status(
        &self,
        sender: PublicKey,
        tx_tree_root: Bytes32,
    ) -> Result<TxStatus, ClientError> {
        let status = get_tx_status(self.validity_prover.as_ref(), sender, tx_tree_root).await?;
        Ok(status)
    }

    pub async fn get_withdrawal_info(
        &self,
        view_pair: ViewPair,
    ) -> Result<Vec<WithdrawalInfo>, ClientError> {
        let withdrawal_info = self
            .withdrawal_server
            .get_withdrawal_info(view_pair)
            .await?;
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

    pub async fn get_mining_list(&self, view_pair: ViewPair) -> Result<Vec<Mining>, ClientError> {
        let current_time = chrono::Utc::now().timestamp() as u64;
        let minings = fetch_mining_info(
            self.store_vault_server.as_ref(),
            self.validity_prover.as_ref(),
            &self.liquidity_contract,
            view_pair,
            self.config.is_faster_mining,
            current_time,
            &ProcessStatus::default(),
            self.config.tx_timeout,
            self.config.deposit_timeout,
        )
        .await?;
        Ok(minings)
    }

    pub async fn get_claim_info(&self, view_pair: ViewPair) -> Result<Vec<ClaimInfo>, ClientError> {
        let claim_info = self.withdrawal_server.get_claim_info(view_pair).await?;
        Ok(claim_info)
    }

    pub async fn fetch_deposit_history(
        &self,
        view_pair: ViewPair,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<HistoryEntry<DepositData>>, MetaDataCursorResponse), ClientError> {
        fetch_deposit_history(self, view_pair, cursor).await
    }

    pub async fn fetch_transfer_history(
        &self,
        view_pair: ViewPair,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<HistoryEntry<TransferData>>, MetaDataCursorResponse), ClientError> {
        fetch_transfer_history(self, view_pair, cursor).await
    }

    pub async fn fetch_tx_history(
        &self,
        view_pair: ViewPair,
        cursor: &MetaDataCursor,
    ) -> Result<(Vec<HistoryEntry<TxData>>, MetaDataCursorResponse), ClientError> {
        fetch_tx_history(self, view_pair, cursor).await
    }

    pub async fn quote_transfer_fee(
        &self,
        block_builder_url: &str,
        pubkey: U256,
        fee_token_index: u32,
    ) -> Result<TransferFeeQuote, ClientError> {
        let account_info = self.validity_prover.get_account_info(pubkey).await?;
        let is_registration_block = account_info.account_id.is_none();
        let fee_info = self.block_builder.get_fee_info(block_builder_url).await?;
        let (fee, collateral_fee) =
            quote_transfer_fee(is_registration_block, fee_token_index, &fee_info)?;
        if fee.is_none() && collateral_fee.is_some() {
            return Err(ClientError::BlockBuilderFeeError(
                "collateral fee is required but fee is not found".to_string(),
            ));
        }
        Ok(TransferFeeQuote {
            beneficiary: fee_info.beneficiary,
            fee,
            collateral_fee,
            block_builder_address: fee_info.block_builder_address,
        })
    }

    pub async fn quote_withdrawal_fee(
        &self,
        withdrawal_token_index: u32,
        fee_token_index: u32,
    ) -> Result<FeeQuote, ClientError> {
        let (beneficiary, fee) = quote_withdrawal_fee(
            self.withdrawal_server.as_ref(),
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
        let (beneficiary, fee) =
            quote_claim_fee(self.withdrawal_server.as_ref(), fee_token_index).await?;
        Ok(FeeQuote {
            beneficiary,
            fee,
            collateral_fee: None,
        })
    }

    pub async fn generate_withdrawal_transfers(
        &self,
        withdrawal_transfer_request: &TransferRequest,
        fee_token_index: u32,
        with_claim_fee: bool,
    ) -> Result<WithdrawalTransferRequests, ClientError> {
        let withdrawal_transfers = generate_withdrawal_transfers(
            self.withdrawal_server.as_ref(),
            &self.withdrawal_contract,
            withdrawal_transfer_request,
            fee_token_index,
            with_claim_fee,
        )
        .await?;
        Ok(withdrawal_transfers)
    }

    pub async fn make_history_backup(
        &self,
        view_pair: ViewPair,
        from: u64,
        chunk_size: usize,
    ) -> Result<Vec<String>, ClientError> {
        let csvs = make_history_backup(self, view_pair, from, chunk_size).await?;
        Ok(csvs)
    }

    pub async fn generate_transfer_receipt(
        &self,
        view_pair: ViewPair,
        tx_digest: Bytes32,
        transfer_index: u32,
    ) -> Result<String, ClientError> {
        generate_transfer_receipt(self, view_pair, tx_digest, transfer_index).await
    }

    pub async fn validate_transfer_receipt(
        &self,
        view_pair: ViewPair,
        transfer_receipt: &str,
    ) -> Result<TransferData, ClientError> {
        validate_transfer_receipt(self, view_pair, transfer_receipt).await
    }

    pub async fn get_balances_without_sync(
        &self,
        view_pair: ViewPair,
    ) -> Result<Balances, ClientError> {
        let (_, balances, _) = determine_sequence(
            self.store_vault_server.as_ref(),
            self.validity_prover.as_ref(),
            &self.rollup_contract,
            &self.liquidity_contract,
            view_pair,
            self.config.deposit_timeout,
            self.config.tx_timeout,
        )
        .await?;
        Ok(balances)
    }

    pub async fn check_validity_prover(&self) -> Result<(), ClientError> {
        let onchain_block_number = self.rollup_contract.get_latest_block_number().await?;
        wait_till_validity_prover_synced(self.validity_prover.as_ref(), true, onchain_block_number)
            .await?;
        log::info!("validity prover is synced for onchain block {onchain_block_number}");
        let validity_proof = self
            .validity_prover
            .get_validity_proof(onchain_block_number)
            .await?;
        let verifier = CircuitVerifiers::load().get_validity_vd();
        verifier.verify(validity_proof.clone()).map_err(|e| {
            ClientError::ValidityProverError(format!("Failed to verify validity proof: {e}"))
        })?;
        let validity_pis =
            ValidityPublicInputs::from_pis(&validity_proof.public_inputs).map_err(|e| {
                ClientError::ValidityProverError(format!("Failed to parse validity proof pis: {e}"))
            })?;
        let onchain_block_hash = self
            .rollup_contract
            .get_block_hash(onchain_block_number)
            .await?;
        if validity_pis.public_state.block_hash != onchain_block_hash {
            return Err(ClientError::ValidityProverError(format!(
                "Invalid block hash: validity prover {} != onchain {}",
                validity_pis.public_state.block_hash, onchain_block_hash
            )));
        }
        log::info!("validity proof is valid");
        Ok(())
    }
}

fn balance_check(balances: &Balances, amounts: &[(u32, U256)]) -> Result<(), ClientError> {
    let mut balances = balances.clone();
    for (token_index, amount) in amounts {
        let prev_balance = balances.get(*token_index);
        let is_insufficient = balances.sub_token(*token_index, *amount);
        if is_insufficient {
            return Err(ClientError::BalanceError(format!(
                "Insufficient balance: {prev_balance} < {amount} for token #{token_index}"
            )));
        }
    }
    Ok(())
}
