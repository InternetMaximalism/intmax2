/* tslint:disable */
/* eslint-disable */
export const memory: WebAssembly.Memory;
export function __wbg_key_free(a: number, b: number): void;
export function __wbg_get_key_privkey(a: number): Array;
export function __wbg_set_key_privkey(a: number, b: number, c: number): void;
export function __wbg_get_key_pubkey(a: number): Array;
export function __wbg_set_key_pubkey(a: number, b: number, c: number): void;
export function generate_key_from_provisional(a: number, b: number): number;
export function prepare_deposit(a: number, b: number, c: number, d: number, e: number, f: number): number;
export function send_tx_request(a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number): number;
export function finalize_tx(a: number, b: number, c: number, d: number, e: number, f: number): number;
export function sync(a: number, b: number, c: number): number;
export function sync_withdrawals(a: number, b: number, c: number): number;
export function get_user_data(a: number, b: number, c: number): number;
export function decryt_deposit_data(a: number, b: number, c: number, d: number, e: number): number;
export function mimic_deposit(a: number, b: number, c: number, d: number, e: number): number;
export function __wbg_jsuserdata_free(a: number, b: number): void;
export function __wbg_get_jsuserdata_pubkey(a: number): Array;
export function __wbg_set_jsuserdata_pubkey(a: number, b: number, c: number): void;
export function __wbg_get_jsuserdata_block_number(a: number): number;
export function __wbg_set_jsuserdata_block_number(a: number, b: number): void;
export function __wbg_get_jsuserdata_balances(a: number): Array;
export function __wbg_set_jsuserdata_balances(a: number, b: number, c: number): void;
export function __wbg_get_jsuserdata_private_commitment(a: number): Array;
export function __wbg_set_jsuserdata_private_commitment(a: number, b: number, c: number): void;
export function __wbg_get_jsuserdata_deposit_lpt(a: number): number;
export function __wbg_set_jsuserdata_deposit_lpt(a: number, b: number): void;
export function __wbg_get_jsuserdata_transfer_lpt(a: number): number;
export function __wbg_set_jsuserdata_transfer_lpt(a: number, b: number): void;
export function __wbg_get_jsuserdata_tx_lpt(a: number): number;
export function __wbg_set_jsuserdata_tx_lpt(a: number, b: number): void;
export function __wbg_get_jsuserdata_withdrawal_lpt(a: number): number;
export function __wbg_set_jsuserdata_withdrawal_lpt(a: number, b: number): void;
export function __wbg_get_jsuserdata_processed_deposit_uuids(a: number): Array;
export function __wbg_set_jsuserdata_processed_deposit_uuids(a: number, b: number, c: number): void;
export function __wbg_get_jsuserdata_processed_transfer_uuids(a: number): Array;
export function __wbg_set_jsuserdata_processed_transfer_uuids(a: number, b: number, c: number): void;
export function __wbg_get_jsuserdata_processed_tx_uuids(a: number): Array;
export function __wbg_set_jsuserdata_processed_tx_uuids(a: number, b: number, c: number): void;
export function __wbg_get_jsuserdata_processed_withdrawal_uuids(a: number): Array;
export function __wbg_set_jsuserdata_processed_withdrawal_uuids(a: number, b: number, c: number): void;
export function __wbg_tokenbalance_free(a: number, b: number): void;
export function __wbg_config_free(a: number, b: number): void;
export function __wbg_get_config_store_vault_server_url(a: number): Array;
export function __wbg_set_config_store_vault_server_url(a: number, b: number, c: number): void;
export function __wbg_get_config_block_validity_prover_url(a: number): Array;
export function __wbg_set_config_block_validity_prover_url(a: number, b: number, c: number): void;
export function __wbg_get_config_balance_prover_url(a: number): Array;
export function __wbg_set_config_balance_prover_url(a: number, b: number, c: number): void;
export function __wbg_get_config_withdrawal_aggregator_url(a: number): Array;
export function __wbg_set_config_withdrawal_aggregator_url(a: number, b: number, c: number): void;
export function __wbg_get_config_deposit_timeout(a: number): number;
export function __wbg_set_config_deposit_timeout(a: number, b: number): void;
export function __wbg_get_config_tx_timeout(a: number): number;
export function __wbg_set_config_tx_timeout(a: number, b: number): void;
export function __wbg_get_config_max_tx_query_times(a: number): number;
export function __wbg_set_config_max_tx_query_times(a: number, b: number): void;
export function __wbg_get_config_tx_query_interval(a: number): number;
export function __wbg_set_config_tx_query_interval(a: number, b: number): void;
export function config_new(a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number): number;
export function __wbg_jsdepositdata_free(a: number, b: number): void;
export function __wbg_get_jsdepositdata_deposit_salt(a: number): Array;
export function __wbg_set_jsdepositdata_deposit_salt(a: number, b: number, c: number): void;
export function __wbg_get_jsdepositdata_pubkey_salt_hash(a: number): Array;
export function __wbg_set_jsdepositdata_pubkey_salt_hash(a: number, b: number, c: number): void;
export function __wbg_get_jsdepositdata_token_index(a: number): number;
export function __wbg_set_jsdepositdata_token_index(a: number, b: number): void;
export function __wbg_get_jsdepositdata_amount(a: number): Array;
export function __wbg_set_jsdepositdata_amount(a: number, b: number, c: number): void;
export function __wbg_jstransfer_free(a: number, b: number): void;
export function __wbg_get_jstransfer_is_withdrawal(a: number): number;
export function __wbg_set_jstransfer_is_withdrawal(a: number, b: number): void;
export function __wbg_jstransferdata_free(a: number, b: number): void;
export function __wbg_get_jstransferdata_transfer(a: number): number;
export function __wbg_set_jstransferdata_transfer(a: number, b: number): void;
export function __wbg_jstx_free(a: number, b: number): void;
export function __wbg_get_jstx_nonce(a: number): number;
export function __wbg_set_jstx_nonce(a: number, b: number): void;
export function __wbg_jstxdata_free(a: number, b: number): void;
export function __wbg_get_jstxdata_tx(a: number): number;
export function __wbg_set_jstxdata_tx(a: number, b: number): void;
export function __wbg_get_jstxdata_transfers(a: number): Array;
export function __wbg_set_jstxdata_transfers(a: number, b: number, c: number): void;
export function __wbg_get_jstransfer_token_index(a: number): number;
export function __wbg_set_jstransfer_recipient(a: number, b: number, c: number): void;
export function __wbg_set_jstransfer_amount(a: number, b: number, c: number): void;
export function __wbg_set_jstransfer_salt(a: number, b: number, c: number): void;
export function __wbg_set_jstransferdata_sender(a: number, b: number, c: number): void;
export function __wbg_set_jstx_transfer_tree_root(a: number, b: number, c: number): void;
export function __wbg_set_jstransfer_token_index(a: number, b: number): void;
export function __wbg_get_jstransfer_recipient(a: number): Array;
export function __wbg_get_jstransfer_amount(a: number): Array;
export function __wbg_get_jstransfer_salt(a: number): Array;
export function __wbg_get_jstransferdata_sender(a: number): Array;
export function __wbg_get_jstx_transfer_tree_root(a: number): Array;
export function __wbindgen_malloc(a: number, b: number): number;
export function __wbindgen_realloc(a: number, b: number, c: number, d: number): number;
export const __wbindgen_export_2: WebAssembly.Table;
export const __wbindgen_export_3: WebAssembly.Table;
export function closure528_externref_shim(a: number, b: number, c: number): void;
export function __wbindgen_free(a: number, b: number, c: number): void;
export function __externref_drop_slice(a: number, b: number): void;
export function __externref_table_alloc(): number;
export function __wbindgen_exn_store(a: number): void;
export function closure645_externref_shim(a: number, b: number, c: number, d: number): void;
export function __wbindgen_start(): void;
