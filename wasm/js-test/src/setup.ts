import { bool, cleanEnv, num, str, url, } from 'envalid';
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

    USE_PRIVATE_ZKP_SERVER: bool(),
    USE_S3: bool(),

    // Timeout configurations
    DEPOSIT_TIMEOUT: num(),
    TX_TIMEOUT: num(),
    IS_FASTER_MINING: bool(),

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
    WITHDRAWAL_CONTRACT_ADDRESS: str(),

    // Private ZKP server configurations
    PRIVATE_ZKP_SERVER_MAX_RETRIES: num({ default: 30 }),
    PRIVATE_ZKP_SERVER_RETRY_INTERVAL: num({ default: 5 }),
});

export const config = new Config(
    env.STORE_VAULT_SERVER_BASE_URL,
    env.BALANCE_PROVER_BASE_URL,
    env.VALIDITY_PROVER_BASE_URL,
    env.WITHDRAWAL_SERVER_BASE_URL,
    BigInt(env.DEPOSIT_TIMEOUT),
    BigInt(env.TX_TIMEOUT),
    env.IS_FASTER_MINING,
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
    env.WITHDRAWAL_CONTRACT_ADDRESS,
    env.USE_PRIVATE_ZKP_SERVER,
    env.USE_S3,
    env.PRIVATE_ZKP_SERVER_MAX_RETRIES,
    BigInt(env.PRIVATE_ZKP_SERVER_RETRY_INTERVAL),
);
