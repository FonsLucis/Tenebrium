#![no_main]

use libfuzzer_sys::fuzz_target;
use tenebrium_consensus::{merkle_root, Block};

fuzz_target!(|data: &[u8]| {
    if let Ok(block) = serde_json::from_slice::<Block>(data) {
        let txids = block
            .txs
            .iter()
            .map(|tx| tx.txid_v2())
            .collect::<Result<Vec<_>, _>>();
        if let Ok(ids) = txids {
            let _ = merkle_root(&ids);
        }
    }
});
