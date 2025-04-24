import { generate_intmax_account_from_eth_key, make_history_backup, } from '../pkg';
import { env, config } from './setup';

async function main() {
    const ethKey = env.USER_ETH_PRIVATE_KEY;
    const key = await generate_intmax_account_from_eth_key(ethKey);
    const privkey = key.privkey;

    const backup = await make_history_backup(config, privkey, 0n, 1000);
    console.log(backup);
}

main().then(() => {
    process.exit(0);
}).catch((err) => {
    console.error(err);
    process.exit(1);
});