import { cleanEnv, num, str, url } from 'envalid';
import { Config, } from '../pkg/intmax2_wasm_lib';
import * as dotenv from 'dotenv';
dotenv.config();

export const env = cleanEnv(process.env, {
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
    WITHDRAWAL_CONTRACT_ADDRESS: str(),
});

export const config = new Config(
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
    env.WITHDRAWAL_CONTRACT_ADDRESS,
);
