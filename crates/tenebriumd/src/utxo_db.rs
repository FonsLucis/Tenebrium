use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tenebrium_utxo::{OutPoint, TxOut};

#[derive(Debug, thiserror::Error)]
pub enum UtxoDbError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[allow(dead_code)]
    #[error("invalid data: {0}")]
    InvalidData(String),
    #[error("sled error: {0}")]
    Sled(#[from] sled::Error),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UtxoEntry {
    pub outpoint: OutPoint,
    pub txout: TxOut,
}

#[allow(dead_code)]
pub trait UtxoStore {
    fn get(&self, outpoint: &OutPoint) -> Result<Option<TxOut>, UtxoDbError>;
    fn put(&mut self, outpoint: &OutPoint, txout: &TxOut) -> Result<(), UtxoDbError>;
    fn remove(&mut self, outpoint: &OutPoint) -> Result<(), UtxoDbError>;
}

/// KV-store adapter skeleton (backend wiring to be implemented).
#[allow(dead_code)]
pub struct KvUtxoStore {
    db: sled::Db,
}

#[allow(dead_code)]
impl KvUtxoStore {
    pub fn open(path: PathBuf) -> Result<Self, UtxoDbError> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }
}

impl UtxoStore for KvUtxoStore {
    fn get(&self, _outpoint: &OutPoint) -> Result<Option<TxOut>, UtxoDbError> {
        let key = encode_outpoint(_outpoint);
        match self.db.get(key)? {
            Some(ivec) => decode_txout(&ivec),
            None => Ok(None),
        }
    }

    fn put(&mut self, _outpoint: &OutPoint, _txout: &TxOut) -> Result<(), UtxoDbError> {
        let key = encode_outpoint(_outpoint);
        let value = encode_txout(_txout);
        self.db.insert(key, value)?;
        Ok(())
    }

    fn remove(&mut self, _outpoint: &OutPoint) -> Result<(), UtxoDbError> {
        let key = encode_outpoint(_outpoint);
        self.db.remove(key)?;
        Ok(())
    }
}

pub trait UtxoReader {
    fn for_each<F>(&self, f: F) -> Result<(), UtxoDbError>
    where
        F: FnMut(UtxoEntry) -> Result<(), UtxoDbError>;
}

pub struct JsonlUtxoReader {
    path: PathBuf,
}

impl JsonlUtxoReader {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn open(&self) -> Result<BufReader<std::fs::File>, UtxoDbError> {
        let file = std::fs::File::open(&self.path)?;
        Ok(BufReader::new(file))
    }
}

impl UtxoReader for JsonlUtxoReader {
    fn for_each<F>(&self, mut f: F) -> Result<(), UtxoDbError>
    where
        F: FnMut(UtxoEntry) -> Result<(), UtxoDbError>,
    {
        let reader = self.open()?;
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let entry: UtxoEntry = serde_json::from_str(trimmed)?;
            f(entry)?;
        }
        Ok(())
    }
}

pub fn jsonl_reader(path: &Path) -> JsonlUtxoReader {
    JsonlUtxoReader::new(path.to_path_buf())
}

pub fn encode_outpoint(outpoint: &OutPoint) -> Vec<u8> {
    let mut buf = Vec::with_capacity(36);
    buf.extend_from_slice(&outpoint.txid);
    buf.extend_from_slice(&outpoint.vout.to_le_bytes());
    buf
}

pub fn encode_txout(txout: &TxOut) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16 + txout.script_pubkey.len());
    buf.extend_from_slice(&txout.value.to_le_bytes());
    buf.extend_from_slice(&(txout.script_pubkey.len() as u64).to_le_bytes());
    buf.extend_from_slice(&txout.script_pubkey);
    buf
}

pub fn decode_txout(bytes: &[u8]) -> Result<Option<TxOut>, UtxoDbError> {
    if bytes.len() < 16 {
        return Err(UtxoDbError::InvalidData("txout too short".to_string()));
    }
    let value = u64::from_le_bytes(bytes[0..8].try_into().map_err(|_| {
        UtxoDbError::InvalidData("invalid value bytes".to_string())
    })?);
    let len = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| {
        UtxoDbError::InvalidData("invalid script length bytes".to_string())
    })?) as usize;
    if bytes.len() < 16 + len {
        return Err(UtxoDbError::InvalidData("script length exceeds buffer".to_string()));
    }
    let script_pubkey = bytes[16..16 + len].to_vec();
    Ok(Some(TxOut { value, script_pubkey }))
}

pub fn decode_outpoint(bytes: &[u8]) -> Result<OutPoint, UtxoDbError> {
    if bytes.len() != 36 {
        return Err(UtxoDbError::InvalidData("outpoint length mismatch".to_string()));
    }
    let mut txid = [0u8; 32];
    txid.copy_from_slice(&bytes[0..32]);
    let vout = u32::from_le_bytes(bytes[32..36].try_into().map_err(|_| {
        UtxoDbError::InvalidData("invalid vout bytes".to_string())
    })?);
    Ok(OutPoint { txid, vout })
}

#[allow(dead_code)]
pub fn decode_utxo_entry(
    outpoint_bytes: &[u8],
    txout_bytes: &[u8],
) -> Result<(OutPoint, TxOut), UtxoDbError> {
    let outpoint = decode_outpoint(outpoint_bytes)?;
    let txout = decode_txout(txout_bytes)?
        .ok_or_else(|| UtxoDbError::InvalidData("txout decode failed".to_string()))?;
    Ok((outpoint, txout))
}
