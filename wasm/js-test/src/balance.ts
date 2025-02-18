import { Config, get_user_data, JsGenericAddress, JsTransfer, sync, } from '../pkg';
import { env, config } from './setup';
import { ethers } from 'ethers';

async function main() {
    const ethKey = env.USER_ETH_PRIVATE_KEY;
    const privateKey = ethKey;

    // sync the account's balance proof 
    await syncBalanceProof(config, privateKey);

    // get the account's balance
    let userData = await get_user_data(config, privateKey);
    let balances = userData.balances;
    for (let i = 0; i < balances.length; i++) {
        const balance = balances[i];
        console.log(`Token ${balance.token_index}: ${balance.amount}`);
    }

    const recipient = new JsGenericAddress(false, ethers.ZeroAddress);
    const transfer = new JsTransfer(recipient, 0, "100", ethers.ZeroHash);
    const withdrawal = transfer.to_withdrawal();
    const nullifier = withdrawal.nullifier;
    const withdrawal_hash = withdrawal.hash();
    console.log(`nullifier: ${nullifier}, withdrawal_hash: ${withdrawal_hash}`);
}

async function syncBalanceProof(config: Config, privateKey: string) {
    console.log("syncing balance proof...");
    while (true) {
        try {
            await sync(config, privateKey);
            break;
        } catch (error) {
            console.log("Error syncing balance proof: ", error, "retrying...");
        }
        await sleep(10);
    }
    console.log("balance proof synced");
}

async function sleep(sec: number) {
    return new Promise((resolve) => setTimeout(resolve, sec * 1000));
}

main().then(() => {
    process.exit(0);
}).catch((err) => {
    console.error(err);
    process.exit(1);
});