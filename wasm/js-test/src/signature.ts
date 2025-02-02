import { cleanEnv, str } from 'envalid';
import { generate_intmax_account_from_eth_key, sign_message, verify_signature } from '../pkg';
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
  const ethKey = env.USER_ETH_PRIVATE_KEY;
  const key = await generate_intmax_account_from_eth_key(ethKey);
  let longMessage = "";
  for (let i = 0; i < 100; i++) {
    longMessage += "hello world ";
  }
  const message = Buffer.from(longMessage, "utf-8");
  const signature = await sign_message(key.privkey, message);

  const result = await verify_signature(signature, key.pubkey, message);
  if (!result) {
    throw new Error("Invalid signature");
  }

  const test1 = async () => {
    const key = await generate_intmax_account_from_eth_key("7397927abf5b7665c4667e8cb8b92e929e287625f79264564bb66c1fa2232b2c");
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

  console.log("Done");
}

main().then(() => {
  process.exit(0);
}).catch((err) => {
  console.error(err);
  process.exit(1);
});
