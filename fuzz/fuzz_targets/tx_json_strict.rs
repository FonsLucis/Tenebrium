#![no_main]

use libfuzzer_sys::fuzz_target;
use tenebrium_utxo::Transaction;

fuzz_target!(|data: &[u8]| {
    if let Ok(tx) = Transaction::from_json_bytes(data) {
        let _ = tx.canonical_bytes_v2();
        let _ = tx.txid_v2();
    }
});
