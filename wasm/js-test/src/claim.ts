import { generate_intmax_account_from_eth_key, get_withdrawal_info } from '../pkg/intmax2_wasm_lib';
import { ethers } from 'ethers';
import { claimWithdrawals } from './contract';
import { env, config } from './setup';

async function main() {
    const ethKey = env.USER_ETH_PRIVATE_KEY;
    const ethAddress = new ethers.Wallet(ethKey).address;
    console.log("ethAddress: ", ethAddress);

    const key = await generate_intmax_account_from_eth_key(ethKey);
    const publicKey = key.pubkey;
    const privateKey = key.privkey;
    console.log("privateKey: ", privateKey);
    console.log("publicKey: ", publicKey);

    // print withdrawal status 
    let needClaimWithdrawals = [];
    const withdrawalInfo = await get_withdrawal_info(config, privateKey);
    for (let i = 0; i < withdrawalInfo.length; i++) {
        const withdrawal = withdrawalInfo[i];
        if (withdrawal.status === "need_claim") {
            needClaimWithdrawals.push(withdrawal.contract_withdrawal);
        }
    }
    await claimWithdrawals(privateKey, env.L1_RPC_URL, env.LIQUIDITY_CONTRACT_ADDRESS, needClaimWithdrawals);
}

main().then(() => {
    process.exit(0);
}).catch((err) => {
    console.error(err);
    process.exit(1);
});