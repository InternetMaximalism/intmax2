import { generate_intmax_account_from_eth_key, get_derive_path_list, JsDerive, save_derive_path, } from '../pkg';
import { env, config } from './setup';

async function main() {
    const ethKey = env.USER_ETH_PRIVATE_KEY;
    const account = await generate_intmax_account_from_eth_key(config.network, ethKey, false);
    
    const derive = new JsDerive(1, 3);
    await save_derive_path(config, account.view_pair, derive);

    const list = await get_derive_path_list(config, account.view_pair);
    for (const path of list) {
        console.log(path.derive_path, path.redeposit_path);
    }
}


main().then(() => {
    process.exit(0);
}).catch((err) => {
    console.error(err);
    process.exit(1);
});