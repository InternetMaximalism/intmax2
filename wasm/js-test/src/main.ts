import { await_tx_sendable, Config, fetch_deposit_history, fetch_transfer_history, fetch_tx_history, generate_fee_payment_memo, generate_intmax_account_from_eth_key, generate_withdrawal_transfers, get_tx_status, get_user_data, get_withdrawal_info, JsGenericAddress, JsMetaDataCursor, JsPaymentMemoEntry, JsTransfer, JsTxRequestMemo, prepare_deposit, query_and_finalize, quote_transfer_fee, quote_withdrawal_fee, send_tx_request, sync, sync_withdrawals, } from '../pkg';
import { generateRandomHex } from './utils';
import { deposit, getEthBalance } from './contract';
import { ethers } from 'ethers';
import { env, config } from './setup';
import { TokenType } from './token';

async function syncAndPrintBalances(config: Config, configPrivKey: string, userPrivKey: string) {
  await syncBalanceProof(config, configPrivKey);
  const userData = await get_user_data(config, userPrivKey);
  
  console.log("Synced balances:");
  for (const balance of userData.balances) {
    console.log(`Token ${balance.token_index}: ${balance.amount}`);
  }
}

async function main() {
  const ethKey = env.USER_ETH_PRIVATE_KEY;
  const ethAddress = new ethers.Wallet(ethKey).address;
  console.log("ethAddress: ", ethAddress);

  // generate key
  const { privkey: privateKey, pubkey: publicKey } = await generate_intmax_account_from_eth_key(ethKey);
  console.log("privateKey: ", privateKey);
  console.log("publicKey: ", publicKey);

  const balance = await getEthBalance(ethKey, env.L1_RPC_URL);
  console.log("eth balance: ", balance);

  // deposit to the account
  const tokenType = TokenType.Native; // 0: native token, 1: ERC20, 2: ERC721, 3: ERC1155
  const tokenAddress = "0x0000000000000000000000000000000000000000";
  const tokenId = "0"; // Use "0" for fungible tokens
  const amount = "1000000000000000"; // in wei
  await depositWrapper(config, ethKey, ethAddress, publicKey, amount, tokenType, tokenAddress, tokenId);

  // sync the account's balance proof and print the account's balance
  syncAndPrintBalances(config, privateKey, privateKey);

  // send a transfer tx
  const someonesKey = await generate_intmax_account_from_eth_key(generateRandomHex(32));
  const genericAddress = new JsGenericAddress(true, someonesKey.pubkey);

  const transfer = new JsTransfer(genericAddress, 0, "1", generateRandomHex(32));
  const feeTokenIndex = TokenType.Native; // use native token for fee

  await sendTx(config, env.BLOCK_BUILDER_BASE_URL, publicKey, privateKey, [transfer], [], feeTokenIndex);

  // wait for the validity prover syncs
  await sleep(80);

  // get the receiver's balance
  syncAndPrintBalances(config, privateKey, someonesKey.privkey);

  // Withdrawal 
  const withClaimFee = false; // set to true if you want to pay claim fee
  await sendWithdrawal(config, env.BLOCK_BUILDER_BASE_URL, publicKey, privateKey, generateRandomHex(20), 0, "1", feeTokenIndex, withClaimFee,);

  // wait for the validity prover syncs
  await sleep(80);

  // sync withdrawals 
  await sync_withdrawals(config, privateKey, feeTokenIndex);
  console.log("Withdrawal synced");

  // print the history 
  await syncBalanceProof(config, privateKey);
  console.log("balance proof synced");


  const cursor = new JsMetaDataCursor(null, "asc", null);
  const deposit_history = await fetch_deposit_history(config, privateKey, cursor);
  for (const entry of deposit_history.history) {
    console.log(`Deposit: depositor ${entry.data.depositor} of ${entry.data.amount} (#${entry.data.token_index}) at ${entry.meta.timestamp} ${entry.status.status}`);
  }
  const transfer_history = await fetch_transfer_history(config, privateKey, cursor);
  for (const entry of transfer_history.history) {
    console.log(`Receive: sender ${entry.data.sender} of ${entry.data.transfer.amount} (#${entry.data.transfer.token_index}) at ${entry.meta.timestamp} ${entry.status.status}`);
  }
  const tx_history = await fetch_tx_history(config, privateKey, cursor);
  for (const entry of tx_history.history) {
    console.log(`Send: transfers ${entry.data.transfers.length} at ${entry.meta.timestamp} ${entry.status.status}`);
  }
  // print withdrawal status 
  const withdrawalInfo = await get_withdrawal_info(config, privateKey);
  for (const withdrawal of withdrawalInfo) {
    const contract_withdrawal = withdrawal.contract_withdrawal;
    console.log(`Withdrawal: amount: ${contract_withdrawal.amount}, token_index: ${contract_withdrawal.token_index}, status: ${withdrawal.status}`);
  }
}

async function depositWrapper(config: Config, ethKey: string, ethAddress: string, publicKey: string, amount: string, tokenType: number, tokenAddress: string, tokenId: string) {
  const depositResult = await prepare_deposit(config, ethAddress, publicKey, amount, tokenType, tokenAddress, tokenId, false);
  const pubkeySaltHash = depositResult.deposit_data.pubkey_salt_hash;
  await deposit(ethKey, env.L1_RPC_URL, env.LIQUIDITY_CONTRACT_ADDRESS, env.L2_RPC_URL, env.ROLLUP_CONTRACT_ADDRESS, BigInt(amount), tokenType, tokenAddress, tokenId, pubkeySaltHash, ethAddress);
  // wait for the validity prover syncs
  console.log("Waiting for the validity prover to sync...");
  await sleep(40);
}

async function syncBalanceProof(config: Config, privateKey: string) {
  console.log("syncing balance proof...");
  while (true) {
    try {
      await sync(config, privateKey);
      break;
    } catch (error) {
      console.log("Error syncing balance proof: ", error, "retrying...");
    }
    await sleep(10);
  }
  console.log("balance proof synced");
}

async function sendTx(config: Config, block_builder_base_url: string, publicKey: string, privateKey: string, transfers: JsTransfer[], payment_memos: JsPaymentMemoEntry[], feeTokenIndex: number) {
  console.log("Sending tx...");
  const fee_quote = await quote_transfer_fee(config, block_builder_base_url, publicKey, feeTokenIndex);
  console.log("Fee beneficiary: ", fee_quote.beneficiary);
  console.log("Fee: ", fee_quote.fee?.amount);
  console.log("Collateral fee: ", fee_quote.collateral_fee?.amount);

  console.log("Waiting for tx sendable...");
  await await_tx_sendable(config, privateKey, transfers, fee_quote);
  console.log("Tx sendable");

  let memo: JsTxRequestMemo = await send_tx_request(config, block_builder_base_url, privateKey, transfers, payment_memos, fee_quote);
  console.log("Transfer root of tx: ", memo.tx().transfer_tree_root);
  // wait for the block builder to propose the block
  await sleep(env.BLOCK_BUILDER_QUERY_WAIT_TIME);
  const result = await query_and_finalize(config, env.BLOCK_BUILDER_BASE_URL, privateKey, memo);

  const tx_tree_root = result.tx_tree_root;
  while (true) {
    let status = await get_tx_status(config, publicKey, tx_tree_root);
    if (status === "success") {
      break;
    } else if (status === "failed") {
      throw new Error("tx failed")
    }
    console.log("Tx status: ", status,);
    await sleep(10);
  }

  console.log("Tx success");
}

async function sendWithdrawal(config: Config, block_builder_base_url: string, publicKey: string, privateKey: string, ethAddress: string, tokenIndex: number, amount: string, feeTokenIndex: number, withClaimFee: boolean) {
  console.log("Sending withdrawal tx...");
  const withdrawalTransfer = new JsTransfer(new JsGenericAddress(false, ethAddress), tokenIndex, amount, generateRandomHex(32));
  const fee_quote = await quote_withdrawal_fee(config, tokenIndex, feeTokenIndex);
  console.log("Withdrawal fee beneficiary: ", fee_quote.beneficiary);
  console.log("Withdrawal fee quote: ", fee_quote.fee?.amount);
  const withdrawalTransfers = await generate_withdrawal_transfers(config, withdrawalTransfer, feeTokenIndex, withClaimFee);
  const paymentMemos = generate_fee_payment_memo(withdrawalTransfers.transfers, withdrawalTransfers.withdrawal_fee_transfer_index, withdrawalTransfers.claim_fee_transfer_index);
  await sendTx(config, block_builder_base_url, publicKey, privateKey, withdrawalTransfers.transfers, paymentMemos, feeTokenIndex);
}

async function sleep(sec: number) {
  return new Promise((resolve) => setTimeout(resolve, sec * 1000));
}

main().then(() => {
  process.exit(0);
}).catch((err) => {
  console.error(err);
  process.exit(1);
});