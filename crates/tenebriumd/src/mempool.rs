use std::collections::{HashMap, HashSet};
use tenebrium_utxo::{OutPoint, Transaction, UtxoError, UtxoSet};

#[derive(Debug, Clone)]
pub struct MempoolConfig {
    pub max_txs: usize,
    pub max_total_bytes: usize,
    pub min_fee_rate: f64,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            max_txs: 10_000,
            max_total_bytes: 50 * 1024 * 1024,
            min_fee_rate: 0.0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MempoolError {
    #[error("UTXO error: {0}")]
    Utxo(#[from] UtxoError),
    #[error("duplicate txid")]
    DuplicateTx,
    #[error("double spend in mempool: {0:?}")]
    DoubleSpend(OutPoint),
    #[error("mempool full")]
    Full,
    #[error("mempool bytes limit exceeded")]
    BytesLimit,
    #[error("fee rate too low")]
    LowFee,
}

#[derive(Debug, Clone)]
pub struct MempoolEntry {
    pub tx: Transaction,
    pub txid_v1: [u8; 32],
    pub txid_v2: [u8; 32],
    pub fee: u64,
    pub size_bytes: usize,
}

#[derive(Debug, Default)]
pub struct Mempool {
    cfg: MempoolConfig,
    map_v2: HashMap<[u8; 32], MempoolEntry>,
    map_v1: HashMap<[u8; 32], [u8; 32]>,
    spent: HashSet<OutPoint>,
    total_bytes: usize,
}

impl Mempool {
    pub fn new(cfg: MempoolConfig) -> Self {
        Self {
            cfg,
            map_v2: HashMap::new(),
            map_v1: HashMap::new(),
            spent: HashSet::new(),
            total_bytes: 0,
        }
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.map_v2.len()
    }

    #[allow(dead_code)]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn contains(&self, txid: &[u8; 32]) -> bool {
        self.map_v2.contains_key(txid)
    }

    pub fn contains_v1(&self, txid: &[u8; 32]) -> bool {
        self.map_v1.contains_key(txid)
    }

    pub fn get_tx(&self, txid: &[u8; 32]) -> Option<Transaction> {
        self.map_v2.get(txid).map(|entry| entry.tx.clone())
    }

    pub fn get_tx_v1(&self, txid: &[u8; 32]) -> Option<Transaction> {
        let v2 = self.map_v1.get(txid)?;
        self.map_v2.get(v2).map(|entry| entry.tx.clone())
    }

    pub fn add_tx(&mut self, tx: Transaction, utxos: &dyn UtxoSet) -> Result<(), MempoolError> {
        let txid_v1 = tx.txid_v1()?;
        let txid_v2 = tx.txid_v2()?;
        if self.map_v2.contains_key(&txid_v2) || self.map_v1.contains_key(&txid_v1) {
            return Err(MempoolError::DuplicateTx);
        }

        for vin in &tx.vin {
            if self.spent.contains(&vin.prevout) {
                return Err(MempoolError::DoubleSpend(vin.prevout.clone()));
            }
        }

        let fee = Transaction::validate_value_conservation(&tx, utxos)?;
        let size_bytes = tx.canonical_bytes_v2()?.len();
        let fee_rate = if size_bytes == 0 {
            0.0
        } else {
            fee as f64 / size_bytes as f64
        };
        if fee_rate < self.cfg.min_fee_rate {
            return Err(MempoolError::LowFee);
        }

        if self.map_v2.len() + 1 > self.cfg.max_txs
            || self.total_bytes + size_bytes > self.cfg.max_total_bytes
        {
            self.evict_low_fee()?;
            if self.map_v2.len() + 1 > self.cfg.max_txs {
                return Err(MempoolError::Full);
            }
            if self.total_bytes + size_bytes > self.cfg.max_total_bytes {
                return Err(MempoolError::BytesLimit);
            }
        }

        for vin in &tx.vin {
            self.spent.insert(vin.prevout.clone());
        }
        self.total_bytes += size_bytes;
        self.map_v1.insert(txid_v1, txid_v2);
        self.map_v2.insert(
            txid_v2,
            MempoolEntry {
                tx,
                txid_v1,
                txid_v2,
                fee,
                size_bytes,
            },
        );
        Ok(())
    }

    pub fn remove_tx(&mut self, txid: &[u8; 32]) -> Option<MempoolEntry> {
        let entry = self.map_v2.remove(txid)?;
        self.map_v1.remove(&entry.txid_v1);
        for vin in &entry.tx.vin {
            self.spent.remove(&vin.prevout);
        }
        self.total_bytes = self.total_bytes.saturating_sub(entry.size_bytes);
        Some(entry)
    }

    pub fn remove_tx_v1(&mut self, txid: &[u8; 32]) -> Option<MempoolEntry> {
        let v2 = self.map_v1.get(txid).copied()?;
        self.remove_tx(&v2)
    }

    #[allow(dead_code)]
    pub fn all_txids(&self) -> Vec<[u8; 32]> {
        self.map_v2.keys().cloned().collect()
    }

    pub fn entries(&self) -> Vec<MempoolEntry> {
        self.map_v2.values().cloned().collect()
    }

    fn evict_low_fee(&mut self) -> Result<(), MempoolError> {
        if self.map_v2.is_empty() {
            return Ok(());
        }
        let mut entries: Vec<MempoolEntry> = self.map_v2.values().cloned().collect();
        entries.sort_by(|a, b| fee_rate(a).partial_cmp(&fee_rate(b)).unwrap_or(std::cmp::Ordering::Equal));
        for entry in entries {
            self.remove_tx(&entry.txid_v2);
            if self.map_v2.len() < self.cfg.max_txs && self.total_bytes < self.cfg.max_total_bytes {
                break;
            }
        }
        Ok(())
    }
}

fn fee_rate(entry: &MempoolEntry) -> f64 {
    if entry.size_bytes == 0 {
        return 0.0;
    }
    entry.fee as f64 / entry.size_bytes as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tenebrium_utxo::{InMemoryUtxoSet, TxIn, TxOut};

    fn sample_utxo() -> (InMemoryUtxoSet, OutPoint) {
        let mut set = InMemoryUtxoSet::new();
        let outpoint = OutPoint {
            txid: [7u8; 32],
            vout: 0,
        };
        let txout = TxOut {
            value: 1_000,
            script_pubkey: vec![1, 2, 3],
        };
        set.insert(outpoint.clone(), txout);
        (set, outpoint)
    }

    fn make_tx(prev: OutPoint, value: u64) -> Transaction {
        Transaction {
            version: 1,
            vin: vec![TxIn {
                prevout: prev,
                script_sig: vec![],
                sequence: 0xffff_ffff,
            }],
            vout: vec![TxOut {
                value,
                script_pubkey: vec![4, 5, 6],
            }],
            lock_time: 0,
        }
    }

    #[test]
    fn add_and_remove_tx() {
        let (utxos, outpoint) = sample_utxo();
        let tx = make_tx(outpoint.clone(), 900);
        let mut mempool = Mempool::new(MempoolConfig::default());
        mempool.add_tx(tx.clone(), &utxos).unwrap();
        let txid = tx.txid_v2().unwrap();
        assert!(mempool.contains(&txid));
        let removed = mempool.remove_tx(&txid).unwrap();
        assert_eq!(removed.txid_v2, txid);
        assert!(!mempool.contains(&txid));
    }

    #[test]
    fn double_spend_rejected() {
        let (utxos, outpoint) = sample_utxo();
        let tx1 = make_tx(outpoint.clone(), 900);
        let tx2 = make_tx(outpoint.clone(), 800);
        let mut mempool = Mempool::new(MempoolConfig::default());
        mempool.add_tx(tx1, &utxos).unwrap();
        let err = mempool.add_tx(tx2, &utxos).unwrap_err();
        matches!(err, MempoolError::DoubleSpend(_));
    }
}
