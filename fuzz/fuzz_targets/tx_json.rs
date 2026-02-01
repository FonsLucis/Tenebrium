#![no_main]

use libfuzzer_sys::fuzz_target;
use tenebrium_utxo::Transaction;

fuzz_target!(|data: &[u8]| {
    if let Ok(tx) = serde_json::from_slice::<Transaction>(data) {
        let _ = tx.validate();
        let _ = tx.canonical_bytes_v2();
        let _ = tx.txid_v2();
    }
});
