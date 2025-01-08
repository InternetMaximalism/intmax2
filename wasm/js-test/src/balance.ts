import { cleanEnv, num, str, url } from 'envalid';
import { Config, generate_intmax_account_from_eth_key, get_user_data, JsGenericAddress, JsTransfer, sync, } from '../pkg';
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
    const privateKey = ethKey;

    // sync the account's balance proof 
    await syncBalanceProof(config, privateKey);

    // get the account's balance
    let userData = await get_user_data(config, privateKey);
    let balances = userData.balances;
    for (let i = 0; i < balances.length; i++) {
        const balance = balances[i];
        console.log(`Token ${balance.token_index}: ${balance.amount}`);
    }

    const recipient = new JsGenericAddress(false, ethers.ZeroAddress);
    const transfer = new JsTransfer(recipient, 0, "100", ethers.ZeroHash);
    const withdrawal = transfer.to_withdrawal();
    const nullifier = withdrawal.nullifier;
    const withdrawal_hash = withdrawal.hash();
    console.log(`nullifier: ${nullifier}, withdrawal_hash: ${withdrawal_hash}`);
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

async function sleep(sec: number) {
    return new Promise((resolve) => setTimeout(resolve, sec * 1000));
}

main().then(() => {
    process.exit(0);
}).catch((err) => {
    console.error(err);
    process.exit(1);
});