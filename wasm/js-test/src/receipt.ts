import { generate_intmax_account_from_eth_key, generate_transfer_receipt, validate_transfer_receipt, } from '../pkg';
import { env, config } from './setup';

async function main() {
    const ethKey = env.USER_ETH_PRIVATE_KEY;
    const { privkey: privateKey, pubkey: publicKey } = await generate_intmax_account_from_eth_key(ethKey);
    console.log(`privkey`, privateKey);
    console.log(`pubkey`, publicKey);

    const tx_digest = "0xd1e845b5c4ad76ed15b75606f280ec1b3cb24c153f12da01a4c0e08490a6b9b9"; // self transfer tx digest in dev env
    const transfer_index = 0; // the first transfer
    console.log(`tx_digest: ${tx_digest}`);

    const receipt = await generate_transfer_receipt(config, privateKey, tx_digest, transfer_index);
    console.log(`size of receipt: ${receipt.length}`);

    // verify the receipt
    const recovered_transfer_data = await validate_transfer_receipt(config, privateKey, receipt)
    console.log(`recovered transfer amount: ${recovered_transfer_data.transfer.amount}`);
}

main().then(() => {
    process.exit(0);
}).catch((err) => {
    console.error(err);
    process.exit(1);
});