import { fetch_transfer_history, generate_intmax_account_from_eth_key, generate_transfer_receipt, get_derive_path_list, JsDerive, JsMetaDataCursor, save_derive_path, validate_transfer_receipt, } from '../pkg';
import { env, config } from './setup';

async function main() {
    const ethKey = env.USER_ETH_PRIVATE_KEY;
    const key = await generate_intmax_account_from_eth_key(ethKey);
    const privkey = key.privkey;
    console.log(`privkey`, privkey);
    console.log(`pubkey`, key.pubkey);

    const cursor = new JsMetaDataCursor(null, "asc", null);
    const transfer_history = await fetch_transfer_history(config, key.privkey, cursor);
    if (transfer_history.history.length === 0) {
        console.log("No transfer history found");
        return;
    }
    const transfer_data = transfer_history.history[0].data;
    const transfer_digest = transfer_history.history[0].meta.digest;
    const recipient = transfer_data.transfer.recipient.data;
    console.log(`transfer_digest: ${transfer_digest}`);
    console.log(`recipient: ${recipient}`);

    const receipt = await generate_transfer_receipt(config, key.privkey, transfer_digest, recipient);
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