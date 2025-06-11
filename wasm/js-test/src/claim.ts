import { generate_intmax_account_from_eth_key, get_withdrawal_info } from '../pkg/intmax2_wasm_lib';
import { ethers } from 'ethers';
import { claimWithdrawals, } from './contract';
import { env, config } from './setup';

async function getEthAddress(privateKey: string): Promise<string> {
    return new ethers.Wallet(privateKey).address;
}

async function getWithdrawalsToClaim(view_pair: string) {
    const withdrwalInfo = await get_withdrawal_info(config, view_pair);
    return withdrwalInfo
        .filter(w => w.status === "need_claim")
        .map(w => w.contract_withdrawal);
}

async function main() {
    const ethKey = env.USER_ETH_PRIVATE_KEY;

    const ethAddress = await getEthAddress(ethKey);
    console.log("ethAddress: ", ethAddress);

    const account = await generate_intmax_account_from_eth_key(config.network, ethKey, false);

    const needClaimWithdrawals = await getWithdrawalsToClaim(account.view_pair);

    if (needClaimWithdrawals.length === 0) {
        console.log("No withdrawals need to be claimed.");
        return;
    }

    await claimWithdrawals(ethKey, env.L1_RPC_URL, env.LIQUIDITY_CONTRACT_ADDRESS, needClaimWithdrawals);
}

main().then(() => {
    process.exit(0);
}).catch((err) => {
    console.error(err);
    process.exit(1);
});