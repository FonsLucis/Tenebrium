//! Tenebrium UTXO library
//!
//! Provides minimal UTXO-related types, JSON (serde_json) serialization helpers
//! and an in-memory UTXO set implementation for v0.1.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

mod reindex;
pub use reindex::{map_outpoints_v1_to_v2, ReindexErrorEntry, ReindexErrorKind, ReindexReport};

/// Maximum allowed script size in bytes (DoS mitigation)
pub const MAX_SCRIPT_SIZE: usize = 10_000;
/// Maximum allowed number of inputs or outputs in a transaction (temporary cap)
pub const MAX_TX_INOUTS: usize = 10_000;

/// Basic OutPoint identifying an output in a transaction
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutPoint {
    pub txid: [u8; 32],
    pub vout: u32,
}

/// Transaction input
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxIn {
    pub prevout: OutPoint,
    pub script_sig: Vec<u8>,
    pub sequence: u32,
}

/// Transaction output
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxOut {
    pub value: u64,
    pub script_pubkey: Vec<u8>,
}

/// Transaction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub version: i32,
    pub vin: Vec<TxIn>,
    pub vout: Vec<TxOut>,
    pub lock_time: u32,
}

/// Errors for UTXO crate
#[derive(thiserror::Error, Debug)]
pub enum UtxoError {
    #[error("script too large: {0} bytes (max {1})")]
    TooLargeScript(usize, usize),
    #[error("too many inputs or outputs: {0} (max {1})")]
    TooManyInOut(usize, usize),
    #[error("overflow during summation")]
    Overflow,
    #[error("missing utxo: {0:?}")]
    MissingUtxo(OutPoint),
    #[error("value not conserved: input={input} output={output}")]
    ValueNotConserved { input: u64, output: u64 },
    #[error("duplicate input: {0:?}")]
    DuplicateInput(OutPoint),
    #[error("duplicate output: {0:?}")]
    DuplicateOutput(OutPoint),
    #[error("serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),
}

impl Transaction {
    /// Validate transaction fields for v0.1 policy
    pub fn validate(&self) -> Result<(), UtxoError> {
        if self.vin.len() > MAX_TX_INOUTS {
            return Err(UtxoError::TooManyInOut(self.vin.len(), MAX_TX_INOUTS));
        }
        if self.vout.len() > MAX_TX_INOUTS {
            return Err(UtxoError::TooManyInOut(self.vout.len(), MAX_TX_INOUTS));
        }
        for input in &self.vin {
            if input.script_sig.len() > MAX_SCRIPT_SIZE {
                return Err(UtxoError::TooLargeScript(
                    input.script_sig.len(),
                    MAX_SCRIPT_SIZE,
                ));
            }
        }
        for output in &self.vout {
            if output.script_pubkey.len() > MAX_SCRIPT_SIZE {
                return Err(UtxoError::TooLargeScript(
                    output.script_pubkey.len(),
                    MAX_SCRIPT_SIZE,
                ));
            }
        }
        Ok(())
    }

    /// Serialize to JSON bytes
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, UtxoError> {
        serde_json::to_vec(self).map_err(UtxoError::from)
    }

    /// Deserialize from JSON bytes and run validation
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, UtxoError> {
        let tx: Transaction = serde_json::from_slice(bytes)?;
        tx.validate()?;
        Ok(tx)
    }

    /// Sum outputs, detecting overflow
    pub fn sum_outputs(tx: &Transaction) -> Result<u64, UtxoError> {
        let mut sum: u64 = 0;
        for out in &tx.vout {
            sum = sum.checked_add(out.value).ok_or(UtxoError::Overflow)?;
        }
        Ok(sum)
    }

    /// Sum inputs by looking up UTXOs in provided set, detecting missing or overflow
    pub fn sum_inputs(tx: &Transaction, utxos: &dyn UtxoSet) -> Result<u64, UtxoError> {
        let mut sum: u64 = 0;
        for vin in &tx.vin {
            let prev = utxos
                .get(&vin.prevout)
                .ok_or(UtxoError::MissingUtxo(vin.prevout.clone()))?;
            sum = sum.checked_add(prev.value).ok_or(UtxoError::Overflow)?;
        }
        Ok(sum)
    }

    /// Validate value conservation and duplicate inputs. Returns fee (input - output)
    pub fn validate_value_conservation(
        tx: &Transaction,
        utxos: &dyn UtxoSet,
    ) -> Result<u64, UtxoError> {
        let mut seen: HashSet<OutPoint> = HashSet::new();
        for vin in &tx.vin {
            if !seen.insert(vin.prevout.clone()) {
                return Err(UtxoError::DuplicateInput(vin.prevout.clone()));
            }
        }
        let input_sum = Transaction::sum_inputs(tx, utxos)?;
        let output_sum = Transaction::sum_outputs(tx)?;
        if input_sum < output_sum {
            return Err(UtxoError::ValueNotConserved {
                input: input_sum,
                output: output_sum,
            });
        }
        Ok(input_sum - output_sum)
    }

    /// Canonical bytes v1 (JSON-based) - kept for backward compatibility
    pub fn canonical_bytes_v1(&self) -> Result<Vec<u8>, UtxoError> {
        serde_json::to_vec(self).map_err(UtxoError::from)
    }

    /// Canonical bytes v2 (binary deterministic encoding)
    /// Layout (all integers little-endian):
    /// - version: i32
    /// - vin_count: u64
    /// - for each vin:
    ///   - prevout.txid (32 bytes)
    ///   - prevout.vout u32
    ///   - script_sig_len u64, script_sig bytes
    ///   - sequence u32
    /// - vout_count: u64
    /// - for each vout:
    ///   - value u64
    ///   - script_pubkey_len u64, script_pubkey bytes
    /// - lock_time u32
    pub fn canonical_bytes_v2(&self) -> Result<Vec<u8>, UtxoError> {
        // validation ensures script lengths and counts are within bounds
        self.validate()?;
        let mut out: Vec<u8> = Vec::new();
        out.extend(&self.version.to_le_bytes());
        out.extend(&(self.vin.len() as u64).to_le_bytes());
        for vin in &self.vin {
            out.extend(&vin.prevout.txid);
            out.extend(&vin.prevout.vout.to_le_bytes());
            out.extend(&(vin.script_sig.len() as u64).to_le_bytes());
            out.extend(&vin.script_sig);
            out.extend(&vin.sequence.to_le_bytes());
        }
        out.extend(&(self.vout.len() as u64).to_le_bytes());
        for vout in &self.vout {
            out.extend(&vout.value.to_le_bytes());
            out.extend(&(vout.script_pubkey.len() as u64).to_le_bytes());
            out.extend(&vout.script_pubkey);
        }
        out.extend(&self.lock_time.to_le_bytes());
        Ok(out)
    }

    /// Compute txid as double-SHA256 of canonical bytes v2
    pub fn txid_v2(&self) -> Result<[u8; 32], UtxoError> {
        let bytes = self.canonical_bytes_v2()?;
        let first = Sha256::digest(&bytes);
        let second = Sha256::digest(&first);
        let mut out = [0u8; 32];
        out.copy_from_slice(&second);
        Ok(out)
    }

    /// Compute txid v1 (legacy JSON-based) for compatibility
    pub fn txid_v1(&self) -> Result<[u8; 32], UtxoError> {
        let bytes = self.canonical_bytes_v1()?;
        let first = Sha256::digest(&bytes);
        let second = Sha256::digest(&first);
        let mut out = [0u8; 32];
        out.copy_from_slice(&second);
        Ok(out)
    }

    /// Default txid() now returns v2 (canonical binary) — this is the preferred v0.2 behavior
    pub fn txid(&self) -> Result<[u8; 32], UtxoError> {
        self.txid_v2()
    }

    /// Make OutPoints for transaction outputs using canonical txid v2
    pub fn make_outpoints_v2(tx: &Transaction) -> Result<Vec<OutPoint>, UtxoError> {
        let txid = tx.txid_v2()?;
        Ok(tx
            .vout
            .iter()
            .enumerate()
            .map(|(i, _)| OutPoint {
                txid,
                vout: i as u32,
            })
            .collect())
    }

    /// make_outpoints() kept as alias to v2 for v0.2 default
    pub fn make_outpoints(tx: &Transaction) -> Result<Vec<OutPoint>, UtxoError> {
        Transaction::make_outpoints_v2(tx)
    }
}

/// Compute a signing hash (sighash) over canonical bytes v2 with all script_sig cleared.
/// This is a simple baseline scheme for v0.1 tooling.
pub fn tx_sighash_v2(tx: &Transaction) -> Result<[u8; 32], UtxoError> {
    let mut tmp = tx.clone();
    for vin in &mut tmp.vin {
        vin.script_sig.clear();
    }
    let bytes = tmp.canonical_bytes_v2()?;
    let first = Sha256::digest(&bytes);
    let second = Sha256::digest(&first);
    let mut out = [0u8; 32];
    out.copy_from_slice(&second);
    Ok(out)
}

/// Receipt describing changes from an apply_tx (for rollback)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyReceipt {
    pub removed: Vec<(OutPoint, TxOut)>,
    pub inserted: Vec<OutPoint>,
}

/// UTXO set trait
pub trait UtxoSet {
    fn get(&self, outpoint: &OutPoint) -> Option<TxOut>;
    fn insert(&mut self, outpoint: OutPoint, txout: TxOut);
    fn remove(&mut self, outpoint: &OutPoint) -> Option<TxOut>;

    /// Apply a transaction atomically, returning an ApplyReceipt for possible rollback
    fn apply_tx(&mut self, tx: &Transaction) -> Result<ApplyReceipt, UtxoError>;

    /// Rollback a previous ApplyReceipt
    fn rollback(&mut self, receipt: ApplyReceipt) -> Result<(), UtxoError>;
}

/// In-memory HashMap-backed UTXO set
#[derive(Debug, Default)]
pub struct InMemoryUtxoSet {
    map: HashMap<OutPoint, TxOut>,
}

impl InMemoryUtxoSet {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn entries(&self) -> Vec<(OutPoint, TxOut)> {
        self.map
            .iter()
            .map(|(op, txout)| (op.clone(), txout.clone()))
            .collect()
    }
}

impl UtxoSet for InMemoryUtxoSet {
    fn get(&self, outpoint: &OutPoint) -> Option<TxOut> {
        self.map.get(outpoint).cloned()
    }

    fn insert(&mut self, outpoint: OutPoint, txout: TxOut) {
        self.map.insert(outpoint, txout);
    }

    fn remove(&mut self, outpoint: &OutPoint) -> Option<TxOut> {
        self.map.remove(outpoint)
    }

    fn apply_tx(&mut self, tx: &Transaction) -> Result<ApplyReceipt, UtxoError> {
        // Validate basic properties + value conservation
        tx.validate()?;
        let _fee = Transaction::validate_value_conservation(tx, &*self)?;

        // Collect removed UTXOs by attempting to remove them one-by-one (so we can simulate mid-failure)
        let mut removed: Vec<(OutPoint, TxOut)> = Vec::new();
        for vin in &tx.vin {
            match self.remove(&vin.prevout) {
                Some(txout) => removed.push((vin.prevout.clone(), txout)),
                None => {
                    // rollback any prior removals
                    for (op, to) in removed.iter().rev() {
                        self.insert(op.clone(), to.clone());
                    }
                    return Err(UtxoError::MissingUtxo(vin.prevout.clone()));
                }
            }
        }

        // Insert new outputs; track inserted outpoints
        let outpoints = Transaction::make_outpoints(tx)?;
        let mut inserted: Vec<OutPoint> = Vec::new();
        for (op, txout) in outpoints.into_iter().zip(tx.vout.iter()) {
            if self.map.contains_key(&op) {
                // collision -> rollback
                let receipt = ApplyReceipt {
                    removed: removed.clone(),
                    inserted: inserted.clone(),
                };
                let _ = self.rollback(receipt.clone());
                return Err(UtxoError::DuplicateOutput(op));
            }
            self.insert(op.clone(), txout.clone());
            inserted.push(op);
        }

        Ok(ApplyReceipt { removed, inserted })
    }

    fn rollback(&mut self, receipt: ApplyReceipt) -> Result<(), UtxoError> {
        // remove inserted
        for op in &receipt.inserted {
            self.remove(op);
        }
        // re-insert removed
        for (op, txout) in receipt.removed {
            self.insert(op, txout);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tx_serde_json_roundtrip() -> Result<(), UtxoError> {
        let outpoint = OutPoint {
            txid: [0u8; 32],
            vout: 0,
        };
        let txin = TxIn {
            prevout: outpoint,
            script_sig: b"hello".to_vec(),
            sequence: 0xffffffff,
        };
        let txout = TxOut {
            value: 50_000,
            script_pubkey: b"world".to_vec(),
        };
        let tx = Transaction {
            version: 1,
            vin: vec![txin],
            vout: vec![txout],
            lock_time: 0,
        };

        let bytes = tx.to_json_bytes()?;
        let tx2 = Transaction::from_json_bytes(&bytes)?;
        assert_eq!(tx, tx2);
        Ok(())
    }

    #[test]
    fn script_length_limit_errs() {
        // build a tx with too-large script_sig
        let outpoint = OutPoint {
            txid: [1u8; 32],
            vout: 1,
        };
        let too_long = vec![0u8; MAX_SCRIPT_SIZE + 1];
        let txin = TxIn {
            prevout: outpoint,
            script_sig: too_long,
            sequence: 0,
        };
        let tx = Transaction {
            version: 1,
            vin: vec![txin],
            vout: vec![],
            lock_time: 0,
        };

        let res = tx.validate();
        assert!(res.is_err());
        match res.err().unwrap() {
            UtxoError::TooLargeScript(len, max) => {
                assert_eq!(len, MAX_SCRIPT_SIZE + 1);
                assert_eq!(max, MAX_SCRIPT_SIZE);
            }
            e => panic!("unexpected error: {:?}", e),
        }
    }

    #[test]
    fn in_memory_utxo_set_ops() {
        let mut set = InMemoryUtxoSet::new();
        let outpoint = OutPoint {
            txid: [2u8; 32],
            vout: 2,
        };
        let txout = TxOut {
            value: 100,
            script_pubkey: b"pk".to_vec(),
        };

        assert!(set.get(&outpoint).is_none());
        set.insert(outpoint.clone(), txout.clone());
        assert_eq!(set.get(&outpoint), Some(txout.clone()));
        assert_eq!(set.remove(&outpoint), Some(txout.clone()));
        assert!(set.get(&outpoint).is_none());
    }

    #[test]
    fn overflow_on_outputs() {
        let tx = Transaction {
            version: 1,
            vin: vec![],
            vout: vec![
                TxOut {
                    value: u64::MAX,
                    script_pubkey: vec![],
                },
                TxOut {
                    value: 1,
                    script_pubkey: vec![],
                },
            ],
            lock_time: 0,
        };
        match Transaction::sum_outputs(&tx) {
            Err(UtxoError::Overflow) => (),
            other => panic!("expected Overflow, got {:?}", other),
        }
    }

    #[test]
    fn missing_utxo_error() {
        let tx = Transaction {
            version: 1,
            vin: vec![TxIn {
                prevout: OutPoint {
                    txid: [9u8; 32],
                    vout: 0,
                },
                script_sig: vec![],
                sequence: 0,
            }],
            vout: vec![],
            lock_time: 0,
        };
        let set = InMemoryUtxoSet::new();
        match Transaction::sum_inputs(&tx, &set) {
            Err(UtxoError::MissingUtxo(op)) => assert_eq!(op, tx.vin[0].prevout),
            other => panic!("expected MissingUtxo, got {:?}", other),
        }
    }

    #[test]
    fn value_not_conserved_error() {
        let mut set = InMemoryUtxoSet::new();
        let in_op = OutPoint {
            txid: [3u8; 32],
            vout: 0,
        };
        set.insert(
            in_op.clone(),
            TxOut {
                value: 10,
                script_pubkey: vec![],
            },
        );
        let tx = Transaction {
            version: 1,
            vin: vec![TxIn {
                prevout: in_op,
                script_sig: vec![],
                sequence: 0,
            }],
            vout: vec![TxOut {
                value: 20,
                script_pubkey: vec![],
            }],
            lock_time: 0,
        };
        match Transaction::validate_value_conservation(&tx, &set) {
            Err(UtxoError::ValueNotConserved { input, output }) => {
                assert_eq!(input, 10);
                assert_eq!(output, 20);
            }
            other => panic!("expected ValueNotConserved, got {:?}", other),
        }
    }

    #[test]
    fn duplicate_input_error() {
        let op = OutPoint {
            txid: [4u8; 32],
            vout: 0,
        };
        let tx = Transaction {
            version: 1,
            vin: vec![
                TxIn {
                    prevout: op.clone(),
                    script_sig: vec![],
                    sequence: 0,
                },
                TxIn {
                    prevout: op.clone(),
                    script_sig: vec![],
                    sequence: 0,
                },
            ],
            vout: vec![],
            lock_time: 0,
        };
        let set = InMemoryUtxoSet::new();
        match Transaction::validate_value_conservation(&tx, &set) {
            Err(UtxoError::DuplicateInput(dup)) => assert_eq!(dup, op),
            other => panic!("expected DuplicateInput, got {:?}", other),
        }
    }

    #[test]
    fn apply_tx_success_and_state() {
        let mut set = InMemoryUtxoSet::new();
        let in_op = OutPoint {
            txid: [5u8; 32],
            vout: 0,
        };
        set.insert(
            in_op.clone(),
            TxOut {
                value: 100,
                script_pubkey: b"a".to_vec(),
            },
        );

        let tx = Transaction {
            version: 1,
            vin: vec![TxIn {
                prevout: in_op.clone(),
                script_sig: vec![],
                sequence: 0,
            }],
            vout: vec![
                TxOut {
                    value: 60,
                    script_pubkey: b"b".to_vec(),
                },
                TxOut {
                    value: 39,
                    script_pubkey: b"c".to_vec(),
                },
            ],
            lock_time: 0,
        };

        let txid = tx.txid().expect("txid should compute");
        let receipt = set.apply_tx(&tx).expect("apply_tx should succeed");
        // original spent
        assert!(set.get(&in_op).is_none());
        // new outputs present
        let outpoints =
            Transaction::make_outpoints(&tx).expect("make_outpoints should compute txid");
        assert_eq!(outpoints[0].txid, txid);
        assert_eq!(outpoints[1].txid, txid);
        assert_eq!(set.get(&outpoints[0]).unwrap().value, 60);
        assert_eq!(set.get(&outpoints[1]).unwrap().value, 39);
        // receipt reflects changes
        assert_eq!(receipt.removed.len(), 1);
        assert_eq!(receipt.inserted.len(), 2);
    }

    #[test]
    fn apply_tx_atomicity_on_collision() {
        // Prepare inputs
        let mut set = InMemoryUtxoSet::new();
        let in1 = OutPoint {
            txid: [6u8; 32],
            vout: 0,
        };
        let in2 = OutPoint {
            txid: [6u8; 32],
            vout: 1,
        };
        set.insert(
            in1.clone(),
            TxOut {
                value: 50,
                script_pubkey: vec![],
            },
        );
        set.insert(
            in2.clone(),
            TxOut {
                value: 50,
                script_pubkey: vec![],
            },
        );

        // We'll pre-insert a UTXO at the future tx's outpoint (txid,vout=1) to force a collision
        // Compute txid for the tx we will apply and insert that outpoint to simulate collision
        // (We compute txid here to ensure the pre-insert uses the same canonical txid)
        let tx = Transaction {
            version: 1,
            vin: vec![
                TxIn {
                    prevout: in1.clone(),
                    script_sig: vec![],
                    sequence: 0,
                },
                TxIn {
                    prevout: in2.clone(),
                    script_sig: vec![],
                    sequence: 0,
                },
            ],
            vout: vec![
                TxOut {
                    value: 60,
                    script_pubkey: vec![],
                },
                TxOut {
                    value: 39,
                    script_pubkey: vec![],
                },
            ],
            lock_time: 0,
        };

        let txid = tx.txid().expect("txid should compute");
        let collision = OutPoint { txid, vout: 1 };
        set.insert(
            collision.clone(),
            TxOut {
                value: 999,
                script_pubkey: vec![],
            },
        );

        let _res = set.apply_tx(&tx);

        let tx = Transaction {
            version: 1,
            vin: vec![
                TxIn {
                    prevout: in1.clone(),
                    script_sig: vec![],
                    sequence: 0,
                },
                TxIn {
                    prevout: in2.clone(),
                    script_sig: vec![],
                    sequence: 0,
                },
            ],
            vout: vec![
                TxOut {
                    value: 60,
                    script_pubkey: vec![],
                },
                TxOut {
                    value: 39,
                    script_pubkey: vec![],
                },
            ],
            lock_time: 0,
        };

        let res = set.apply_tx(&tx);
        assert!(res.is_err());
        // ensure original UTXOs still exist (rolled back)
        assert_eq!(set.get(&in1).unwrap().value, 50);
        assert_eq!(set.get(&in2).unwrap().value, 50);
        // ensure collision remained intact
        assert_eq!(set.get(&collision).unwrap().value, 999);
    }

    #[test]
    fn cross_language_vectors_match() -> Result<(), UtxoError> {
        // load generated vectors and verify canonical bytes + txid for v1 and v2
        let s = include_str!("../test_vectors/vectors.json");
        // Some Windows tools may write a BOM; tolerate it by trimming.
        let s = s.trim_start_matches('\u{FEFF}');
        let vecs: serde_json::Value =
            serde_json::from_str(s).map_err(|e| UtxoError::SerdeError(e))?;
        for v in vecs.as_array().unwrap() {
            let name = v["name"].as_str().unwrap();
            let tx_value = &v["tx"];
            let tx: Transaction =
                serde_json::from_value(tx_value.clone()).map_err(|e| UtxoError::SerdeError(e))?;

            // v2 checks
            let c2 = tx.canonical_bytes_v2()?;
            let txid2 = tx.txid_v2()?;
            assert_eq!(
                hex::encode(&c2),
                v["canonical_v2"].as_str().unwrap(),
                "canonical_v2 mismatch for {}",
                name
            );
            assert_eq!(
                hex::encode(&txid2),
                v["txid_v2"].as_str().unwrap(),
                "txid_v2 mismatch for {}",
                name
            );

            // v1 checks
            let c1 = tx.canonical_bytes_v1()?;
            let txid1 = tx.txid_v1()?;
            assert_eq!(
                hex::encode(&c1),
                v["canonical_v1"].as_str().unwrap(),
                "canonical_v1 mismatch for {}",
                name
            );
            assert_eq!(
                hex::encode(&txid1),
                v["txid_v1"].as_str().unwrap(),
                "txid_v1 mismatch for {}",
                name
            );
        }
        Ok(())
    }
}
