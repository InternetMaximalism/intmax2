import { generate_intmax_account_from_eth_key, make_history_backup, } from '../pkg';
import { env, config } from './setup';

async function main() {
    const ethKey = env.USER_ETH_PRIVATE_KEY;
    const account = await generate_intmax_account_from_eth_key(config.network, ethKey, false);
    const backup = await make_history_backup(config, account.view_pair, 0n, 1000);
    console.log(backup);
}

main().then(() => {
    process.exit(0);
}).catch((err) => {
    console.error(err);
    process.exit(1);
});