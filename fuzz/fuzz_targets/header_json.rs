#![no_main]

use libfuzzer_sys::fuzz_target;
use tenebrium_consensus::{check_pow, header_hash, BlockHeader};

fuzz_target!(|data: &[u8]| {
    if let Ok(header) = serde_json::from_slice::<BlockHeader>(data) {
        let _ = header_hash(&header);
        let _ = check_pow(&header);
    }
});
