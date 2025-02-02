import { cleanEnv, str } from 'envalid';
import { generate_intmax_account_from_eth_key, JsFlatG2, sign_message, verify_signature } from '../pkg';
import * as dotenv from 'dotenv';
dotenv.config();

const env = cleanEnv(process.env, {
  USER_ETH_PRIVATE_KEY: str(),
});

const shouldBeFailed = async (fn: () => Promise<void>, expectedError?: string) => {
  try {
    await fn();
  } catch (err) {
    if (!expectedError) {
      return;
    }

    if ((err as Error).message === expectedError) {
      return;
    }

    throw new Error(`expected error: ${expectedError}, but got: ${(err as Error).message}`);
  }

  throw new Error(`should be failed`);
};

async function main() {
  const key = await generate_intmax_account_from_eth_key("0x1d7ca104307dae85de604175a38546b4bd358b014b9690fe6dd322dc6790f41a");
  let longMessage = "";
  for (let i = 0; i < 100; i++) {
    longMessage += "hello world ";
  }
  const message = Buffer.from(longMessage, "utf-8");
  const signature = await sign_message(key.privkey, message);

  const newSignature = new JsFlatG2(signature.elements); // construct a signature from raw data

  const result = await verify_signature(newSignature, key.pubkey, message);
  if (!result) {
    throw new Error("Invalid signature");
  }

  const test1 = async () => {
    const key = await generate_intmax_account_from_eth_key("087df966aa392aa8e32376617921418f8a0e078ef5d2b1d4ee873726798b608b");
    const result = await verify_signature(signature, key.pubkey, message);
    if (result) {
      throw new Error("Should be failed because of invalid pubkey");
    }
  };
  await test1();

  const test2 = async () => {
    const message = Buffer.from("hello world", "utf-8");
    const result = await verify_signature(signature, key.pubkey, message);
    if (result) {
      throw new Error("Should be failed because of invalid message");
    }
  };
  await test2();

  await shouldBeFailed(async () => {
    await verify_signature(signature, "087df966aa392aa8e32376617921418f8a0e078ef5d2b1d4ee873726798b608b", message);
  }, "Failed to parse public key");

  console.log("Done");
}

main().then(() => {
  process.exit(0);
}).catch((err) => {
  console.error(err);
  process.exit(1);
});
