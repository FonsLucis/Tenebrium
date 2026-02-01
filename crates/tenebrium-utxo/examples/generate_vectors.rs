use hex::encode;
use serde_json::json;
use tenebrium_utxo::{OutPoint, Transaction, TxIn, TxOut};

fn main() {
    let mut vectors = Vec::new();

    // Simple tx
    let tx1 = Transaction {
        version: 1,
        vin: vec![TxIn {
            prevout: OutPoint {
                txid: [0u8; 32],
                vout: 0,
            },
            script_sig: vec![],
            sequence: 0,
        }],
        vout: vec![TxOut {
            value: 50,
            script_pubkey: vec![],
        }],
        lock_time: 0,
    };
    let c2 = tx1.canonical_bytes_v2().unwrap();
    let t2 = tx1.txid_v2().unwrap();
    let c1 = tx1.canonical_bytes_v1().unwrap();
    let t1 = tx1.txid_v1().unwrap();

    vectors.push(json!({
        "name": "simple",
        "tx": tx1,
        "canonical_v2": encode(&c2),
        "txid_v2": encode(&t2),
        "canonical_v1": encode(&c1),
        "txid_v1": encode(&t1),
    }));

    // Multiple inputs
    let tx2 = Transaction {
        version: 1,
        vin: vec![
            TxIn {
                prevout: OutPoint {
                    txid: [1u8; 32],
                    vout: 0,
                },
                script_sig: b"sig1".to_vec(),
                sequence: 1,
            },
            TxIn {
                prevout: OutPoint {
                    txid: [2u8; 32],
                    vout: 1,
                },
                script_sig: b"sig2".to_vec(),
                sequence: 2,
            },
        ],
        vout: vec![
            TxOut {
                value: 60,
                script_pubkey: b"pk1".to_vec(),
            },
            TxOut {
                value: 39,
                script_pubkey: b"pk2".to_vec(),
            },
        ],
        lock_time: 0,
    };
    let c2 = tx2.canonical_bytes_v2().unwrap();
    let t2 = tx2.txid_v2().unwrap();
    let c1 = tx2.canonical_bytes_v1().unwrap();
    let t1 = tx2.txid_v1().unwrap();

    vectors.push(json!({
        "name": "multiple_inputs",
        "tx": tx2,
        "canonical_v2": encode(&c2),
        "txid_v2": encode(&t2),
        "canonical_v1": encode(&c1),
        "txid_v1": encode(&t1),
    }));

    // Script boundary (1000 bytes)
    let big_script = vec![0xABu8; 1000];
    let tx3 = Transaction {
        version: 1,
        vin: vec![TxIn {
            prevout: OutPoint {
                txid: [3u8; 32],
                vout: 0,
            },
            script_sig: big_script.clone(),
            sequence: 0,
        }],
        vout: vec![TxOut {
            value: 1000,
            script_pubkey: big_script.clone(),
        }],
        lock_time: 0,
    };
    let c2 = tx3.canonical_bytes_v2().unwrap();
    let t2 = tx3.txid_v2().unwrap();
    let c1 = tx3.canonical_bytes_v1().unwrap();
    let t1 = tx3.txid_v1().unwrap();

    vectors.push(json!({
        "name": "script_boundary",
        "tx": tx3,
        "canonical_v2": encode(&c2),
        "txid_v2": encode(&t2),
        "canonical_v1": encode(&c1),
        "txid_v1": encode(&t1),
    }));

    // Edge values
    let tx4 = Transaction {
        version: 1,
        vin: vec![TxIn {
            prevout: OutPoint {
                txid: [4u8; 32],
                vout: 0,
            },
            script_sig: vec![],
            sequence: 0,
        }],
        vout: vec![TxOut {
            value: u64::MAX,
            script_pubkey: vec![],
        }],
        lock_time: 0,
    };
    let c2 = tx4.canonical_bytes_v2().unwrap();
    let t2 = tx4.txid_v2().unwrap();
    let c1 = tx4.canonical_bytes_v1().unwrap();
    let t1 = tx4.txid_v1().unwrap();

    vectors.push(json!({
        "name": "edge_values",
        "tx": tx4,
        "canonical_v2": encode(&c2),
        "txid_v2": encode(&t2),
        "canonical_v1": encode(&c1),
        "txid_v1": encode(&t1),
    }));

    println!("{}", serde_json::to_string_pretty(&vectors).unwrap());
}
