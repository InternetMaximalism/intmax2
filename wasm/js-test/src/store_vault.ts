import { cleanEnv, num, str, url } from 'envalid';
import { Config, fetch_encrypted_data, generate_auth_for_store_vault, generate_intmax_account_from_eth_key, } from '../pkg';
import * as dotenv from 'dotenv';
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
    const key = await generate_intmax_account_from_eth_key(ethKey);
    const privkey = key.privkey;

    // get auth for store vault using private key
    const auth = await generate_auth_for_store_vault(privkey);
    console.log(`auth: pubkey ${auth.pubkey}, expiry ${auth.expiry}`);

    // get latest 10 encrypted data
    const timestamp = undefined;
    const uuid = undefined;
    const limit = 10;
    const order = "desc"; // or "asc"
    const data = await fetch_encrypted_data(config, auth, timestamp, uuid, limit, order);
    console.log(`data.length: ${data.length}`);
    for (const d of data) {
        console.log(`type:${d.data_type} timestamp:${d.timestamp} uuid: ${d.uuid} data.length: ${d.data.length}`);
    }
}


main().then(() => {
    process.exit(0);
}).catch((err) => {
    console.error(err);
    process.exit(1);
});