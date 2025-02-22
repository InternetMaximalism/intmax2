import { fetch_encrypted_data, generate_auth_for_store_vault, generate_intmax_account_from_eth_key, JsMetaData, JsMetaDataCursor, } from '../pkg';
import { env, config } from './setup';

async function main() {
    const ethKey = env.USER_ETH_PRIVATE_KEY;
    const key = await generate_intmax_account_from_eth_key(ethKey);
    const privkey = key.privkey;

    // get auth for store vault using private key
    const auth = await generate_auth_for_store_vault(privkey);
    console.log(`auth: pubkey ${auth.pubkey}, expiry ${auth.expiry}`);

    // get latest 10 encrypted data
    const limit = 10;
    const order = "desc"; // or "asc"
    const cursor = new JsMetaDataCursor(null, order, limit);
    const data = await fetch_encrypted_data(config, auth, cursor);
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