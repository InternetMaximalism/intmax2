import { cleanEnv, str } from "envalid";
import {
  calc_simple_aggregated_pubkey,
  decrypt_bls_interaction_step1,
  decrypt_bls_interaction_step2,
  decrypt_bls_interaction_step3,
  encrypt_message,
  generate_intmax_account_from_eth_key,
  JsFlatG2,
  multi_signature_interaction_step1,
  multi_signature_interaction_step2,
  multi_signature_interaction_step3,
  sign_message,
  verify_signature,
} from "../pkg";
import * as dotenv from "dotenv";
dotenv.config();

const env = cleanEnv(process.env, {
  USER_ETH_PRIVATE_KEY: str(),
});

async function main() {
  const key = await generate_intmax_account_from_eth_key(
    "0x1d7ca104307dae85de604175a38546b4bd358b014b9690fe6dd322dc6790f41a",
  );
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
    const key = await generate_intmax_account_from_eth_key(
      "087df966aa392aa8e32376617921418f8a0e078ef5d2b1d4ee873726798b608b",
    );
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

  // Test two-party multisig
  const test3 = async () => {
    const client_key = await generate_intmax_account_from_eth_key(
      "1d7ca104307dae85de604175a38546b4bd358b014b9690fe6dd322dc6790f41a",
    );
    const server_key = await generate_intmax_account_from_eth_key(
      "1ce9e55589f4b29fa5ca76860c1351d84a9d519505755171190f02add3a4759b",
    );

    const actual_aggregated_pubkey = calc_simple_aggregated_pubkey([
      client_key.pubkey,
      server_key.pubkey,
    ]);

    const response_step1 = multi_signature_interaction_step1(
      client_key.privkey,
      message,
    );
    const response_step2 = multi_signature_interaction_step2(
      server_key.privkey,
      response_step1,
    );
    const response_step3 = multi_signature_interaction_step3(
      client_key.privkey,
      response_step1,
      response_step2,
    );
    const aggregated_pubkey = response_step3.aggregated_pubkey;
    const aggregated_signature = response_step3.aggregated_signature;
    if (aggregated_pubkey !== actual_aggregated_pubkey) {
      console.log("aggregated_pubkey", aggregated_pubkey);
      console.log("actual_aggregated_pubkey", actual_aggregated_pubkey);
      throw new Error("Invalid aggregated pubkey");
    }

    const result = await verify_signature(
      aggregated_signature,
      aggregated_pubkey,
      message,
    );
    if (!result) {
      throw new Error("Invalid signature");
    }

    const wrong_result = await verify_signature(
      aggregated_signature,
      client_key.pubkey,
      message,
    );
    if (wrong_result) {
      throw new Error("Should be failed because of invalid message");
    }
  };
  await test3();

  // Test encryption interaction
  const testEncryptionInteraction = async () => {
    const client_key = await generate_intmax_account_from_eth_key(
      "1d7ca104307dae85de604175a38546b4bd358b014b9690fe6dd322dc6790f41a",
    );
    const server_key = await generate_intmax_account_from_eth_key(
      "1ce9e55589f4b29fa5ca76860c1351d84a9d519505755171190f02add3a4759b",
    );

    const message = Buffer.from("hello world", "utf-8");
    const aggregated_pubkey = calc_simple_aggregated_pubkey([
      client_key.pubkey,
      server_key.pubkey,
    ]);
    const encrypted_message = encrypt_message(aggregated_pubkey, message);

    const response_step1 = decrypt_bls_interaction_step1(
      server_key.privkey,
      encrypted_message,
    );
    const response_step2 = decrypt_bls_interaction_step2(
      client_key.privkey,
      response_step1,
    );
    const response_step3 = decrypt_bls_interaction_step3(
      server_key.privkey,
      response_step1,
      response_step2,
    );
    const decrypted_message = response_step3.message;
    if (!message.equals(Buffer.from(decrypted_message))) {
      console.log(
        "decrypted_message",
        Buffer.from(decrypted_message).toString("utf-8"),
      );
      console.log("message", message.toString("utf-8"));
      throw new Error("Invalid decrypted message");
    }
  };
  await testEncryptionInteraction();

  // Test encryption interaction with wrong client key
  const testEncryptionInteractionWithWrongClientKey = async () => {
    const client_key = await generate_intmax_account_from_eth_key(
      "1d7ca104307dae85de604175a38546b4bd358b014b9690fe6dd322dc6790f41a",
    );
    const server_key = await generate_intmax_account_from_eth_key(
      "1ce9e55589f4b29fa5ca76860c1351d84a9d519505755171190f02add3a4759b",
    );

    const message = Buffer.from("hello world", "utf-8");
    const aggregated_pubkey = calc_simple_aggregated_pubkey([
      client_key.pubkey,
      server_key.pubkey,
    ]);
    const encrypted_message = encrypt_message(aggregated_pubkey, message);

    const wrong_client_key = await generate_intmax_account_from_eth_key(
      "2b2fc905c05ab0ded82327c9be57ce9827a10461ba448ba7b3468e89f875794e",
    );
    const response_step1 = decrypt_bls_interaction_step1(
      wrong_client_key.privkey,
      encrypted_message,
    );
    const response_step2 = decrypt_bls_interaction_step2(
      server_key.privkey,
      response_step1,
    );

    try {
      decrypt_bls_interaction_step3(
        wrong_client_key.privkey,
        response_step1,
        response_step2,
      );

      throw new Error("Should be failed because of invalid client key");
    } catch (e) {
      const errorMessage = (e as Error).message;
      if (errorMessage !== "tag check failure in read_header") {
        throw new Error("Should be failed because of unexpected error message");
      }
    }
  };
  await testEncryptionInteractionWithWrongClientKey();

  // Test encryption interaction with wrong server key
  const testEncryptionInteractionWithWrongServerKey = async () => {
    const client_key = await generate_intmax_account_from_eth_key(
      "1d7ca104307dae85de604175a38546b4bd358b014b9690fe6dd322dc6790f41a",
    );
    const server_key = await generate_intmax_account_from_eth_key(
      "1ce9e55589f4b29fa5ca76860c1351d84a9d519505755171190f02add3a4759b",
    );

    const message = Buffer.from("hello world", "utf-8");
    const aggregated_pubkey = calc_simple_aggregated_pubkey([
      client_key.pubkey,
      server_key.pubkey,
    ]);
    const encrypted_message = encrypt_message(aggregated_pubkey, message);

    const wrong_server_key = await generate_intmax_account_from_eth_key(
      "2b2fc905c05ab0ded82327c9be57ce9827a10461ba448ba7b3468e89f875794e",
    );
    const response_step1 = decrypt_bls_interaction_step1(
      client_key.privkey,
      encrypted_message,
    );
    const response_step2 = decrypt_bls_interaction_step2(
      wrong_server_key.privkey,
      response_step1,
    );

    try {
      decrypt_bls_interaction_step3(
        client_key.privkey,
        response_step1,
        response_step2,
      );

      throw new Error("Should be failed because of invalid server key");
    } catch (e) {
      const errorMessage = (e as Error).message;
      if (errorMessage !== "tag check failure in read_header") {
        throw new Error("Should be failed because of unexpected error message");
      }
    }
  };
  await testEncryptionInteractionWithWrongServerKey();

  console.log("Done");
}

main()
  .then(() => {
    process.exit(0);
  })
  .catch((err) => {
    console.error(err);
    process.exit(1);
  });
