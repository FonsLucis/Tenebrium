use crate::mempool::{Mempool, MempoolEntry};
use tenebrium_consensus::{Block, ConsensusError};
use tenebrium_utxo::Transaction;

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("consensus error: {0}")]
    Consensus(#[from] ConsensusError),
    #[error("block size limit exceeded")]
    SizeLimit,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BlockTemplate {
    pub block: Block,
    pub total_fees: u64,
    pub total_size: usize,
}

pub fn build_block_template(
    mempool: &Mempool,
    coinbase: Transaction,
    prev_block_hash: [u8; 32],
    time: u32,
    bits: u32,
    version: i32,
    max_block_bytes: usize,
) -> Result<BlockTemplate, TemplateError> {
    let mut txs = Vec::new();
    let mut total_fees = 0u64;
    let mut total_size = 0usize;

    let mut entries = mempool.entries();
    entries.sort_by(|a, b| {
        let rate_cmp = fee_rate(b)
            .partial_cmp(&fee_rate(a))
            .unwrap_or(std::cmp::Ordering::Equal);
        if rate_cmp == std::cmp::Ordering::Equal {
            a.txid_v2.cmp(&b.txid_v2)
        } else {
            rate_cmp
        }
    });

    txs.push(coinbase.clone());
    total_size += coinbase.canonical_bytes_v2().map_err(ConsensusError::Utxo)?.len();

    for entry in entries {
        if total_size + entry.size_bytes > max_block_bytes {
            continue;
        }
        total_fees = total_fees.saturating_add(entry.fee);
        total_size += entry.size_bytes;
        txs.push(entry.tx.clone());
    }

    if total_size > max_block_bytes {
        return Err(TemplateError::SizeLimit);
    }

    let block = Block::new(version, prev_block_hash, time, bits, 0, txs)?;
    Ok(BlockTemplate {
        block,
        total_fees,
        total_size,
    })
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
    use crate::mempool::MempoolConfig;
    use tenebrium_utxo::{InMemoryUtxoSet, OutPoint, TxIn, TxOut, UtxoSet};

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
    fn build_template_includes_mempool_txs() {
        let mut utxos = InMemoryUtxoSet::new();
        let outpoint = OutPoint {
            txid: [9u8; 32],
            vout: 0,
        };
        utxos.insert(
            outpoint.clone(),
            TxOut {
                value: 2_000,
                script_pubkey: vec![1],
            },
        );

        let tx = make_tx(outpoint.clone(), 1_500);
        let mut mempool = Mempool::new(MempoolConfig::default());
        mempool.add_tx(tx.clone(), &utxos).unwrap();

        let coinbase = Transaction {
            version: 1,
            vin: vec![],
            vout: vec![TxOut {
                value: 500,
                script_pubkey: vec![0],
            }],
            lock_time: 0,
        };

        let template = build_block_template(
            &mempool,
            coinbase,
            [0u8; 32],
            0,
            0,
            1,
            1_000_000,
        )
        .unwrap();

        assert_eq!(template.block.txs.len(), 2);
    }
}
