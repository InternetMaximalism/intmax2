import { fetch_tx_history, generate_intmax_account_from_eth_key, generate_transfer_receipt, JsMetaDataCursor, validate_transfer_receipt, } from '../pkg';
import { env, config } from './setup';

async function main() {
    const ethKey = env.USER_ETH_PRIVATE_KEY;
    const key = await generate_intmax_account_from_eth_key(ethKey);
    const privkey = key.privkey;
    console.log(`privkey`, privkey);
    console.log(`pubkey`, key.pubkey);

    const cursor = new JsMetaDataCursor(null, "asc", null);
    const tx_history = await fetch_tx_history(config, key.privkey, cursor);
    if (tx_history.history.length === 0) {
        console.log("No transfer history found");
        return;
    }
    const tx_data = tx_history.history[0];
    const tx_digest = tx_data.meta.digest;
    const transfer_index = 0; // the first transfer
    console.log(`tx_digest: ${tx_digest}`);

    const receipt = await generate_transfer_receipt(config, key.privkey, tx_digest, transfer_index);
    console.log(`size of receipt: ${receipt.length}`);

    // verify the receipt
    const recovered_transfer_data = await validate_transfer_receipt(config, key.privkey, receipt)
    console.log(`recovered transfer amount: ${recovered_transfer_data.transfer.amount}`);
}

main().then(() => {
    process.exit(0);
}).catch((err) => {
    console.error(err);
    process.exit(1);
});