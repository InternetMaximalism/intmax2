import * as dotenv from 'dotenv';
import { cleanEnv, num, str, url } from 'envalid';
import { Config, generate_intmax_account_from_eth_key, get_withdrawal_info } from '../pkg/intmax2_wasm_lib';
import { ethers } from 'ethers';
import { claimWithdrawals } from './contract';
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

    const key = await generate_intmax_account_from_eth_key(ethKey);
    const publicKey = key.pubkey;
    const privateKey = key.privkey;
    console.log("privateKey: ", privateKey);
    console.log("publicKey: ", publicKey);

    // print withdrawal status 
    let needClaimWithdrawals = [];
    const withdrawalInfo = await get_withdrawal_info(config, privateKey);
    for (let i = 0; i < withdrawalInfo.length; i++) {
        const withdrawal = withdrawalInfo[i];
        if (withdrawal.status === "need_claim") {
            needClaimWithdrawals.push(withdrawal.contract_withdrawal);
        }
    }
    await claimWithdrawals(privateKey, env.L1_RPC_URL, env.LIQUIDITY_CONTRACT_ADDRESS, needClaimWithdrawals);
}

main().then(() => {
    process.exit(0);
}).catch((err) => {
    console.error(err);
    process.exit(1);
});