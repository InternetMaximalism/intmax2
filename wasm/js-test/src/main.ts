import { cleanEnv, num, str, url } from 'envalid';
import { Config, finalize_tx, generate_intmax_account_from_eth_key, get_user_data, JsBlockProposal, JsGenericAddress, JsTransfer, JsTxRequestMemo, prepare_deposit, query_proposal, send_tx_request, sync, sync_withdrawals, } from '../pkg';
import { postEmptyBlock, } from './state-manager';
import { generateRandomHex } from './utils';
import { printHistory } from './history';
import { deposit, getEthBalance } from './contract';
import * as dotenv from 'dotenv';
dotenv.config();

const env = cleanEnv(process.env, {
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
    env.L1_RPC_URL,
    BigInt(env.L1_CHAIN_ID),
    env.LIQUIDITY_CONTRACT_ADDRESS,
    env.L2_RPC_URL,
    BigInt(env.L2_CHAIN_ID),
    env.ROLLUP_CONTRACT_ADDRESS,
    BigInt(env.ROLLUP_CONTRACT_DEPLOYED_BLOCK_NUMBER),
  );

  // generate key
  const key = await generate_intmax_account_from_eth_key(generateRandomHex(32));
  const publicKey = key.pubkey;
  const privateKey = key.privkey;
  console.log("privateKey: ", privateKey);
  console.log("publicKey: ", publicKey);

  // One of default anvil keys
  const ethKey = "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6"

  // deposit to the account
  const tokenType = 0;
  const tokenAddress = "0x0000000000000000000000000000000000000000";
  const tokenId = "0";
  const amount = "123"; // in wei

  const balance = await getEthBalance(ethKey, env.L1_RPC_URL);
  console.log("balance: ", balance);

  // const pubkeySaltHash = await prepare_deposit(config, publicKey, amount, tokenType, tokenAddress, tokenId);
  // console.log("pubkeySaltHash: ", pubkeySaltHash);
  const pubkeySaltHash = ethKey;

  await deposit(ethKey, env.L1_RPC_URL, env.LIQUIDITY_CONTRACT_ADDRESS, env.L2_RPC_URL, env.ROLLUP_CONTRACT_ADDRESS, BigInt(amount), tokenType, tokenAddress, tokenId, pubkeySaltHash);

  await postEmptyBlock(env.BLOCK_BUILDER_BASE_URL); // block builder post empty block (this is not used in production)

  // wait for the validity prover syncs
  await sleep(80);

  // sync the account's balance proof 
  await sync(config, privateKey);
  console.log("balance proof synced");

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
  await sleep(80);

  // get the receiver's balance
  await sync(config, someonesKey.privkey);
  console.log("balance proof synced");
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
  await sleep(80);

  // sync withdrawals 
  await sync_withdrawals(config, privateKey);
  console.log("Withdrawal synced");

  // print the history 
  await sync(config, privateKey);
  console.log("balance proof synced");
  userData = await get_user_data(config, privateKey);
  await printHistory(env.STORE_VAULT_SERVER_BASE_URL, privateKey, userData);
}

async function sendTx(config: Config, block_builder_base_url: string, privateKey: string, transfers: JsTransfer[]) {
  let memo: JsTxRequestMemo | undefined = undefined;
  for (let i = 0; i < env.BLOCK_BUILDER_REQUEST_LIMIT; i++) {
    try {
      memo = await send_tx_request(config, block_builder_base_url, privateKey, transfers);
      break;
    } catch (error) {
      console.log("Error sending tx request: ", error, "retrying...");
    }
    await sleep(env.BLOCK_BUILDER_REQUEST_INTERVAL);
  }
  if (!memo) {
    throw new Error("Failed to send tx request after all retries");
  }

  const tx = memo.tx();
  const isRegistrationBlock = memo.is_registration_block();

  // wait for the block builder to propose the block
  await sleep(env.BLOCK_BUILDER_QUERY_WAIT_TIME);

  // query the block proposal
  let proposal: JsBlockProposal | undefined = undefined;
  for (let i = 0; i < env.BLOCK_BUILDER_QUERY_LIMIT; i++) {
    const proposal = await query_proposal(config, block_builder_base_url, privateKey, isRegistrationBlock, tx);
    if (proposal) {
      break;
    }
    console.log("No proposal found, retrying...");
    await sleep(env.BLOCK_BUILDER_QUERY_INTERVAL);
  }
  if (!proposal) {
    throw new Error("No proposal found");
  }
  // finalize the tx
  await finalize_tx(config, env.BLOCK_BUILDER_BASE_URL, privateKey, memo, proposal);
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