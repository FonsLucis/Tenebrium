#![no_main]

use libfuzzer_sys::fuzz_target;
use tenebriumd::p2p::parse_message_bytes;

fuzz_target!(|data: &[u8]| {
    let _ = parse_message_bytes(data);
});
