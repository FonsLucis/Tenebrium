use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tenebrium_utxo::Transaction;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockHeader {
	pub version: i32,
	pub prev_block_hash: [u8; 32],
	pub merkle_root: [u8; 32],
	pub time: u32,
	pub bits: u32,
	pub nonce: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
	pub header: BlockHeader,
	pub txs: Vec<Transaction>,
}

impl Block {
	pub fn new(
		version: i32,
		prev_block_hash: [u8; 32],
		time: u32,
		bits: u32,
		nonce: u32,
		txs: Vec<Transaction>,
	) -> Result<Self, ConsensusError> {
		let txids = txs
			.iter()
			.map(|tx| tx.txid_v2())
			.collect::<Result<Vec<_>, _>>()
			.map_err(ConsensusError::Utxo)?;
		let merkle_root = merkle_root(&txids);
		Ok(Block {
			header: BlockHeader {
				version,
				prev_block_hash,
				merkle_root,
				time,
				bits,
				nonce,
			},
			txs,
		})
	}
}

#[derive(Debug, thiserror::Error)]
pub enum ConsensusError {
	#[error("UTXO error: {0}")]
	Utxo(#[from] tenebrium_utxo::UtxoError),
	#[error("invalid bits")]
	InvalidBits,
}

pub fn header_hash(header: &BlockHeader) -> [u8; 32] {
	let mut bytes = Vec::with_capacity(4 + 32 + 32 + 4 + 4 + 4);
	bytes.extend_from_slice(&header.version.to_le_bytes());
	bytes.extend_from_slice(&header.prev_block_hash);
	bytes.extend_from_slice(&header.merkle_root);
	bytes.extend_from_slice(&header.time.to_le_bytes());
	bytes.extend_from_slice(&header.bits.to_le_bytes());
	bytes.extend_from_slice(&header.nonce.to_le_bytes());
	let first = Sha256::digest(&bytes);
	let second = Sha256::digest(&first);
	let mut out = [0u8; 32];
	out.copy_from_slice(&second);
	out
}

pub fn bits_to_target(bits: u32) -> Result<[u8; 32], ConsensusError> {
	if bits == 0 {
		return Err(ConsensusError::InvalidBits);
	}
	let exponent = (bits >> 24) as u32;
	let mantissa = bits & 0x007f_ffff;
	if mantissa == 0 {
		return Err(ConsensusError::InvalidBits);
	}

	let mut target = [0u8; 32];
	if exponent <= 3 {
		let shift = 8 * (3 - exponent);
		let value = mantissa >> shift;
		let bytes = value.to_be_bytes();
		let start = 32 - exponent as usize;
		let copy_start = 4 - exponent as usize;
		target[start..].copy_from_slice(&bytes[copy_start..]);
	} else {
		let start = 32usize
			.checked_sub(exponent as usize)
			.ok_or(ConsensusError::InvalidBits)?;
		if start + 3 > 32 {
			return Err(ConsensusError::InvalidBits);
		}
		let mantissa_bytes = [
			((mantissa >> 16) & 0xff) as u8,
			((mantissa >> 8) & 0xff) as u8,
			(mantissa & 0xff) as u8,
		];
		target[start..start + 3].copy_from_slice(&mantissa_bytes);
	}
	Ok(target)
}

pub fn check_pow(header: &BlockHeader) -> Result<bool, ConsensusError> {
	let target = bits_to_target(header.bits)?;
	let hash = header_hash(header);
	Ok(hash_leq(&hash, &target))
}

pub fn mine_header(header: &mut BlockHeader, max_nonce: u32) -> Result<Option<u32>, ConsensusError> {
	for _ in 0..=max_nonce {
		if check_pow(header)? {
			return Ok(Some(header.nonce));
		}
		header.nonce = header.nonce.wrapping_add(1);
	}
	Ok(None)
}

fn hash_leq(a: &[u8; 32], b: &[u8; 32]) -> bool {
	for i in 0..32 {
		if a[i] < b[i] {
			return true;
		}
		if a[i] > b[i] {
			return false;
		}
	}
	true
}

pub fn merkle_root(txids: &[[u8; 32]]) -> [u8; 32] {
	if txids.is_empty() {
		return [0u8; 32];
	}
	let mut level = txids.to_vec();
	while level.len() > 1 {
		let mut next = Vec::with_capacity((level.len() + 1) / 2);
		let mut i = 0;
		while i < level.len() {
			let left = level[i];
			let right = if i + 1 < level.len() {
				level[i + 1]
			} else {
				level[i]
			};
			let mut data = Vec::with_capacity(64);
			data.extend_from_slice(&left);
			data.extend_from_slice(&right);
			let first = Sha256::digest(&data);
			let second = Sha256::digest(&first);
			let mut out = [0u8; 32];
			out.copy_from_slice(&second);
			next.push(out);
			i += 2;
		}
		level = next;
	}
	level[0]
}

#[cfg(test)]
mod tests {
	use super::*;
	use tenebrium_utxo::{OutPoint, TxIn, TxOut};

	fn tx_with_id(byte: u8) -> Transaction {
		Transaction {
			version: 1,
			vin: vec![TxIn {
				prevout: OutPoint {
					txid: [byte; 32],
					vout: 0,
				},
				script_sig: vec![],
				sequence: 0,
			}],
			vout: vec![TxOut {
				value: 1,
				script_pubkey: vec![1, 2],
			}],
			lock_time: 0,
		}
	}

	#[test]
	fn merkle_root_single() {
		let tx = tx_with_id(1);
		let txid = tx.txid_v2().unwrap();
		let root = merkle_root(&[txid]);
		assert_eq!(root, txid);
	}

	#[test]
	fn merkle_root_two() {
		let tx1 = tx_with_id(1);
		let tx2 = tx_with_id(2);
		let txids = vec![tx1.txid_v2().unwrap(), tx2.txid_v2().unwrap()];
		let root = merkle_root(&txids);
		assert_ne!(root, [0u8; 32]);
	}

	#[test]
	fn bits_to_target_bitcoin_style() {
		let target = bits_to_target(0x1d00ffff).unwrap();
		let expected = [
			0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		];
		assert_eq!(target, expected);
	}

	#[test]
	fn pow_check_easy() {
		let header = BlockHeader {
			version: 1,
			prev_block_hash: [0u8; 32],
			merkle_root: [1u8; 32],
			time: 0,
			bits: 0x207fffff,
			nonce: 0,
		};
		let ok = check_pow(&header).unwrap();
		assert!(ok);
	}
}
