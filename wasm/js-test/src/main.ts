import { cleanEnv, num, str, url } from 'envalid';
import { Config, fetch_deposit_history, fetch_transfer_history, fetch_tx_history, generate_intmax_account_from_eth_key, get_user_data, get_withdrawal_info, JsGenericAddress, JsTransfer, JsTxRequestMemo, prepare_deposit, query_and_finalize, send_tx_request, sync, sync_withdrawals, } from '../pkg';
import { generateRandomHex } from './utils';
import { deposit, getEthBalance } from './contract';
import * as dotenv from 'dotenv';
import { ethers } from 'ethers';
dotenv.config();

const env = cleanEnv(process.env, {
  USER_ETH_PRIVATE_KEY: str(),
  ENV: str(),

  // Base URLs
  STORE_VAULT_SERVER_BASE_URL: url(),
  BALANCE_PROVER_BASE_URL: url(),
  VALIDITY_PROVER_BASE_URL: url(),
  WITHDRAWAL_SERVER_BASE_URL: url(),
  BLOCK_BUILDER_BASE_URL: url(),

  // Timeout configurations
  DEPOSIT_TIMEOUT: num(),
  TX_TIMEOUT: num(),

  // Block builder client configurations
  BLOCK_BUILDER_REQUEST_INTERVAL: num(),
  BLOCK_BUILDER_REQUEST_LIMIT: num(),
  BLOCK_BUILDER_QUERY_WAIT_TIME: num(),
  BLOCK_BUILDER_QUERY_INTERVAL: num(),
  BLOCK_BUILDER_QUERY_LIMIT: num(),

  // L1 configurations
  L1_RPC_URL: url(),
  L1_CHAIN_ID: num(),
  LIQUIDITY_CONTRACT_ADDRESS: str(),

  // L2 configurations
  L2_RPC_URL: url(),
  L2_CHAIN_ID: num(),
  ROLLUP_CONTRACT_ADDRESS: str(),
  ROLLUP_CONTRACT_DEPLOYED_BLOCK_NUMBER: num(),
});

async function main() {
  const config = new Config(
    env.STORE_VAULT_SERVER_BASE_URL,
    env.BALANCE_PROVER_BASE_URL,
    env.VALIDITY_PROVER_BASE_URL,
    env.WITHDRAWAL_SERVER_BASE_URL,
    BigInt(env.DEPOSIT_TIMEOUT),
    BigInt(env.TX_TIMEOUT),
    BigInt(env.BLOCK_BUILDER_REQUEST_INTERVAL),
    BigInt(env.BLOCK_BUILDER_REQUEST_LIMIT),
    BigInt(env.BLOCK_BUILDER_QUERY_WAIT_TIME),
    BigInt(env.BLOCK_BUILDER_QUERY_INTERVAL),
    BigInt(env.BLOCK_BUILDER_QUERY_LIMIT),
    env.L1_RPC_URL,
    BigInt(env.L1_CHAIN_ID),
    env.LIQUIDITY_CONTRACT_ADDRESS,
    env.L2_RPC_URL,
    BigInt(env.L2_CHAIN_ID),
    env.ROLLUP_CONTRACT_ADDRESS,
    BigInt(env.ROLLUP_CONTRACT_DEPLOYED_BLOCK_NUMBER),
  );

  const ethKey = env.USER_ETH_PRIVATE_KEY;
  const ethAddress = new ethers.Wallet(ethKey).address;
  console.log("ethAddress: ", ethAddress);

  // generate key
  const key = await generate_intmax_account_from_eth_key(ethKey);
  const publicKey = key.pubkey;
  const privateKey = key.privkey;
  console.log("privateKey: ", privateKey);
  console.log("publicKey: ", publicKey);

  // deposit to the account
  const tokenType = 0; // 0: native token, 1: ERC20, 2: ERC721, 3: ERC1155
  const tokenAddress = "0x0000000000000000000000000000000000000000";
  const tokenId = "0"; // Use "0" for fungible tokens
  const amount = "123"; // in wei

  const balance = await getEthBalance(ethKey, env.L1_RPC_URL);
  console.log("balance: ", balance);

  const depositResult = await prepare_deposit(config, ethAddress, publicKey, amount, tokenType, tokenAddress, tokenId, false);
  const pubkeySaltHash = depositResult.deposit_data.pubkey_salt_hash;
  console.log("pubkeySaltHash: ", pubkeySaltHash);

  await deposit(ethKey, env.L1_RPC_URL, env.LIQUIDITY_CONTRACT_ADDRESS, env.L2_RPC_URL, env.ROLLUP_CONTRACT_ADDRESS, BigInt(amount), tokenType, tokenAddress, tokenId, pubkeySaltHash, ethAddress);

  // wait for the validity prover syncs
  console.log("Waiting for the validity prover to sync...");
  await sleep(40);

  // sync the account's balance proof 
  await syncBalanceProof(config, privateKey);

  // get the account's balance
  let userData = await get_user_data(config, privateKey);
  let balances = userData.balances;
  for (let i = 0; i < balances.length; i++) {
    const balance = balances[i];
    console.log(`Token ${balance.token_index}: ${balance.amount}`);
  }

  // send a transfer tx
  const someonesKey = await generate_intmax_account_from_eth_key(generateRandomHex(32));
  const genericAddress = new JsGenericAddress(true, someonesKey.pubkey);
  const salt = generateRandomHex(32);
  const transfer = new JsTransfer(genericAddress, 0, "1", salt);

  await sendTx(config, env.BLOCK_BUILDER_BASE_URL, privateKey, [transfer]);

  // wait for the validity prover syncs
  await sleep(40);

  // get the receiver's balance
  await syncBalanceProof(config, privateKey);
  userData = await get_user_data(config, someonesKey.privkey);
  balances = userData.balances;
  for (let i = 0; i < balances.length; i++) {
    const balance = balances[i];
    console.log(`Token ${balance.token_index}: ${balance.amount}`);
  }

  // Withdrawal 
  const withdrawalEthAddress = generateRandomHex(20);
  const withdrawalTokenIndex = 0;
  const withdrawalAmount = "1";
  const withdrawalSalt = generateRandomHex(32);
  const withdrawalTransfer = new JsTransfer(new JsGenericAddress(false, withdrawalEthAddress), withdrawalTokenIndex, withdrawalAmount, withdrawalSalt);
  await sendTx(config, env.BLOCK_BUILDER_BASE_URL, privateKey, [withdrawalTransfer]);

  // wait for the validity prover syncs
  await sleep(40);

  // sync withdrawals 
  await sync_withdrawals(config, privateKey);
  console.log("Withdrawal synced");

  // print the history 
  await syncBalanceProof(config, privateKey);
  console.log("balance proof synced");

  const deposit_history = await fetch_deposit_history(config, privateKey,);
  for (let i = 0; i < deposit_history.length; i++) {
    const entry = deposit_history[i];
    console.log(`Deposit: depositor ${entry.data.depositor} of ${entry.data.amount} (#${entry.data.token_index}) at ${entry.meta.timestamp} ${entry.status.status}`);
  }
  const transfer_history = await fetch_transfer_history(config, privateKey);
  for (let i = 0; i < transfer_history.length; i++) {
    const entry = transfer_history[i];
    console.log(`Receive: sender ${entry.data.sender} of ${entry.data.transfer.amount} (#${entry.data.transfer.token_index}) at ${entry.meta.timestamp} ${entry.status.status}`);
  }
  const tx_history = await fetch_tx_history(config, privateKey);
  for (let i = 0; i < tx_history.length; i++) {
    const entry = tx_history[i];
    console.log(`Send: transfers ${entry.data.transfers.length} at ${entry.meta.timestamp} ${entry.status.status}`);
  }
  // print withdrawal status 
  const withdrawalInfo = await get_withdrawal_info(config, privateKey);
  for (let i = 0; i < withdrawalInfo.length; i++) {
    const withdrawal = withdrawalInfo[i];
    console.log("Withdrawal: ", withdrawal);
  }
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

async function sendTx(config: Config, block_builder_base_url: string, privateKey: string, transfers: JsTransfer[]) {
  console.log("Sending tx...");
  let memo: JsTxRequestMemo = await send_tx_request(config, block_builder_base_url, privateKey, transfers);
  console.log("Transfer root of tx: ", memo.tx().transfer_tree_root);

  // wait for the block builder to propose the block
  await sleep(env.BLOCK_BUILDER_QUERY_WAIT_TIME);

  await query_and_finalize(config, env.BLOCK_BUILDER_BASE_URL, privateKey, memo);
  console.log("Tx finalized");
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