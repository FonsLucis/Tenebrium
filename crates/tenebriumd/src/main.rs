mod block_template;
mod mempool;
mod p2p;
mod utxo_db;

use clap::{Parser, Subcommand, ValueEnum};
use serde::{de::SeqAccess, de::Visitor, Deserialize, Serialize};
use serde::de::Deserializer as _;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tenebriumd::LogLevel;
use block_template::build_block_template;
use mempool::{Mempool, MempoolConfig};
use tenebrium_consensus::{check_pow, merkle_root, mine_header};
use tenebrium_utxo::{
    map_outpoints_v1_to_v2, OutPoint, ReindexErrorEntry, ReindexErrorKind, ReindexReport,
    Transaction, UtxoError, InMemoryUtxoSet, UtxoSet,
};
use utxo_db::{jsonl_reader, KvUtxoStore, UtxoDbError, UtxoEntry, UtxoReader, UtxoStore};

#[derive(Debug, Parser)]
#[command(name = "tenebriumd", version, about = "Tenebrium node daemon")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Reindex UTXO set from v1 txid to v2 txid
    UtxoReindex {
        /// Input UTXO DB path (v1)
        #[arg(long)]
        db: PathBuf,
        /// Input format
        #[arg(long, value_enum, default_value_t = DbFormat::JsonArray)]
        db_format: DbFormat,
        /// Optional UTXO entries file (JSONL) to reindex using tx map
        #[arg(long)]
        utxo: Option<PathBuf>,
        /// UTXO entries format
        #[arg(long, value_enum, default_value_t = UtxoFormat::Jsonl)]
        utxo_format: UtxoFormat,
        /// Output UTXO DB path (v2)
        #[arg(long)]
        out: PathBuf,
        /// Output format
        #[arg(long, value_enum, default_value_t = OutFormat::JsonArray)]
        out_format: OutFormat,
        /// Optional report path (JSON)
        #[arg(long)]
        report: Option<PathBuf>,
        /// Optional checkpoint path (JSON)
        #[arg(long)]
        checkpoint: Option<PathBuf>,
        /// Verify after reindex
        #[arg(long)]
        verify: bool,
        /// Verify by reading back written entries (sled only)
        #[arg(long = "verify-read")]
        verify_read: bool,
        /// Resume from checkpoint
        #[arg(long)]
        resume: bool,
        /// Dry run (no writes)
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
    /// Mine a block template locally (PoW)
    Mine {
        /// Previous block hash (hex, 32 bytes)
        #[arg(long)]
        prev_hash: String,
        /// Compact bits
        #[arg(long)]
        bits: u32,
        /// Block time (unix seconds). If omitted, uses now.
        #[arg(long)]
        time: Option<u32>,
        /// Coinbase reward value
        #[arg(long)]
        reward: u64,
        /// Coinbase script_pubkey hex
        #[arg(long)]
        coinbase_script: String,
        /// Max block size in bytes
        #[arg(long, default_value_t = 1_000_000)]
        max_block_bytes: usize,
        /// Optional UTXO JSONL for mempool validation
        #[arg(long)]
        utxo: Option<PathBuf>,
        /// Optional tx JSONL (Transaction JSON per line)
        #[arg(long)]
        txs: Option<PathBuf>,
        /// Max nonce iterations
        #[arg(long, default_value_t = 5_000_000)]
        max_nonce: u32,
        /// Output block JSON path
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Validate and apply a block to the UTXO set (local)
    SubmitBlock {
        /// Block JSON path
        #[arg(long)]
        block: PathBuf,
        /// UTXO JSONL input
        #[arg(long)]
        utxo: PathBuf,
        /// Output UTXO JSONL path
        #[arg(long)]
        out: PathBuf,
        /// Optional block reward (enforce coinbase <= reward + fees)
        #[arg(long)]
        reward: Option<u64>,
        /// Skip PoW check
        #[arg(long)]
        no_pow_check: bool,
    },
    /// Run a basic P2P node
    P2p {
        /// Listen address (host:port)
        #[arg(long, default_value = "0.0.0.0:8333")]
        listen: String,
        /// Peer addresses to connect (repeatable)
        #[arg(long)]
        peer: Vec<String>,
        /// Optional seed file (one address per line)
        #[arg(long)]
        seed_file: Option<PathBuf>,
        /// Optional UTXO JSONL for tx/block validation
        #[arg(long)]
        utxo: Option<PathBuf>,
        /// Skip PoW check for incoming blocks
        #[arg(long)]
        no_pow_check: bool,
        /// Network id (mainnet/testnet/devnet)
        #[arg(long, default_value = "mainnet")]
        network: String,
        /// Optional data directory for on-disk persistence
        #[arg(long)]
        data_dir: Option<PathBuf>,
        /// Emit periodic node stats (seconds, 0=disabled)
        #[arg(long, default_value_t = 0)]
        stats_interval: u64,
        /// Log level
        #[arg(long, value_enum, default_value_t = LogLevel::Info)]
        log_level: LogLevel,
        /// Optional log file path
        #[arg(long)]
        log_file: Option<PathBuf>,
        /// txid version for P2P messages (v1/v2)
        #[arg(long, value_enum, default_value_t = TxidVersion::V2)]
        txid_version: TxidVersion,
    },
    /// Migrate on-disk sled schema
    DbMigrate {
        /// Data directory containing chain.sled
        #[arg(long)]
        data_dir: PathBuf,
        /// Target schema version
        #[arg(long, default_value_t = 1)]
        target: u32,
        /// Allow no-op if already at target
        #[arg(long)]
        allow_noop: bool,
        /// Backup DB directory before migration
        #[arg(long)]
        backup: bool,
        /// Dry run (validate only, no changes)
        #[arg(long)]
        dry_run: bool,
        /// Output dry-run summary as JSON
        #[arg(long)]
        json: bool,
    },
    /// Backup on-disk sled DB
    DbBackup {
        /// Data directory containing chain.sled
        #[arg(long)]
        data_dir: PathBuf,
        /// Backup output directory
        #[arg(long)]
        out_dir: PathBuf,
        /// Overwrite existing backup
        #[arg(long)]
        force: bool,
    },
    /// Restore on-disk sled DB from backup
    DbRestore {
        /// Backup directory containing chain.sled
        #[arg(long)]
        backup_dir: PathBuf,
        /// Data directory to restore into
        #[arg(long)]
        data_dir: PathBuf,
        /// Overwrite existing DB
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TxidVersion {
    V1,
    V2,
}

impl TxidVersion {
    fn as_u8(self) -> u8 {
        match self {
            TxidVersion::V1 => 1,
            TxidVersion::V2 => 2,
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum ReindexError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("UTXO error: {0}")]
    Utxo(#[from] UtxoError),
    #[error("UTXO DB error: {0}")]
    UtxoDb(#[from] UtxoDbError),
    #[error("sled error: {0}")]
    Sled(#[from] sled::Error),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("mining error: {0}")]
    Mining(String),
}


fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), ReindexError> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::UtxoReindex {
            db,
            db_format,
            utxo,
            utxo_format,
            out,
            out_format,
            report,
            checkpoint,
            verify,
            verify_read,
            resume,
            dry_run,
        }) => utxo_reindex(
            db,
            db_format,
            utxo,
            utxo_format,
            out,
            out_format,
            report,
            checkpoint,
            verify,
            verify_read,
            resume,
            dry_run,
        ),
        Some(Command::Mine {
            prev_hash,
            bits,
            time,
            reward,
            coinbase_script,
            max_block_bytes,
            utxo,
            txs,
            max_nonce,
            out,
        }) => mine_block(
            prev_hash,
            bits,
            time,
            reward,
            coinbase_script,
            max_block_bytes,
            utxo,
            txs,
            max_nonce,
            out,
        ),
        Some(Command::SubmitBlock {
            block,
            utxo,
            out,
            reward,
            no_pow_check,
        }) => submit_block(block, utxo, out, reward, no_pow_check),
        Some(Command::P2p {
            listen,
            peer,
            seed_file,
            utxo,
            no_pow_check,
            network,
            data_dir,
            stats_interval,
            log_level,
            log_file,
            txid_version,
        }) => {
            let mut peers = peer;
            if let Some(path) = seed_file {
                peers.extend(load_seed_file(&path)?);
            }
            p2p::run_p2p(
            listen,
            peers,
            utxo,
            no_pow_check,
            network,
            data_dir,
            stats_interval,
            log_level,
            log_file,
            txid_version.as_u8(),
        )
            .map_err(|e| ReindexError::Mining(e.to_string()))
        }
        Some(Command::DbMigrate {
            data_dir,
            target,
            allow_noop,
            backup,
            dry_run,
            json,
        }) => db_migrate(data_dir, target, allow_noop, backup, dry_run, json),
        Some(Command::DbBackup {
            data_dir,
            out_dir,
            force,
        }) => db_backup(data_dir, out_dir, force),
        Some(Command::DbRestore {
            backup_dir,
            data_dir,
            force,
        }) => db_restore(backup_dir, data_dir, force),
        None => {
            println!("Starting tenebriumd...");
            Ok(())
        }
    }
}

fn db_migrate(
    data_dir: PathBuf,
    target: u32,
    allow_noop: bool,
    backup: bool,
    dry_run: bool,
    json: bool,
) -> Result<(), ReindexError> {
    if json && !dry_run {
        return Err(ReindexError::InvalidArgs(
            "--json requires --dry-run".to_string(),
        ));
    }
    if dry_run && backup {
        return Err(ReindexError::InvalidArgs(
            "--dry-run cannot be combined with --backup".to_string(),
        ));
    }
    let db_path = data_dir.join("chain.sled");
    if backup {
        let backup_path = data_dir.join("chain.sled.bak");
        if backup_path.exists() {
            return Err(ReindexError::InvalidArgs(
                "backup already exists".to_string(),
            ));
        }
        std::fs::create_dir_all(&backup_path).map_err(ReindexError::from)?;
        copy_dir_recursive(&db_path, &backup_path)?;
    }
    let db = sled::open(db_path).map_err(ReindexError::from)?;
    let meta = db.open_tree("meta").map_err(ReindexError::from)?;
    let current = meta
        .get("schema_version")
        .map_err(ReindexError::from)?
        .and_then(|v| {
            if v.len() == 4 {
                Some(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
            } else {
                None
            }
        })
        .unwrap_or(0);

    if current == target {
        if allow_noop {
            if dry_run {
                validate_schema(&db)?;
                print_dry_run_summary(&db, json)?;
                println!("dry-run: schema version {current} valid");
                return Ok(());
            }
            println!("schema version already {current}");
            return Ok(());
        }
        return Err(ReindexError::InvalidArgs(
            "schema version already at target".to_string(),
        ));
    }
    if current == 0 && target == 1 {
        if dry_run {
            validate_schema(&db)?;
            print_dry_run_summary(&db, json)?;
            println!("dry-run: can migrate schema 0 -> 1");
            return Ok(());
        }
        meta.insert("schema_version", target.to_le_bytes().to_vec())
            .map_err(ReindexError::from)?;
        meta.flush().map_err(ReindexError::from)?;
        println!("migrated schema 0 -> 1");
        return Ok(());
    }

    if current == 1 && target == 2 {
        if dry_run {
            validate_schema(&db)?;
            print_dry_run_summary(&db, json)?;
            println!("dry-run: can migrate schema 1 -> 2");
            return Ok(());
        }
        meta.insert("schema_version", target.to_le_bytes().to_vec())
            .map_err(ReindexError::from)?;
        if meta.get("network_id").map_err(ReindexError::from)?.is_none() {
            meta.insert("network_id", b"mainnet".to_vec())
                .map_err(ReindexError::from)?;
        }
        meta.flush().map_err(ReindexError::from)?;
        validate_schema(&db)?;
        println!("migrated schema 1 -> 2");
        return Ok(());
    }

    Err(ReindexError::InvalidArgs(
        "unsupported migration path".to_string(),
    ))
}

fn db_backup(data_dir: PathBuf, out_dir: PathBuf, force: bool) -> Result<(), ReindexError> {
    let src = data_dir.join("chain.sled");
    let dst = out_dir.join("chain.sled");
    if dst.exists() {
        if !force {
            return Err(ReindexError::InvalidArgs(
                "backup already exists".to_string(),
            ));
        }
        fs::remove_dir_all(&dst).map_err(ReindexError::from)?;
    }
    fs::create_dir_all(&dst).map_err(ReindexError::from)?;
    copy_dir_recursive(&src, &dst)?;
    println!("backup created at {}", dst.display());
    Ok(())
}

fn db_restore(backup_dir: PathBuf, data_dir: PathBuf, force: bool) -> Result<(), ReindexError> {
    let src = backup_dir.join("chain.sled");
    let dst = data_dir.join("chain.sled");
    if dst.exists() {
        if !force {
            return Err(ReindexError::InvalidArgs(
                "destination already exists".to_string(),
            ));
        }
        fs::remove_dir_all(&dst).map_err(ReindexError::from)?;
    }
    fs::create_dir_all(&dst).map_err(ReindexError::from)?;
    copy_dir_recursive(&src, &dst)?;
    println!("restore completed into {}", dst.display());
    Ok(())
}

fn validate_schema(db: &sled::Db) -> Result<(), ReindexError> {
    let meta = db.open_tree("meta").map_err(ReindexError::from)?;
    let schema = meta
        .get("schema_version")
        .map_err(ReindexError::from)?
        .ok_or_else(|| ReindexError::InvalidArgs("missing schema_version".to_string()))?;
    if schema.len() != 4 {
        return Err(ReindexError::InvalidArgs(
            format!("invalid schema_version length: {}", schema.len()),
        ));
    }
    let ver = u32::from_le_bytes([schema[0], schema[1], schema[2], schema[3]]);
    if ver != 2 {
        return Err(ReindexError::InvalidArgs(
            format!("schema_version mismatch: expected 2, got {ver}"),
        ));
    }
    let network_id = meta.get("network_id").map_err(ReindexError::from)?;
    if let Some(bytes) = network_id {
        if bytes.is_empty() {
            return Err(ReindexError::InvalidArgs(
                "invalid network_id: empty".to_string(),
            ));
        }
    } else {
        return Err(ReindexError::InvalidArgs("missing network_id".to_string()));
    }
    for tree in ["headers", "heights", "work", "utxo", "blocks"] {
        db.open_tree(tree)
            .map_err(|e| ReindexError::InvalidArgs(format!("missing tree {tree}: {e}")))?;
    }
    check_utxo_block_sample(db)?;
    Ok(())
}

fn check_utxo_block_sample(db: &sled::Db) -> Result<(), ReindexError> {
    const SAMPLE_LIMIT: usize = 3;
    let utxo = db.open_tree("utxo").map_err(ReindexError::from)?;
    let blocks = db.open_tree("blocks").map_err(ReindexError::from)?;

    let mut sample_txids: HashSet<[u8; 32]> = HashSet::new();
    for entry in utxo.iter() {
        let (key, _) = entry.map_err(ReindexError::from)?;
        let outpoint = utxo_db::decode_outpoint(&key).map_err(ReindexError::from)?;
        sample_txids.insert(outpoint.txid);
        if sample_txids.len() >= SAMPLE_LIMIT {
            break;
        }
    }
    if sample_txids.is_empty() {
        return Ok(());
    }

    if blocks.is_empty() {
        return Err(ReindexError::InvalidArgs(
            "utxo entries exist but blocks tree is empty".to_string(),
        ));
    }

    let mut remaining = sample_txids.clone();
    for entry in blocks.iter() {
        let (_, value) = entry.map_err(ReindexError::from)?;
        let block: tenebrium_consensus::Block = serde_json::from_slice(&value)?;
        for tx in block.txs.iter() {
            let txid = tx.txid_v2()?;
            remaining.remove(&txid);
            if remaining.is_empty() {
                break;
            }
        }
        if remaining.is_empty() {
            break;
        }
    }

    if !remaining.is_empty() {
        let missing = remaining
            .iter()
            .next()
            .map(|txid| hex::encode(txid))
            .unwrap_or_else(|| "<unknown>".to_string());
        return Err(ReindexError::InvalidArgs(format!(
            "sample utxo txid not found in blocks: {missing}"
        )));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct DryRunSummary {
    schema_version: u32,
    network_id: String,
    meta_keys: u64,
    headers: u64,
    heights: u64,
    work: u64,
    utxo: u64,
    blocks: u64,
    utxo_count: Option<u64>,
}

fn print_dry_run_summary(db: &sled::Db, json: bool) -> Result<(), ReindexError> {
    let meta = db.open_tree("meta").map_err(ReindexError::from)?;
    let schema = meta
        .get("schema_version")
        .map_err(ReindexError::from)?
        .and_then(|v| {
            if v.len() == 4 {
                Some(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
            } else {
                None
            }
        })
        .unwrap_or(0);
    let network_id = meta
        .get("network_id")
        .map_err(ReindexError::from)?
        .map(|v| String::from_utf8_lossy(&v).to_string())
        .unwrap_or_else(|| "<missing>".to_string());
    let utxo_count = meta
        .get("utxo_count")
        .map_err(ReindexError::from)?
        .and_then(|v| {
            if v.len() == 8 {
                Some(u64::from_le_bytes([
                    v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7],
                ]))
            } else {
                None
            }
        });

    let headers = db.open_tree("headers").map_err(ReindexError::from)?;
    let heights = db.open_tree("heights").map_err(ReindexError::from)?;
    let work = db.open_tree("work").map_err(ReindexError::from)?;
    let utxo = db.open_tree("utxo").map_err(ReindexError::from)?;
    let blocks = db.open_tree("blocks").map_err(ReindexError::from)?;

    let summary = DryRunSummary {
        schema_version: schema,
        network_id,
        meta_keys: meta.len() as u64,
        headers: headers.len() as u64,
        heights: heights.len() as u64,
        work: work.len() as u64,
        utxo: utxo.len() as u64,
        blocks: blocks.len() as u64,
        utxo_count,
    };

    if json {
        let payload = serde_json::to_string(&summary)?;
        println!("{payload}");
        return Ok(());
    }

    println!("dry-run summary:");
    println!("  schema_version: {}", summary.schema_version);
    println!("  network_id: {}", summary.network_id);
    println!("  meta_keys: {}", summary.meta_keys);
    println!("  headers: {}", summary.headers);
    println!("  heights: {}", summary.heights);
    println!("  work: {}", summary.work);
    println!("  utxo: {}", summary.utxo);
    println!("  blocks: {}", summary.blocks);
    if let Some(count) = summary.utxo_count {
        println!("  utxo_count: {count}");
    }
    Ok(())
}

fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<(), ReindexError> {
    if !src.exists() {
        return Err(ReindexError::InvalidArgs("source does not exist".to_string()));
    }
    for entry in std::fs::read_dir(src).map_err(ReindexError::from)? {
        let entry = entry.map_err(ReindexError::from)?;
        let file_type = entry.file_type().map_err(ReindexError::from)?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            std::fs::create_dir_all(&dst_path).map_err(ReindexError::from)?;
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&src_path, &dst_path).map_err(ReindexError::from)?;
        }
    }
    Ok(())
}

fn mine_block(
    prev_hash: String,
    bits: u32,
    time: Option<u32>,
    reward: u64,
    coinbase_script: String,
    max_block_bytes: usize,
    utxo: Option<PathBuf>,
    txs: Option<PathBuf>,
    max_nonce: u32,
    out: Option<PathBuf>,
) -> Result<(), ReindexError> {
    if txs.is_some() && utxo.is_none() {
        return Err(ReindexError::InvalidArgs(
            "--txs requires --utxo for validation".to_string(),
        ));
    }

    let prev_hash = decode_hex_32(&prev_hash)?;
    let coinbase_script = hex::decode(coinbase_script)
        .map_err(|e| ReindexError::InvalidArgs(format!("invalid coinbase script: {e}")))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| ReindexError::Mining(e.to_string()))?
        .as_secs() as u32;
    let time = time.unwrap_or(now);

    let mut mempool = Mempool::new(MempoolConfig::default());
    if let (Some(utxo_path), Some(tx_path)) = (utxo, txs) {
        let mut set = InMemoryUtxoSet::new();
        let reader = jsonl_reader(&utxo_path);
        reader.for_each(|entry| {
            set.insert(entry.outpoint, entry.txout);
            Ok(())
        })?;
        let file = fs::File::open(&tx_path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let tx: Transaction = serde_json::from_str(line.trim())?;
            mempool.add_tx(tx, &set).map_err(|e| ReindexError::Mining(e.to_string()))?;
        }
    }

    let coinbase = Transaction {
        version: 1,
        vin: vec![],
        vout: vec![tenebrium_utxo::TxOut {
            value: reward,
            script_pubkey: coinbase_script,
        }],
        lock_time: 0,
    };

    let mut template = build_block_template(
        &mempool,
        coinbase,
        prev_hash,
        time,
        bits,
        1,
        max_block_bytes,
    )
    .map_err(|e| ReindexError::Mining(e.to_string()))?;

    let header = &mut template.block.header;
    let found = mine_header(header, max_nonce).map_err(|e| ReindexError::Mining(e.to_string()))?;
    if found.is_none() {
        return Err(ReindexError::Mining("nonce not found".to_string()));
    }

    let json = serde_json::to_string_pretty(&template.block)?;
    match out {
        Some(path) => fs::write(path, json)?,
        None => println!("{json}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::db_migrate;
    use super::{db_backup, db_restore};
    use super::submit_block;
    use super::ReindexError;
    use crate::p2p;
    use tempfile::tempdir;
    use tenebrium_consensus::Block;
    use tenebrium_utxo::{OutPoint, Transaction, TxIn, TxOut};
    use std::fs;

    #[test]
    fn db_migrate_dry_run_rejects_backup() {
        let temp = tempdir().unwrap();
        let result = db_migrate(
            temp.path().to_path_buf(),
            2,
            true,
            true,
            true,
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn db_migrate_dry_run_ok_on_valid_schema() {
        let temp = tempdir().unwrap();
        {
            let db_path = temp.path().join("chain.sled");
            let db = sled::open(db_path).unwrap();
            let meta = db.open_tree("meta").unwrap();
            meta.insert("schema_version", 2u32.to_le_bytes().to_vec())
                .unwrap();
            meta.insert("network_id", b"mainnet".to_vec()).unwrap();
            db.open_tree("headers").unwrap();
            db.open_tree("heights").unwrap();
            db.open_tree("work").unwrap();
            db.open_tree("utxo").unwrap();
            db.open_tree("blocks").unwrap();
            db.flush().unwrap();
        }

        let result = db_migrate(
            temp.path().to_path_buf(),
            2,
            true,
            false,
            true,
            false,
        );
        if let Err(err) = result {
            panic!("db_migrate dry-run failed: {err}");
        }
    }

    #[test]
    fn p2p_schema_allows_db_migrate_dry_run() {
        let temp = tempdir().unwrap();
        let db = p2p::open_sled(&temp.path().to_path_buf()).unwrap();
        drop(db);

        let result = db_migrate(
            temp.path().to_path_buf(),
            2,
            true,
            false,
            true,
            true,
        );
        if let Err(err) = result {
            panic!("p2p schema dry-run failed: {err}");
        }
    }

    fn write_empty_utxo(path: &std::path::Path) {
        fs::write(path, "").unwrap();
    }

    fn write_block(path: &std::path::Path, block: &Block) {
        let json = serde_json::to_string_pretty(block).unwrap();
        fs::write(path, json).unwrap();
    }

    #[test]
    fn db_backup_and_restore_roundtrip() {
        let temp = tempdir().unwrap();
        let data_dir = temp.path().join("data");
        let backup_dir = temp.path().join("backup");
        let restore_dir = temp.path().join("restore");
        fs::create_dir_all(data_dir.join("chain.sled")).unwrap();
        fs::write(data_dir.join("chain.sled").join("dummy"), b"ok").unwrap();

        db_backup(data_dir.clone(), backup_dir.clone(), false).unwrap();
        assert!(backup_dir.join("chain.sled").join("dummy").exists());

        db_restore(backup_dir, restore_dir.clone(), false).unwrap();
        assert!(restore_dir.join("chain.sled").join("dummy").exists());
    }

    #[test]
    fn submit_block_rejects_merkle_mismatch() {
        let temp = tempdir().unwrap();
        let utxo_path = temp.path().join("utxo.jsonl");
        let block_path = temp.path().join("block.json");
        let out_path = temp.path().join("out.jsonl");
        write_empty_utxo(&utxo_path);

        let coinbase = Transaction {
            version: 1,
            vin: vec![],
            vout: vec![TxOut {
                value: 50,
                script_pubkey: vec![1],
            }],
            lock_time: 0,
        };
        let mut block = Block::new(
            1,
            [0u8; 32],
            0,
            0x207fffff,
            0,
            vec![coinbase],
        )
        .unwrap();
        block.header.merkle_root = [0u8; 32];
        write_block(&block_path, &block);

        let result = submit_block(block_path, utxo_path, out_path, None, true);
        match result {
            Err(ReindexError::Mining(msg)) => assert!(msg.contains("merkle root mismatch")),
            _ => panic!("expected merkle root mismatch"),
        }
    }

    #[test]
    fn submit_block_rejects_coinbase_with_inputs() {
        let temp = tempdir().unwrap();
        let utxo_path = temp.path().join("utxo.jsonl");
        let block_path = temp.path().join("block.json");
        let out_path = temp.path().join("out.jsonl");
        write_empty_utxo(&utxo_path);

        let coinbase = Transaction {
            version: 1,
            vin: vec![TxIn {
                prevout: OutPoint {
                    txid: [1u8; 32],
                    vout: 0,
                },
                script_sig: vec![],
                sequence: 0,
            }],
            vout: vec![TxOut {
                value: 50,
                script_pubkey: vec![1],
            }],
            lock_time: 0,
        };
        let block = Block::new(
            1,
            [0u8; 32],
            0,
            0x207fffff,
            0,
            vec![coinbase],
        )
        .unwrap();
        write_block(&block_path, &block);

        let result = submit_block(block_path, utxo_path, out_path, None, true);
        match result {
            Err(ReindexError::Mining(msg)) => assert!(msg.contains("coinbase must have no inputs")),
            _ => panic!("expected coinbase input rejection"),
        }
    }

    #[test]
    fn submit_block_rejects_excess_coinbase_reward() {
        let temp = tempdir().unwrap();
        let utxo_path = temp.path().join("utxo.jsonl");
        let block_path = temp.path().join("block.json");
        let out_path = temp.path().join("out.jsonl");
        write_empty_utxo(&utxo_path);

        let coinbase = Transaction {
            version: 1,
            vin: vec![],
            vout: vec![TxOut {
                value: 100,
                script_pubkey: vec![1],
            }],
            lock_time: 0,
        };
        let block = Block::new(
            1,
            [0u8; 32],
            0,
            0x207fffff,
            0,
            vec![coinbase],
        )
        .unwrap();
        write_block(&block_path, &block);

        let result = submit_block(block_path, utxo_path, out_path, Some(50), true);
        match result {
            Err(ReindexError::Mining(msg)) => assert!(msg.contains("coinbase exceeds reward+fees")),
            _ => panic!("expected coinbase reward rejection"),
        }
    }
}

fn decode_hex_32(hex_str: &str) -> Result<[u8; 32], ReindexError> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| ReindexError::InvalidArgs(format!("invalid hex: {e}")))?;
    if bytes.len() != 32 {
        return Err(ReindexError::InvalidArgs(format!(
            "expected 32-byte hex, got {} bytes",
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn load_seed_file(path: &Path) -> Result<Vec<String>, ReindexError> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut peers = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        peers.push(trimmed.to_string());
    }
    Ok(peers)
}

fn submit_block(
    block_path: PathBuf,
    utxo_path: PathBuf,
    out_path: PathBuf,
    reward: Option<u64>,
    no_pow_check: bool,
) -> Result<(), ReindexError> {
    let block_json = fs::read_to_string(block_path)?;
    let block: tenebrium_consensus::Block = serde_json::from_str(&block_json)?;

    if !no_pow_check {
        let ok = check_pow(&block.header).map_err(|e| ReindexError::Mining(e.to_string()))?;
        if !ok {
            return Err(ReindexError::Mining("invalid PoW".to_string()));
        }
    }

    let txids = block
        .txs
        .iter()
        .map(|tx| tx.txid_v2())
        .collect::<Result<Vec<_>, _>>()
        .map_err(ReindexError::Utxo)?;
    let root = merkle_root(&txids);
    if root != block.header.merkle_root {
        return Err(ReindexError::Mining("merkle root mismatch".to_string()));
    }

    let mut utxos = InMemoryUtxoSet::new();
    let reader = jsonl_reader(&utxo_path);
    reader.for_each(|entry| {
        utxos.insert(entry.outpoint, entry.txout);
        Ok(())
    })?;

    if block.txs.is_empty() {
        return Err(ReindexError::Mining("empty block".to_string()));
    }

    let mut total_fees = 0u64;
    for (i, tx) in block.txs.iter().enumerate() {
        if i == 0 {
            if !tx.vin.is_empty() {
                return Err(ReindexError::Mining("coinbase must have no inputs".to_string()));
            }
            apply_coinbase(tx, &mut utxos)?;
        } else {
            let fee = Transaction::validate_value_conservation(tx, &utxos)?;
            total_fees = total_fees.saturating_add(fee);
            utxos.apply_tx(tx)?;
        }
    }

    if let Some(reward) = reward {
        let coinbase = &block.txs[0];
        let out_sum = Transaction::sum_outputs(coinbase)?;
        if out_sum > reward.saturating_add(total_fees) {
            return Err(ReindexError::Mining("coinbase exceeds reward+fees".to_string()));
        }
    }

    write_utxo_jsonl(&utxos, out_path)?;
    Ok(())
}

fn apply_coinbase(tx: &Transaction, utxos: &mut InMemoryUtxoSet) -> Result<(), ReindexError> {
    tx.validate()?;
    let outpoints = Transaction::make_outpoints(tx)?;
    for (op, txout) in outpoints.into_iter().zip(tx.vout.iter()) {
        if utxos.get(&op).is_some() {
            return Err(ReindexError::Mining("coinbase output already exists".to_string()));
        }
        utxos.insert(op, txout.clone());
    }
    Ok(())
}

fn write_utxo_jsonl(utxos: &InMemoryUtxoSet, out_path: PathBuf) -> Result<(), ReindexError> {
    let file = fs::File::create(out_path)?;
    let mut writer = BufWriter::new(file);
    for (outpoint, txout) in utxos.entries() {
        let entry = UtxoEntry { outpoint, txout };
        serde_json::to_writer(&mut writer, &entry)?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

fn utxo_reindex(
    db: PathBuf,
    db_format: DbFormat,
    utxo: Option<PathBuf>,
    utxo_format: UtxoFormat,
    out: PathBuf,
    out_format: OutFormat,
    report: Option<PathBuf>,
    checkpoint: Option<PathBuf>,
    verify: bool,
    verify_read: bool,
    resume: bool,
    dry_run: bool,
) -> Result<(), ReindexError> {
    println!("utxo-reindex (poc)");
    println!("  db: {}", db.display());
    println!("  db_format: {db_format:?}");
    if let Some(ref utxo) = utxo {
        println!("  utxo: {}", utxo.display());
        println!("  utxo_format: {utxo_format:?}");
    }
    println!("  out: {}", out.display());
    println!("  out_format: {out_format:?}");
    if let Some(ref report) = report {
        println!("  report: {}", report.display());
    }
    if let Some(ref checkpoint) = checkpoint {
        println!("  checkpoint: {}", checkpoint.display());
    }
    println!("  verify: {verify}");
    println!("  verify_read: {verify_read}");
    println!("  resume: {resume}");
    println!("  dry_run: {dry_run}");

    let started_at = now_unix_seconds();
    let mut report_obj = ReindexReport::new(started_at);

    if utxo.is_some() && resume {
        return Err(ReindexError::InvalidArgs(
            "--resume is not supported with --utxo yet".to_string(),
        ));
    }

    if utxo.is_some() && matches!(out_format, OutFormat::JsonArray | OutFormat::Jsonl) {
        return Err(ReindexError::InvalidArgs(
            "use --out-format utxo-jsonl with --utxo".to_string(),
        ));
    }
    if utxo.is_none() && matches!(out_format, OutFormat::Sled) {
        return Err(ReindexError::InvalidArgs(
            "--out-format sled requires --utxo".to_string(),
        ));
    }
    if verify_read && !matches!(out_format, OutFormat::Sled) {
        return Err(ReindexError::InvalidArgs(
            "--verify-read requires --out-format sled".to_string(),
        ));
    }

    let checkpoint_path = if utxo.is_some() {
        None
    } else {
        resolve_checkpoint_path(&out, checkpoint, resume)?
    };
    let (mut mappings, start_index) = if let Some(ref path) = checkpoint_path {
        if resume {
            let cp = load_checkpoint(path)?;
            report_obj = cp.report;
            (cp.mappings, cp.next_tx_index)
        } else {
            (Vec::new(), 0usize)
        }
    } else {
        (Vec::new(), 0usize)
    };

    if let Some(utxo_path) = utxo {
        let txid_map = build_txid_map_from_db(&db, db_format, &mut report_obj)?;
        process_utxo_entries(
            &utxo_path,
            utxo_format,
            &out,
            out_format,
            &txid_map,
            verify,
            verify_read,
            dry_run,
            &mut report_obj,
        )?;
        report_obj.finish(now_unix_seconds());
        write_report(report_obj, report)?;
        return Ok(());
    }

    if checkpoint_path.is_none() && matches!(out_format, OutFormat::Jsonl) && !dry_run {
        let file = fs::File::create(&out)?;
        let mut writer = BufWriter::new(file);
        let mut seen: HashSet<OutPoint> = HashSet::new();
        let mut dupe_count = 0u64;

        stream_transactions(&db, db_format, |_, tx| {
            if let Err(err) = tx.validate() {
                let txid_v1 = tx.txid_v1().ok();
                report_obj.skipped += 1;
                report_obj.record_error(ReindexErrorEntry::new(
                    ReindexErrorKind::InvalidTx,
                    txid_v1,
                    err.to_string(),
                ));
                return Ok(());
            }

            report_obj.total_inputs += tx.vin.len() as u64;
            report_obj.total_outputs += tx.vout.len() as u64;

            let pairs = map_outpoints_v1_to_v2(&tx)?;
            for (v1, v2) in pairs {
                if verify && !seen.insert(v2.clone()) {
                    dupe_count += 1;
                }
                let entry = MappingEntry { v1, v2 };
                let line = serde_json::to_string(&entry)?;
                writeln!(writer, "{line}")?;
            }
            Ok(())
        })?;

        if verify && dupe_count > 0 {
            report_obj.record_error(ReindexErrorEntry::new(
                ReindexErrorKind::DuplicateOutPoint,
                None,
                format!("duplicate v2 outpoints: {dupe_count}"),
            ));
        }

        writer.flush()?;
    } else {
        let txs = load_transactions(&db, db_format)?;
        for (idx, tx) in txs.into_iter().enumerate() {
        if idx < start_index {
            continue;
        }
        if let Err(err) = tx.validate() {
            let txid_v1 = tx.txid_v1().ok();
            report_obj.skipped += 1;
            report_obj.record_error(ReindexErrorEntry::new(
                ReindexErrorKind::InvalidTx,
                txid_v1,
                err.to_string(),
            ));
            continue;
        }

        report_obj.total_inputs += tx.vin.len() as u64;
        report_obj.total_outputs += tx.vout.len() as u64;

        let pairs = map_outpoints_v1_to_v2(&tx)?;
        for (v1, v2) in pairs {
            mappings.push(MappingEntry { v1, v2 });
        }

        if let Some(ref path) = checkpoint_path {
            if idx % CHECKPOINT_INTERVAL == 0 {
                save_checkpoint(path, idx + 1, &mappings, &report_obj)?;
            }
        }
    }

    if verify {
        verify_no_duplicate_v2(&mappings, &mut report_obj)?;
    }

        if !dry_run {
            write_mappings(&out, out_format, &mappings)?;
        }
    }

    report_obj.finish(now_unix_seconds());
    if let Some(ref path) = checkpoint_path {
        save_checkpoint(path, mappings.len(), &mappings, &report_obj)?;
    }
    write_report(report_obj, report)?;
    Ok(())
}

const CHECKPOINT_INTERVAL: usize = 1000;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DbFormat {
    JsonArray,
    Jsonl,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutFormat {
    JsonArray,
    Jsonl,
    UtxoJsonl,
    Sled,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum UtxoFormat {
    Jsonl,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct MappingEntry {
    v1: OutPoint,
    v2: OutPoint,
}


#[derive(Debug, Serialize, Deserialize)]
struct Checkpoint {
    next_tx_index: usize,
    mappings: Vec<MappingEntry>,
    report: ReindexReport,
}

fn verify_no_duplicate_v2(
    mappings: &[MappingEntry],
    report: &mut ReindexReport,
) -> Result<(), ReindexError> {
    let mut seen: HashSet<OutPoint> = HashSet::new();
    let mut dupes = 0u64;
    for m in mappings {
        if !seen.insert(m.v2.clone()) {
            dupes += 1;
        }
    }
    if dupes > 0 {
        report.record_error(ReindexErrorEntry::new(
            ReindexErrorKind::DuplicateOutPoint,
            None,
            format!("duplicate v2 outpoints: {dupes}"),
        ));
    }
    Ok(())
}

fn write_report(report: ReindexReport, path: Option<PathBuf>) -> Result<(), ReindexError> {
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = path {
        fs::write(path, json)?;
    } else {
        println!("{json}");
    }
    Ok(())
}

fn load_transactions(path: &Path, format: DbFormat) -> Result<Vec<Transaction>, ReindexError> {
    match format {
        DbFormat::JsonArray => {
            let mut raw = fs::read_to_string(path)?;
            if raw.starts_with('\u{feff}') {
                raw = raw.trim_start_matches('\u{feff}').to_string();
            }
            let txs: Vec<Transaction> = serde_json::from_str(&raw)?;
            Ok(txs)
        }
        DbFormat::Jsonl => {
            let file = fs::File::open(path)?;
            let reader = BufReader::new(file);
            let mut txs = Vec::new();
            for line in reader.lines() {
                let line = line?;
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let tx: Transaction = serde_json::from_str(trimmed)?;
                txs.push(tx);
            }
            Ok(txs)
        }
    }
}

fn write_mappings(
    path: &Path,
    format: OutFormat,
    mappings: &[MappingEntry],
) -> Result<(), ReindexError> {
    match format {
        OutFormat::JsonArray => {
            let out_json = serde_json::to_string_pretty(mappings)?;
            fs::write(path, out_json)?;
        }
        OutFormat::Jsonl => {
            let file = fs::File::create(path)?;
            let mut writer = BufWriter::new(file);
            for m in mappings {
                let line = serde_json::to_string(m)?;
                writeln!(writer, "{line}")?;
            }
            writer.flush()?;
        }
        OutFormat::UtxoJsonl | OutFormat::Sled => {
            return Err(ReindexError::InvalidArgs(
                "use --utxo with --out-format utxo-jsonl or sled".to_string(),
            ));
        }
    }
    Ok(())
}

fn build_txid_map_from_db(
    path: &Path,
    format: DbFormat,
    report: &mut ReindexReport,
) -> Result<HashMap<[u8; 32], [u8; 32]>, ReindexError> {
    let mut map: HashMap<[u8; 32], [u8; 32]> = HashMap::new();
    stream_transactions(path, format, |_, tx| {
        let v1 = tx.txid_v1()?;
        let v2 = tx.txid_v2()?;
        if map.insert(v1, v2).is_some() {
            report.record_error(ReindexErrorEntry::new(
                ReindexErrorKind::Other,
                Some(v1),
                "duplicate txid_v1 in tx list".to_string(),
            ));
        }
        Ok(())
    })?;
    Ok(map)
}

fn stream_transactions<F>(
    path: &Path,
    format: DbFormat,
    mut f: F,
) -> Result<(), ReindexError>
where
    F: FnMut(usize, Transaction) -> Result<(), ReindexError>,
{
    match format {
        DbFormat::JsonArray => {
            let file = fs::File::open(path)?;
            let reader = BufReader::new(file);
            let mut de = serde_json::Deserializer::from_reader(reader);
            let visitor = TxArrayVisitor { f: &mut f };
            de.deserialize_seq(visitor).map_err(ReindexError::Json)?;
            Ok(())
        }
        DbFormat::Jsonl => {
            let file = fs::File::open(path)?;
            let reader = BufReader::new(file);
            for (idx, line) in reader.lines().enumerate() {
                let line = line?;
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let tx: Transaction = serde_json::from_str(trimmed)?;
                f(idx, tx)?;
            }
            Ok(())
        }
    }
}

struct TxArrayVisitor<'a, F> {
    f: &'a mut F,
}

impl<'de, 'a, F> Visitor<'de> for TxArrayVisitor<'a, F>
where
    F: FnMut(usize, Transaction) -> Result<(), ReindexError>,
{
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON array of transactions")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut idx = 0usize;
        while let Some(tx) = seq.next_element::<Transaction>()? {
            (self.f)(idx, tx).map_err(serde::de::Error::custom)?;
            idx += 1;
        }
        Ok(())
    }
}


fn process_utxo_entries(
    utxo_path: &Path,
    utxo_format: UtxoFormat,
    out_path: &Path,
    out_format: OutFormat,
    txid_map: &HashMap<[u8; 32], [u8; 32]>,
    verify: bool,
    verify_read: bool,
    dry_run: bool,
    report: &mut ReindexReport,
) -> Result<(), ReindexError> {
    let mut writer = if dry_run {
        None
    } else if matches!(out_format, OutFormat::UtxoJsonl) {
        let out_file = fs::File::create(out_path)?;
        Some(BufWriter::new(out_file))
    } else {
        None
    };

    let mut sled_store = if dry_run {
        None
    } else if matches!(out_format, OutFormat::Sled) {
        Some(KvUtxoStore::open(out_path.to_path_buf())?)
    } else {
        None
    };

    let mut seen: HashSet<OutPoint> = HashSet::new();
    let mut dupe_count = 0u64;

    match utxo_format {
        UtxoFormat::Jsonl => {
            let reader = jsonl_reader(utxo_path);
            reader.for_each(|entry| {
                report.total_outputs += 1;
                let v2_txid = match txid_map.get(&entry.outpoint.txid) {
                    Some(v2) => *v2,
                    None => {
                        report.skipped += 1;
                        report.record_error(ReindexErrorEntry::new(
                            ReindexErrorKind::MissingTx,
                            Some(entry.outpoint.txid),
                            "missing txid_v1 in tx list".to_string(),
                        ));
                        return Ok(());
                    }
                };

                let v2_outpoint = OutPoint {
                    txid: v2_txid,
                    vout: entry.outpoint.vout,
                };
                if verify && !seen.insert(v2_outpoint.clone()) {
                    dupe_count += 1;
                }

                if let Some(ref mut w) = writer {
                    let out_entry = UtxoEntry {
                        outpoint: v2_outpoint.clone(),
                        txout: entry.txout.clone(),
                    };
                    let line = serde_json::to_string(&out_entry)?;
                    writeln!(w, "{line}")?;
                }
                if let Some(ref mut store) = sled_store {
                    store.put(&v2_outpoint, &entry.txout)?;
                    if verify_read {
                        match store.get(&v2_outpoint)? {
                            Some(read_txout) => {
                                if read_txout != entry.txout {
                                    report.record_error(ReindexErrorEntry::new(
                                        ReindexErrorKind::Other,
                                        Some(v2_outpoint.txid),
                                        "sled read-back mismatch".to_string(),
                                    ));
                                }
                            }
                            None => {
                                report.record_error(ReindexErrorEntry::new(
                                    ReindexErrorKind::Other,
                                    Some(v2_outpoint.txid),
                                    "sled read-back missing".to_string(),
                                ));
                            }
                        }
                    }
                }
                Ok(())
            })?;
        }
    }

    if verify && dupe_count > 0 {
        report.record_error(ReindexErrorEntry::new(
            ReindexErrorKind::DuplicateOutPoint,
            None,
            format!("duplicate v2 outpoints: {dupe_count}"),
        ));
    }

    if let Some(ref mut w) = writer {
        w.flush()?;
    }
    Ok(())
}

fn resolve_checkpoint_path(
    out: &Path,
    checkpoint: Option<PathBuf>,
    resume: bool,
) -> Result<Option<PathBuf>, ReindexError> {
    if checkpoint.is_some() {
        return Ok(checkpoint);
    }
    if resume {
        return Ok(Some(default_checkpoint_path(out)));
    }
    Ok(None)
}

fn default_checkpoint_path(out: &Path) -> PathBuf {
    let mut p = out.to_path_buf();
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext.is_empty() {
        p.set_extension("checkpoint.json");
    } else {
        p.set_extension(format!("{ext}.checkpoint.json"));
    }
    p
}

fn load_checkpoint(path: &Path) -> Result<Checkpoint, ReindexError> {
    let raw = fs::read_to_string(path)?;
    let cp: Checkpoint = serde_json::from_str(&raw)?;
    Ok(cp)
}

fn save_checkpoint(
    path: &Path,
    next_tx_index: usize,
    mappings: &[MappingEntry],
    report: &ReindexReport,
) -> Result<(), ReindexError> {
    let cp = Checkpoint {
        next_tx_index,
        mappings: mappings.to_vec(),
        report: report.clone(),
    };
    let json = serde_json::to_string_pretty(&cp)?;
    fs::write(path, json)?;
    Ok(())
}

fn now_unix_seconds() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}
