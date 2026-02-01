use crate::{OutPoint, Transaction, UtxoError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReindexErrorKind {
    MissingTx,
    InvalidTx,
    DuplicateOutPoint,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReindexErrorEntry {
    pub kind: ReindexErrorKind,
    pub txid_v1: Option<[u8; 32]>,
    pub message: String,
}

impl ReindexErrorEntry {
    pub fn new(kind: ReindexErrorKind, txid_v1: Option<[u8; 32]>, message: impl Into<String>) -> Self {
        Self {
            kind,
            txid_v1,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReindexReport {
    pub started_at: String,
    pub finished_at: Option<String>,
    pub total_inputs: u64,
    pub total_outputs: u64,
    pub skipped: u64,
    pub errors: Vec<ReindexErrorEntry>,
}

impl ReindexReport {
    pub fn new(started_at: impl Into<String>) -> Self {
        Self {
            started_at: started_at.into(),
            finished_at: None,
            total_inputs: 0,
            total_outputs: 0,
            skipped: 0,
            errors: Vec::new(),
        }
    }

    pub fn finish(&mut self, finished_at: impl Into<String>) {
        self.finished_at = Some(finished_at.into());
    }

    pub fn record_error(&mut self, entry: ReindexErrorEntry) {
        self.errors.push(entry);
    }
}

/// Map v1 outpoints to v2 outpoints for the given transaction.
pub fn map_outpoints_v1_to_v2(
    tx: &Transaction,
) -> Result<Vec<(OutPoint, OutPoint)>, UtxoError> {
    let txid_v1 = tx.txid_v1()?;
    let txid_v2 = tx.txid_v2()?;

    let mut out = Vec::with_capacity(tx.vout.len());
    for (i, _) in tx.vout.iter().enumerate() {
        let vout = i as u32;
        out.push((
            OutPoint { txid: txid_v1, vout },
            OutPoint { txid: txid_v2, vout },
        ));
    }
    Ok(out)
}
