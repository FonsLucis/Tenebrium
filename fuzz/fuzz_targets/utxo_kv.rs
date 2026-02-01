#![no_main]

use libfuzzer_sys::fuzz_target;
use tenebriumd::utxo_db::decode_utxo_entry;

fuzz_target!(|data: &[u8]| {
    if data.len() < 36 {
        return;
    }
    let (op, txout) = data.split_at(36);
    let _ = decode_utxo_entry(op, txout);
});
