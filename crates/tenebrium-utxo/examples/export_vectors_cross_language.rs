use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct VectorIn {
    name: Option<String>,
    tx: Value,
    canonical_v1: String,
    canonical_v2: String,
    txid_v1: String,
    txid_v2: String,
}

#[derive(Debug, Serialize)]
struct VectorOut {
    name: Option<String>,
    tx: Value,
    canonical_v1_json: String,
    canonical_v1_hex: String,
    canonical_v2_hex: String,
    txid_v1_hex: String,
    txid_v2_hex: String,
}

fn main() {
    let in_path = Path::new("crates/tenebrium-utxo/test_vectors/vectors.json");
    let out_path = Path::new("crates/tenebrium-utxo/test_vectors/vectors_cross_language.json");

    let mut raw = fs::read_to_string(in_path).expect("failed to read vectors.json");
    if raw.starts_with('\u{feff}') {
        raw = raw.trim_start_matches('\u{feff}').to_string();
    }
    let inputs: Vec<VectorIn> = serde_json::from_str(&raw).expect("failed to parse vectors.json");

    let mut outputs = Vec::with_capacity(inputs.len());
    for v in inputs {
        let c1_bytes = hex::decode(&v.canonical_v1).expect("invalid canonical_v1 hex");
        let c1_json = String::from_utf8(c1_bytes).expect("canonical_v1 is not valid UTF-8");

        outputs.push(VectorOut {
            name: v.name,
            tx: v.tx,
            canonical_v1_json: c1_json,
            canonical_v1_hex: v.canonical_v1,
            canonical_v2_hex: v.canonical_v2,
            txid_v1_hex: v.txid_v1,
            txid_v2_hex: v.txid_v2,
        });
    }

    let pretty = serde_json::to_string_pretty(&outputs).expect("failed to serialize output");
    fs::write(out_path, pretty).expect("failed to write vectors_cross_language.json");

    println!("Wrote {} vectors -> {}", outputs.len(), out_path.display());
}
