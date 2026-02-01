use hex::encode as hex_encode;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tenebrium_consensus::{
    check_pow, header_hash, merkle_root, Block, BlockHeader, ConsensusError,
};
use tenebrium_utxo::{ApplyReceipt, InMemoryUtxoSet, Transaction, TxOut, UtxoError, UtxoSet};

use crate::mempool::{Mempool, MempoolConfig, MempoolError};
use crate::utxo_db::{
    decode_outpoint, decode_txout, encode_outpoint, encode_txout, jsonl_reader, UtxoDbError,
    UtxoReader,
};
use crate::LogLevel;

#[derive(Debug, thiserror::Error)]
pub enum P2pError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid message length")]
    InvalidLength,
    #[error("UTXO error: {0}")]
    Utxo(#[from] UtxoError),
    #[error("consensus error: {0}")]
    Consensus(#[from] ConsensusError),
    #[error("mempool error: {0}")]
    Mempool(#[from] MempoolError),
    #[error("UTXO DB error: {0}")]
    UtxoDb(#[from] UtxoDbError),
    #[error("sled error: {0}")]
    Sled(#[from] sled::Error),
    #[error("block invalid: {0}")]
    InvalidBlock(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum P2pMessage {
    Hello {
        version: u32,
        network: String,
        node_id: String,
        txid_version: Option<u8>,
    },
    Addr(Vec<String>),
    Inv {
        txids: Vec<[u8; 32]>,
        blocks: Vec<[u8; 32]>,
    },
    GetTx(Vec<[u8; 32]>),
    GetBlock(Vec<[u8; 32]>),
    GetHeaders {
        locator: Vec<[u8; 32]>,
    },
    Headers(Vec<BlockHeader>),
    Ping,
    Pong,
    Tx(Transaction),
    Block(Block),
}

const PROTOCOL_VERSION: u32 = 1;
const MIN_PROTOCOL_VERSION: u32 = 1;
const MAX_PROTOCOL_VERSION: u32 = 1;
const MAX_FUTURE_DRIFT_SECS: u32 = 2 * 60 * 60;
const MAX_MESSAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_ADDR: usize = 1000;
const MAX_INV: usize = 5000;
const MAX_GET: usize = 2000;
const MAX_HEADERS: usize = 2000;
const MAX_PEERS: usize = 64;
const MAX_NODE_ID_LEN: usize = 64;
const MAX_NETWORK_ID_LEN: usize = 16;
const TXID_VERSION_V1: u8 = 1;
const TXID_VERSION_V2: u8 = 2;
const BAN_DURATION_SECS: u64 = 10 * 60;
const MSG_WINDOW_SECS: u64 = 60;
const MAX_MSGS_PER_WINDOW: u32 = 120;
const READ_TIMEOUT_SECS: u64 = 30;
const WRITE_TIMEOUT_SECS: u64 = 30;
const SEED_RETRY_BASE_SECS: u64 = 2;
const SEED_RETRY_MAX_SECS: u64 = 60;
const SEED_RETRY_ATTEMPTS: u32 = 8;
const SEED_DIAL_INTERVAL_SECS: u64 = 30;
const TARGET_BLOCK_TIME_SECS: u32 = 600;
const DIFFICULTY_WINDOW: u32 = 10;
const INITIAL_BITS: u32 = 0x207fffff;
const INITIAL_SUBSIDY: u64 = 50_0000_0000;
const HALVING_INTERVAL: u32 = 210_000;
const DB_SCHEMA_VERSION: u32 = 2;
const GENESIS_TIME: u32 = 1_769_936_400;
const GENESIS_BITS: u32 = 0x207fffff;
const GENESIS_NONCE: u32 = 2;
const GENESIS_MERKLE_ROOT: [u8; 32] = [
    169, 121, 2, 123, 39, 241, 216, 194, 36, 201, 186, 237, 157, 93, 25, 228, 155, 68, 174, 228, 3,
    8, 168, 36, 245, 208, 58, 173, 18, 205, 179, 58,
];

pub fn run_p2p(
    listen_addr: String,
    peers: Vec<String>,
    utxo_path: Option<PathBuf>,
    no_pow_check: bool,
    network_id: String,
    data_dir: Option<PathBuf>,
    stats_interval_secs: u64,
    log_level: LogLevel,
    log_file: Option<PathBuf>,
    txid_version: u8,
) -> Result<(), P2pError> {
    let listener = TcpListener::bind(&listen_addr)?;
    let logger = Arc::new(Logger::new(log_level, log_file)?);
    logger.info(format!("P2P listening on {listen_addr}"));

    let db = match data_dir.as_ref() {
        Some(dir) => Some(open_sled(dir)?),
        None => None,
    };

    let utxos = Arc::new(Mutex::new(load_utxos(
        utxo_path,
        data_dir.clone(),
        db.clone(),
    )?));
    let mempool = Arc::new(Mutex::new(Mempool::new(MempoolConfig::default())));
    let peers = Arc::new(Mutex::new(PeerManager::new(peers)));
    let blocks = Arc::new(Mutex::new(BlockStore::default()));
    let chain = Arc::new(Mutex::new(ChainState::load_or_genesis(db.clone())?));
    let applied = Arc::new(Mutex::new(AppliedState::new(
        chain
            .lock()
            .map_err(|_| P2pError::InvalidBlock("chain lock".to_string()))?
            .tip_hash(),
    )));
    let seen = Arc::new(Mutex::new(Seen::default()));
    let node_id = format!("node-{}", std::process::id());

    if stats_interval_secs > 0 {
        spawn_stats_thread(
            Arc::clone(&peers),
            Arc::clone(&mempool),
            Arc::clone(&utxos),
            Arc::clone(&chain),
            stats_interval_secs,
            Arc::clone(&logger),
        );
    }

    let initial_peers: Vec<String> = {
        let mut guard = peers
            .lock()
            .map_err(|_| P2pError::InvalidBlock("peers lock".to_string()))?;
        guard.list()
    };
    for peer in initial_peers {
        spawn_connect(
            peer,
            Arc::clone(&peers),
            Arc::clone(&mempool),
            Arc::clone(&utxos),
            Arc::clone(&blocks),
            Arc::clone(&chain),
            Arc::clone(&applied),
            Arc::clone(&seen),
            node_id.clone(),
            network_id.clone(),
            data_dir.clone(),
            db.clone(),
            no_pow_check,
            txid_version,
            Arc::clone(&logger),
        );
    }

    spawn_seed_dialer(
        Arc::clone(&peers),
        Arc::clone(&mempool),
        Arc::clone(&utxos),
        Arc::clone(&blocks),
        Arc::clone(&chain),
        Arc::clone(&applied),
        Arc::clone(&seen),
        node_id.clone(),
        network_id.clone(),
        data_dir.clone(),
        db.clone(),
        no_pow_check,
        txid_version,
        Arc::clone(&logger),
    );

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let peer = stream
                    .peer_addr()
                    .map(|p| p.to_string())
                    .unwrap_or_default();
                let can_accept = {
                    let mut guard = peers
                        .lock()
                        .map_err(|_| P2pError::InvalidBlock("peers lock".to_string()))?;
                    guard.allow_incoming(&peer)
                };
                if !can_accept {
                    logger.warn(format!("[{peer}] rejected incoming (limit/banned)"));
                    continue;
                }
                let peers_list = Arc::clone(&peers);
                let seen = Arc::clone(&seen);
                let mempool = Arc::clone(&mempool);
                let utxos = Arc::clone(&utxos);
                let blocks = Arc::clone(&blocks);
                let chain = Arc::clone(&chain);
                let applied = Arc::clone(&applied);
                let data_dir = data_dir.clone();
                let db = db.clone();
                let node_id = node_id.clone();
                let network_id = network_id.clone();
                let logger = Arc::clone(&logger);
                thread::spawn(move || {
                    let peers_for_conn = Arc::clone(&peers_list);
                    let res = handle_connection(
                        stream,
                        peer.clone(),
                        peers_for_conn,
                        mempool,
                        utxos,
                        blocks,
                        chain,
                        applied,
                        seen,
                        node_id,
                        network_id,
                        data_dir,
                        db,
                        no_pow_check,
                        txid_version,
                        Arc::clone(&logger),
                    );
                    if let Err(err) = res {
                        if should_ban(&err) {
                            if let Ok(mut guard) = peers_list.lock() {
                                guard.ban(&peer);
                            }
                        }
                        logger.warn(format!("[{peer}] disconnected: {err}"));
                    }
                });
            }
            Err(err) => return Err(P2pError::Io(err)),
        }
    }
    Ok(())
}

fn handle_connection(
    mut stream: TcpStream,
    peer: String,
    peers: Arc<Mutex<PeerManager>>,
    mempool: Arc<Mutex<Mempool>>,
    utxos: Arc<Mutex<InMemoryUtxoSet>>,
    blocks: Arc<Mutex<BlockStore>>,
    chain: Arc<Mutex<ChainState>>,
    applied: Arc<Mutex<AppliedState>>,
    seen: Arc<Mutex<Seen>>,
    node_id: String,
    network_id: String,
    data_dir: Option<PathBuf>,
    db: Option<Db>,
    no_pow_check: bool,
    txid_version: u8,
    logger: Arc<Logger>,
) -> Result<(), P2pError> {
    stream.set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECS)))?;
    stream.set_write_timeout(Some(Duration::from_secs(WRITE_TIMEOUT_SECS)))?;
    send_message(
        &mut stream,
        &P2pMessage::Hello {
            version: PROTOCOL_VERSION,
            network: network_id.clone(),
            node_id: node_id.clone(),
            txid_version: Some(txid_version),
        },
    )?;
    let tip = chain
        .lock()
        .map_err(|_| P2pError::InvalidBlock("chain lock".to_string()))?
        .tip_hash();
    send_message(&mut stream, &P2pMessage::GetHeaders { locator: vec![tip] })?;
    let mut rate = RateLimiter::new();
    loop {
        let msg = read_message(&mut stream)?;
        rate.bump()?;
        validate_message(&msg)?;
        match msg {
            P2pMessage::Hello {
                version,
                network,
                node_id,
                txid_version: peer_txid_opt,
            } => {
                if version < MIN_PROTOCOL_VERSION || version > MAX_PROTOCOL_VERSION {
                    return Err(P2pError::InvalidBlock(
                        "protocol version not supported".to_string(),
                    ));
                }
                let local_txid_version = txid_version;
                let peer_txid_version = match peer_txid_opt {
                    Some(version) => version,
                    None => {
                        return Err(P2pError::InvalidBlock(
                            "missing txid version (must match local)".to_string(),
                        ))
                    }
                };
                if peer_txid_version != TXID_VERSION_V1 && peer_txid_version != TXID_VERSION_V2 {
                    return Err(P2pError::InvalidBlock(format!(
                        "unsupported txid version {peer_txid_version}"
                    )));
                }
                if peer_txid_version != local_txid_version {
                    return Err(P2pError::InvalidBlock(format!(
                        "txid version mismatch (expected {local_txid_version})"
                    )));
                }
                if network != network_id {
                    return Err(P2pError::InvalidBlock("network mismatch".to_string()));
                }
                {
                    let mut guard = peers
                        .lock()
                        .map_err(|_| P2pError::InvalidBlock("peers lock".to_string()))?;
                    guard.add_peer(&peer)?;
                    guard.mark_seen(&peer);
                }
                logger.info(format!(
                    "[{peer}] hello v{version} net={network} id={node_id} txid=v{peer_txid_version}"
                ));
                let list = {
                    let mut guard = peers
                        .lock()
                        .map_err(|_| P2pError::InvalidBlock("peers lock".to_string()))?;
                    guard.list()
                };
                send_message(&mut stream, &P2pMessage::Addr(list))?;
            }
            P2pMessage::Addr(addrs) => {
                for addr in addrs {
                    let add_res = {
                        let mut guard = peers
                            .lock()
                            .map_err(|_| P2pError::InvalidBlock("peers lock".to_string()))?;
                        guard.add_peer(&addr)
                    };
                    match add_res {
                        Ok(true) => {
                            spawn_connect(
                                addr,
                                Arc::clone(&peers),
                                Arc::clone(&mempool),
                                Arc::clone(&utxos),
                                Arc::clone(&blocks),
                                Arc::clone(&chain),
                                Arc::clone(&applied),
                                Arc::clone(&seen),
                                node_id.clone(),
                                network_id.clone(),
                                data_dir.clone(),
                                db.clone(),
                                no_pow_check,
                                txid_version,
                                Arc::clone(&logger),
                            );
                        }
                        Ok(false) => {}
                        Err(err) => {
                            logger.warn(format!("[{peer}] addr rejected: {err}"));
                            break;
                        }
                    }
                }
            }
            P2pMessage::GetHeaders { locator } => {
                let headers = chain
                    .lock()
                    .map_err(|_| P2pError::InvalidBlock("chain lock".to_string()))?
                    .headers_after(locator, 2000);
                if !headers.is_empty() {
                    send_message(&mut stream, &P2pMessage::Headers(headers))?;
                }
            }
            P2pMessage::Headers(headers) => {
                let mut chain = chain
                    .lock()
                    .map_err(|_| P2pError::InvalidBlock("chain lock".to_string()))?;
                for header in headers {
                    if let Err(err) = chain.add_header(&header, no_pow_check) {
                        logger.warn(format!("[{peer}] header rejected: {err}"));
                        break;
                    }
                }
            }
            P2pMessage::Inv {
                txids,
                blocks: block_hashes,
            } => {
                let mut want_tx = Vec::new();
                let mut want_blocks = Vec::new();

                {
                    let mempool = mempool
                        .lock()
                        .map_err(|_| P2pError::InvalidBlock("mempool lock".to_string()))?;
                    for txid in txids {
                        let have = if txid_version == TXID_VERSION_V1 {
                            mempool.contains_v1(&txid)
                        } else {
                            mempool.contains(&txid)
                        };
                        if !have {
                            want_tx.push(txid);
                        }
                    }
                }

                {
                    let blocks_store = blocks
                        .lock()
                        .map_err(|_| P2pError::InvalidBlock("block store lock".to_string()))?;
                    for hash in block_hashes {
                        if !blocks_store.contains(&hash) {
                            want_blocks.push(hash);
                        }
                    }
                }

                if !want_tx.is_empty() {
                    send_message(&mut stream, &P2pMessage::GetTx(want_tx))?;
                }
                if !want_blocks.is_empty() {
                    send_message(&mut stream, &P2pMessage::GetBlock(want_blocks))?;
                }
            }
            P2pMessage::GetTx(txids) => {
                let mempool = mempool
                    .lock()
                    .map_err(|_| P2pError::InvalidBlock("mempool lock".to_string()))?;
                for txid in txids {
                    let tx = if txid_version == TXID_VERSION_V1 {
                        mempool.get_tx_v1(&txid)
                    } else {
                        mempool.get_tx(&txid)
                    };
                    if let Some(tx) = tx {
                        send_message(&mut stream, &P2pMessage::Tx(tx))?;
                    }
                }
            }
            P2pMessage::GetBlock(hashes) => {
                let blocks_store = blocks
                    .lock()
                    .map_err(|_| P2pError::InvalidBlock("block store lock".to_string()))?;
                for hash in hashes {
                    if let Some(block) = blocks_store.get(&hash) {
                        send_message(&mut stream, &P2pMessage::Block(block))?;
                    }
                }
            }
            P2pMessage::Ping => {
                logger.debug(format!("[{peer}] ping"));
                send_message(&mut stream, &P2pMessage::Pong)?;
            }
            P2pMessage::Pong => {
                logger.debug(format!("[{peer}] pong"));
            }
            P2pMessage::Tx(tx) => {
                let txid = txid_for_version(&tx, txid_version)?;
                if seen_tx(&seen, &txid)? {
                    continue;
                }
                let mut mempool = mempool
                    .lock()
                    .map_err(|_| P2pError::InvalidBlock("mempool lock".to_string()))?;
                let utxos = utxos
                    .lock()
                    .map_err(|_| P2pError::InvalidBlock("utxo lock".to_string()))?;
                match mempool.add_tx(tx, &*utxos) {
                    Ok(()) => {
                        logger.info(format!("[{peer}] tx accepted {txid:?}"));
                        broadcast_inv(&peers, vec![txid], vec![])?;
                    }
                    Err(err) => logger.warn(format!("[{peer}] tx rejected {txid:?}: {err}")),
                }
            }
            P2pMessage::Block(block) => {
                logger.info(format!("[{peer}] block with {} txs", block.txs.len()));
                let block_hash = header_hash(&block.header);
                if seen_block(&seen, &block_hash)? {
                    continue;
                }
                {
                    let mut blocks_store = blocks
                        .lock()
                        .map_err(|_| P2pError::InvalidBlock("block store lock".to_string()))?;
                    blocks_store.insert(block_hash, block.clone());
                }
                let best_tip = {
                    let mut chain = chain
                        .lock()
                        .map_err(|_| P2pError::InvalidBlock("chain lock".to_string()))?;
                    chain.add_header(&block.header, no_pow_check)?;
                    chain.tip_hash()
                };
                let mut utxos = utxos
                    .lock()
                    .map_err(|_| P2pError::InvalidBlock("utxo lock".to_string()))?;
                let mut mempool = mempool
                    .lock()
                    .map_err(|_| P2pError::InvalidBlock("mempool lock".to_string()))?;
                let mut applied = applied
                    .lock()
                    .map_err(|_| P2pError::InvalidBlock("applied lock".to_string()))?;
                let blocks_store = blocks
                    .lock()
                    .map_err(|_| P2pError::InvalidBlock("block store lock".to_string()))?;
                let chain_ref = chain
                    .lock()
                    .map_err(|_| P2pError::InvalidBlock("chain lock".to_string()))?;
                let mut evicted = Vec::new();
                if let Err(err) = reorg_to_tip(
                    &mut *applied,
                    &*chain_ref,
                    &*blocks_store,
                    &mut *utxos,
                    no_pow_check,
                    &mut evicted,
                ) {
                    logger.warn(format!("[{peer}] block rejected: {err}"));
                } else {
                    for tx in evicted {
                        let _ = mempool.add_tx(tx, &*utxos);
                    }
                    for tx in &block.txs {
                        if let Ok(txid) = txid_for_version(tx, txid_version) {
                            if txid_version == TXID_VERSION_V1 {
                                let _ = mempool.remove_tx_v1(&txid);
                            } else {
                                let _ = mempool.remove_tx(&txid);
                            }
                        }
                    }
                    if let Some(ref dir) = data_dir {
                        if let Err(err) = persist_block(dir, &block, &block_hash, db.clone()) {
                            logger.warn(format!("[{peer}] persist block failed: {err}"));
                        }
                        if let Err(err) = persist_utxos(dir, &utxos, db.clone()) {
                            logger.warn(format!("[{peer}] persist utxos failed: {err}"));
                        }
                    }
                    if best_tip == block_hash {
                        logger.info(format!("[{peer}] block accepted"));
                        broadcast_inv(&peers, vec![], vec![block_hash])?;
                    }
                }
            }
        }
    }
}

fn send_message(stream: &mut TcpStream, msg: &P2pMessage) -> Result<(), P2pError> {
    let data = serde_json::to_vec(msg)?;
    let len = data.len() as u32;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(&data)?;
    Ok(())
}

fn read_message(stream: &mut TcpStream) -> Result<P2pMessage, P2pError> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > MAX_MESSAGE_BYTES {
        return Err(P2pError::InvalidLength);
    }
    let mut data = vec![0u8; len];
    stream.read_exact(&mut data)?;
    parse_message_bytes(&data)
}

fn txid_for_version(tx: &Transaction, txid_version: u8) -> Result<[u8; 32], P2pError> {
    match txid_version {
        TXID_VERSION_V1 => Ok(tx.txid_v1()?),
        TXID_VERSION_V2 => Ok(tx.txid_v2()?),
        other => Err(P2pError::InvalidBlock(format!(
            "unsupported txid version {other}"
        ))),
    }
}

pub(crate) fn parse_message_bytes(data: &[u8]) -> Result<P2pMessage, P2pError> {
    let msg = serde_json::from_slice(data)?;
    validate_message(&msg)?;
    Ok(msg)
}

#[derive(Debug, Default)]
struct Seen {
    tx: HashSet<[u8; 32]>,
    block: HashSet<[u8; 32]>,
}

fn seen_tx(seen: &Arc<Mutex<Seen>>, txid: &[u8; 32]) -> Result<bool, P2pError> {
    let mut guard = seen
        .lock()
        .map_err(|_| P2pError::InvalidBlock("seen lock".to_string()))?;
    if guard.tx.contains(txid) {
        return Ok(true);
    }
    guard.tx.insert(*txid);
    Ok(false)
}

fn seen_block(seen: &Arc<Mutex<Seen>>, hash: &[u8; 32]) -> Result<bool, P2pError> {
    let mut guard = seen
        .lock()
        .map_err(|_| P2pError::InvalidBlock("seen lock".to_string()))?;
    if guard.block.contains(hash) {
        return Ok(true);
    }
    guard.block.insert(*hash);
    Ok(false)
}

fn broadcast_inv(
    peers: &Arc<Mutex<PeerManager>>,
    txids: Vec<[u8; 32]>,
    blocks: Vec<[u8; 32]>,
) -> Result<(), P2pError> {
    let list: Vec<String> = {
        let mut guard = peers
            .lock()
            .map_err(|_| P2pError::InvalidBlock("peers lock".to_string()))?;
        guard.list()
    };
    for peer in list {
        if let Ok(mut stream) = TcpStream::connect(peer) {
            let _ = send_message(
                &mut stream,
                &P2pMessage::Inv {
                    txids: txids.clone(),
                    blocks: blocks.clone(),
                },
            );
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct PeerManager {
    peers: HashSet<String>,
    banned: HashMap<String, Instant>,
    last_dial: HashMap<String, Instant>,
}

impl PeerManager {
    fn new(initial: Vec<String>) -> Self {
        Self {
            peers: initial.into_iter().collect(),
            banned: HashMap::new(),
            last_dial: HashMap::new(),
        }
    }

    fn purge_bans(&mut self) {
        let now = Instant::now();
        self.banned.retain(|_, until| *until > now);
    }

    fn is_banned(&mut self, addr: &str) -> bool {
        self.purge_bans();
        self.banned.contains_key(addr)
    }

    fn allow_incoming(&mut self, addr: &str) -> bool {
        self.purge_bans();
        if self.banned.contains_key(addr) || self.peers.len() >= MAX_PEERS {
            return false;
        }
        self.peers.insert(addr.to_string());
        true
    }

    fn add_peer(&mut self, addr: &str) -> Result<bool, P2pError> {
        self.purge_bans();
        if self.banned.contains_key(addr) {
            return Err(P2pError::InvalidBlock("peer banned".to_string()));
        }
        if self.peers.len() >= MAX_PEERS {
            return Err(P2pError::InvalidBlock("peer limit reached".to_string()));
        }
        Ok(self.peers.insert(addr.to_string()))
    }

    fn list(&mut self) -> Vec<String> {
        self.purge_bans();
        self.peers.iter().cloned().collect()
    }

    fn mark_dialed(&mut self, addr: &str) {
        self.last_dial.insert(addr.to_string(), Instant::now());
    }

    fn should_dial(&mut self, addr: &str) -> bool {
        self.purge_bans();
        let now = Instant::now();
        match self.last_dial.get(addr) {
            Some(ts) => now.duration_since(*ts).as_secs() >= SEED_DIAL_INTERVAL_SECS,
            None => true,
        }
    }

    fn ban(&mut self, addr: &str) {
        self.peers.remove(addr);
        self.banned.insert(
            addr.to_string(),
            Instant::now() + Duration::from_secs(BAN_DURATION_SECS),
        );
    }

    fn mark_seen(&mut self, _addr: &str) {}

    fn count(&mut self) -> usize {
        self.purge_bans();
        self.peers.len()
    }
}

fn validate_message(msg: &P2pMessage) -> Result<(), P2pError> {
    match msg {
        P2pMessage::Hello {
            network, node_id, ..
        } => {
            if node_id.is_empty() || node_id.len() > MAX_NODE_ID_LEN {
                return Err(P2pError::InvalidBlock("invalid node_id length".to_string()));
            }
            if network.is_empty() || network.len() > MAX_NETWORK_ID_LEN {
                return Err(P2pError::InvalidBlock(
                    "invalid network id length".to_string(),
                ));
            }
        }
        P2pMessage::Addr(addrs) => {
            if addrs.len() > MAX_ADDR {
                return Err(P2pError::InvalidBlock("addr list too large".to_string()));
            }
        }
        P2pMessage::Inv { txids, blocks } => {
            if txids.len() > MAX_INV || blocks.len() > MAX_INV {
                return Err(P2pError::InvalidBlock("inv list too large".to_string()));
            }
        }
        P2pMessage::GetTx(txids) => {
            if txids.len() > MAX_GET {
                return Err(P2pError::InvalidBlock("gettx list too large".to_string()));
            }
        }
        P2pMessage::GetBlock(hashes) => {
            if hashes.len() > MAX_GET {
                return Err(P2pError::InvalidBlock(
                    "getblock list too large".to_string(),
                ));
            }
        }
        P2pMessage::GetHeaders { locator } => {
            if locator.len() > MAX_GET {
                return Err(P2pError::InvalidBlock("locator list too large".to_string()));
            }
        }
        P2pMessage::Headers(headers) => {
            if headers.len() > MAX_HEADERS {
                return Err(P2pError::InvalidBlock("headers list too large".to_string()));
            }
        }
        _ => {}
    }
    Ok(())
}

struct RateLimiter {
    window_start: Instant,
    count: u32,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            count: 0,
        }
    }

    fn bump(&mut self) -> Result<(), P2pError> {
        if self.window_start.elapsed() >= Duration::from_secs(MSG_WINDOW_SECS) {
            self.window_start = Instant::now();
            self.count = 0;
        }
        self.count = self.count.saturating_add(1);
        if self.count > MAX_MSGS_PER_WINDOW {
            return Err(P2pError::InvalidBlock("rate limit exceeded".to_string()));
        }
        Ok(())
    }
}

fn should_ban(err: &P2pError) -> bool {
    matches!(
        err,
        P2pError::InvalidLength | P2pError::InvalidBlock(_) | P2pError::Json(_)
    )
}

#[derive(Clone)]
struct Logger {
    level: LogLevel,
    file: Option<Arc<Mutex<std::fs::File>>>,
}

impl Logger {
    fn new(level: LogLevel, file_path: Option<PathBuf>) -> Result<Self, P2pError> {
        let file = if let Some(path) = file_path {
            let f = OpenOptions::new().create(true).append(true).open(path)?;
            Some(Arc::new(Mutex::new(f)))
        } else {
            None
        };
        Ok(Self { level, file })
    }

    fn log(&self, level: LogLevel, msg: String) {
        if !self.level.allows(level) {
            return;
        }
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let line = format!("[{ts}] [{level:?}] {msg}");
        if matches!(level, LogLevel::Error | LogLevel::Warn) {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
        if let Some(ref file) = self.file {
            if let Ok(mut guard) = file.lock() {
                let _ = writeln!(guard, "{line}");
            }
        }
    }

    fn info(&self, msg: String) {
        self.log(LogLevel::Info, msg);
    }

    fn warn(&self, msg: String) {
        self.log(LogLevel::Warn, msg);
    }

    fn debug(&self, msg: String) {
        self.log(LogLevel::Debug, msg);
    }
}

fn spawn_stats_thread(
    peers: Arc<Mutex<PeerManager>>,
    mempool: Arc<Mutex<Mempool>>,
    utxos: Arc<Mutex<InMemoryUtxoSet>>,
    chain: Arc<Mutex<ChainState>>,
    interval_secs: u64,
    logger: Arc<Logger>,
) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(interval_secs));
        let peer_count = peers.lock().map(|mut p| p.count()).unwrap_or(0);
        let (mempool_len, mempool_bytes) = mempool
            .lock()
            .map(|m| (m.len(), m.total_bytes()))
            .unwrap_or((0, 0));
        let utxo_count = utxos.lock().map(|u| u.entries().len()).unwrap_or(0);
        let (tip, height) = chain
            .lock()
            .map(|c| {
                let tip = c.tip_hash();
                let height = c.heights.get(&tip).cloned().unwrap_or(0);
                (tip, height)
            })
            .unwrap_or(([0u8; 32], 0));
        logger.info(format!(
            "[stats] peers={peer_count} mempool={mempool_len} mempool_bytes={mempool_bytes} utxo={utxo_count} tip={} height={height}",
            hex_encode(tip)
        ));
    });
}

fn spawn_connect(
    peer: String,
    peers: Arc<Mutex<PeerManager>>,
    mempool: Arc<Mutex<Mempool>>,
    utxos: Arc<Mutex<InMemoryUtxoSet>>,
    blocks: Arc<Mutex<BlockStore>>,
    chain: Arc<Mutex<ChainState>>,
    applied: Arc<Mutex<AppliedState>>,
    seen: Arc<Mutex<Seen>>,
    node_id: String,
    network_id: String,
    data_dir: Option<PathBuf>,
    db: Option<Db>,
    no_pow_check: bool,
    txid_version: u8,
    logger: Arc<Logger>,
) {
    thread::spawn(move || {
        if let Ok(mut guard) = peers.lock() {
            guard.mark_dialed(&peer);
        }
        for attempt in 0..SEED_RETRY_ATTEMPTS {
            let should_connect = {
                let mut guard = match peers.lock() {
                    Ok(guard) => guard,
                    Err(_) => return,
                };
                !guard.is_banned(&peer)
            };
            if !should_connect {
                return;
            }

            if let Ok(mut stream) = TcpStream::connect(&peer) {
                let node_id_clone = node_id.clone();
                let network_id_clone = network_id.clone();
                let _ = send_message(
                    &mut stream,
                    &P2pMessage::Hello {
                        version: PROTOCOL_VERSION,
                        network: network_id_clone,
                        node_id: node_id_clone,
                        txid_version: Some(txid_version),
                    },
                );
                let res = handle_connection(
                    stream,
                    peer.clone(),
                    peers.clone(),
                    mempool,
                    utxos,
                    blocks,
                    chain,
                    applied,
                    seen,
                    node_id,
                    network_id,
                    data_dir,
                    db,
                    no_pow_check,
                    txid_version,
                    Arc::clone(&logger),
                );
                if let Err(err) = res {
                    if should_ban(&err) {
                        if let Ok(mut guard) = peers.lock() {
                            guard.ban(&peer);
                        }
                    }
                    logger.warn(format!("[{peer}] disconnected: {err}"));
                }
                return;
            }

            let backoff = SEED_RETRY_BASE_SECS.saturating_mul(2u64.saturating_pow(attempt));
            let sleep_for = backoff.min(SEED_RETRY_MAX_SECS);
            logger.debug(format!("[{peer}] connect failed, retry in {sleep_for}s"));
            thread::sleep(Duration::from_secs(sleep_for));
        }
    });
}

fn spawn_seed_dialer(
    peers: Arc<Mutex<PeerManager>>,
    mempool: Arc<Mutex<Mempool>>,
    utxos: Arc<Mutex<InMemoryUtxoSet>>,
    blocks: Arc<Mutex<BlockStore>>,
    chain: Arc<Mutex<ChainState>>,
    applied: Arc<Mutex<AppliedState>>,
    seen: Arc<Mutex<Seen>>,
    node_id: String,
    network_id: String,
    data_dir: Option<PathBuf>,
    db: Option<Db>,
    no_pow_check: bool,
    txid_version: u8,
    logger: Arc<Logger>,
) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(SEED_DIAL_INTERVAL_SECS));
        let list = {
            let mut guard = match peers.lock() {
                Ok(guard) => guard,
                Err(_) => continue,
            };
            guard.list()
        };
        for peer in list {
            let should_dial = {
                let mut guard = match peers.lock() {
                    Ok(guard) => guard,
                    Err(_) => continue,
                };
                guard.should_dial(&peer)
            };
            if !should_dial {
                continue;
            }
            spawn_connect(
                peer,
                Arc::clone(&peers),
                Arc::clone(&mempool),
                Arc::clone(&utxos),
                Arc::clone(&blocks),
                Arc::clone(&chain),
                Arc::clone(&applied),
                Arc::clone(&seen),
                node_id.clone(),
                network_id.clone(),
                data_dir.clone(),
                db.clone(),
                no_pow_check,
                txid_version,
                Arc::clone(&logger),
            );
        }
    });
}

fn load_utxos(
    path: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    db: Option<Db>,
) -> Result<InMemoryUtxoSet, P2pError> {
    let mut set = InMemoryUtxoSet::new();
    if let Some(path) = path {
        if path.exists() {
            let reader = jsonl_reader(&path);
            reader.for_each(|entry| {
                set.insert(entry.outpoint, entry.txout);
                Ok(())
            })?;
        }
        return Ok(set);
    }

    if let Some(db) = db.clone() {
        let tree = db.open_tree("utxo")?;
        if tree.iter().next().is_some() {
            for item in tree.iter() {
                let (k, v) = item?;
                let outpoint = decode_outpoint(&k)?;
                let txout = decode_txout(&v)?
                    .ok_or_else(|| UtxoDbError::InvalidData("missing txout in sled".to_string()))?;
                set.insert(outpoint, txout);
            }
            if let Some(expected) = load_utxo_count(&db)? {
                let actual = set.entries().len() as u64;
                if expected != actual {
                    return Err(P2pError::InvalidBlock("utxo count mismatch".to_string()));
                }
            }
            return Ok(set);
        }
    }
    if let Some(dir) = data_dir {
        let jsonl_path = dir.join("utxo.jsonl");
        if jsonl_path.exists() {
            let reader = jsonl_reader(&jsonl_path);
            reader.for_each(|entry| {
                set.insert(entry.outpoint, entry.txout);
                Ok(())
            })?;
        }
    }
    Ok(set)
}

fn load_utxo_count(db: &Db) -> Result<Option<u64>, P2pError> {
    let meta = db.open_tree("meta")?;
    if let Some(value) = meta.get("utxo_count")? {
        if value.len() == 8 {
            let count = u64::from_le_bytes(
                value
                    .as_ref()
                    .try_into()
                    .map_err(|_| P2pError::InvalidBlock("invalid utxo_count bytes".to_string()))?,
            );
            return Ok(Some(count));
        }
    }
    Ok(None)
}

fn load_tip_meta(db: &Db) -> Result<Option<([u8; 32], u32)>, P2pError> {
    let meta = db.open_tree("meta")?;
    let hash = meta.get("tip_hash")?;
    let height = meta.get("tip_height")?;
    match (hash, height) {
        (Some(h), Some(he)) if h.len() == 32 && he.len() == 4 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(&h);
            let height = u32::from_le_bytes(
                he.as_ref()
                    .try_into()
                    .map_err(|_| P2pError::InvalidBlock("invalid tip height bytes".to_string()))?,
            );
            Ok(Some((out, height)))
        }
        _ => Ok(None),
    }
}

fn ensure_db_schema(db: &Db) -> Result<(), P2pError> {
    let meta = db.open_tree("meta")?;
    if let Some(val) = meta.get("schema_version")? {
        if val.len() == 4 {
            let ver =
                u32::from_le_bytes(val.as_ref().try_into().map_err(|_| {
                    P2pError::InvalidBlock("invalid schema_version bytes".to_string())
                })?);
            if ver != DB_SCHEMA_VERSION {
                return Err(P2pError::InvalidBlock(
                    "unsupported db schema version".to_string(),
                ));
            }
            return Ok(());
        }
        return Err(P2pError::InvalidBlock("invalid schema_version".to_string()));
    }
    meta.insert("schema_version", DB_SCHEMA_VERSION.to_le_bytes().to_vec())?;
    meta.insert("network_id", b"mainnet".to_vec())?;
    meta.flush()?;
    Ok(())
}

fn apply_block_with_undo(
    block: &Block,
    utxos: &mut InMemoryUtxoSet,
    no_pow_check: bool,
    subsidy: u64,
) -> Result<Vec<ApplyReceipt>, P2pError> {
    if !no_pow_check {
        let ok = check_pow(&block.header)?;
        if !ok {
            return Err(P2pError::InvalidBlock("invalid PoW".to_string()));
        }
    }

    if block.txs.is_empty() {
        return Err(P2pError::InvalidBlock("empty block".to_string()));
    }

    let txids = block
        .txs
        .iter()
        .map(|tx| tx.txid_v2())
        .collect::<Result<Vec<_>, _>>()?;
    let root = merkle_root(&txids);
    if root != block.header.merkle_root {
        return Err(P2pError::InvalidBlock("merkle root mismatch".to_string()));
    }

    let mut total_fees = 0u64;
    let mut receipts = Vec::new();
    for (i, tx) in block.txs.iter().enumerate() {
        if i == 0 {
            if !tx.vin.is_empty() {
                return Err(P2pError::InvalidBlock(
                    "coinbase must have no inputs".to_string(),
                ));
            }
            receipts.push(apply_coinbase(tx, utxos)?);
        } else {
            let fee = Transaction::validate_value_conservation(tx, utxos)?;
            total_fees = total_fees.saturating_add(fee);
            let receipt = utxos.apply_tx(tx)?;
            receipts.push(receipt);
        }
    }
    let coinbase = &block.txs[0];
    let out_sum = Transaction::sum_outputs(coinbase)?;
    if out_sum > subsidy.saturating_add(total_fees) {
        return Err(P2pError::InvalidBlock(
            "coinbase exceeds subsidy+fees".to_string(),
        ));
    }
    Ok(receipts)
}

fn apply_coinbase(tx: &Transaction, utxos: &mut InMemoryUtxoSet) -> Result<ApplyReceipt, P2pError> {
    tx.validate()?;
    let outpoints = Transaction::make_outpoints(tx)?;
    let mut inserted = Vec::new();
    for (op, txout) in outpoints.into_iter().zip(tx.vout.iter()) {
        if utxos.get(&op).is_some() {
            return Err(P2pError::InvalidBlock("coinbase output exists".to_string()));
        }
        utxos.insert(
            op.clone(),
            TxOut {
                value: txout.value,
                script_pubkey: txout.script_pubkey.clone(),
            },
        );
        inserted.push(op);
    }
    Ok(ApplyReceipt {
        removed: Vec::new(),
        inserted,
    })
}

fn persist_block(
    dir: &PathBuf,
    block: &Block,
    hash: &[u8; 32],
    db: Option<Db>,
) -> Result<(), P2pError> {
    let blocks_dir = dir.join("blocks");
    std::fs::create_dir_all(&blocks_dir)?;
    let file = blocks_dir.join(format!("{}.json", hex_encode(hash)));
    let tmp = blocks_dir.join(format!("{}.json.tmp", hex_encode(hash)));
    let json = serde_json::to_string_pretty(block)?;
    std::fs::write(&tmp, json)?;
    if file.exists() {
        std::fs::remove_file(&file)?;
    }
    std::fs::rename(&tmp, &file)?;
    if let Some(db) = db {
        let tree = db.open_tree("blocks")?;
        let key = hash.to_vec();
        let value = serde_json::to_vec(block)?;
        tree.insert(key, value)?;
        tree.flush()?;
    }
    Ok(())
}

fn persist_utxos(dir: &PathBuf, utxos: &InMemoryUtxoSet, db: Option<Db>) -> Result<(), P2pError> {
    let file = dir.join("utxo.jsonl");
    let tmp = dir.join("utxo.jsonl.tmp");
    let mut writer = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
    for (outpoint, txout) in utxos.entries() {
        let entry = crate::utxo_db::UtxoEntry { outpoint, txout };
        serde_json::to_writer(&mut writer, &entry)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    if file.exists() {
        std::fs::remove_file(&file)?;
    }
    std::fs::rename(&tmp, &file)?;
    if let Some(db) = db {
        let tree = db.open_tree("utxo")?;
        for (outpoint, txout) in utxos.entries() {
            let key = encode_outpoint(&outpoint);
            let value = encode_txout(&txout);
            tree.insert(key, value)?;
        }
        tree.flush()?;

        let meta = db.open_tree("meta")?;
        let count = utxos.entries().len() as u64;
        meta.insert("utxo_count", count.to_le_bytes().to_vec())?;
        meta.flush()?;
    }
    Ok(())
}

pub(crate) fn open_sled(dir: &PathBuf) -> Result<Db, P2pError> {
    let db_path = dir.join("chain.sled");
    let db = sled::open(db_path)?;
    ensure_db_schema(&db)?;
    Ok(db)
}

#[derive(Debug)]
struct ChainState {
    headers: HashMap<[u8; 32], BlockHeader>,
    heights: HashMap<[u8; 32], u32>,
    work: HashMap<[u8; 32], u128>,
    tip: [u8; 32],
    db: Option<Db>,
}

#[derive(Debug)]
struct AppliedState {
    tip: [u8; 32],
    undo: HashMap<[u8; 32], Vec<ApplyReceipt>>,
}

impl AppliedState {
    fn new(tip: [u8; 32]) -> Self {
        Self {
            tip,
            undo: HashMap::new(),
        }
    }
}

impl ChainState {
    fn with_genesis(db: Option<Db>) -> Self {
        let genesis = BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: GENESIS_MERKLE_ROOT,
            time: GENESIS_TIME,
            bits: GENESIS_BITS,
            nonce: GENESIS_NONCE,
        };
        let hash = header_hash(&genesis);
        let mut headers = HashMap::new();
        let mut heights = HashMap::new();
        let mut work = HashMap::new();
        headers.insert(hash, genesis);
        heights.insert(hash, 0);
        work.insert(hash, 0);
        Self {
            headers,
            heights,
            work,
            tip: hash,
            db,
        }
    }

    fn load_or_genesis(db: Option<Db>) -> Result<Self, P2pError> {
        let Some(db) = db.clone() else {
            return Ok(Self::with_genesis(db));
        };
        let headers_tree = db.open_tree("headers")?;
        if headers_tree.is_empty() {
            return Ok(Self::with_genesis(Some(db)));
        }
        let heights_tree = db.open_tree("heights")?;
        let work_tree = db.open_tree("work")?;

        let mut headers = HashMap::new();
        let mut heights = HashMap::new();
        let mut work = HashMap::new();
        for item in headers_tree.iter() {
            let (k, v) = item?;
            let hash = decode_hash(&k)?;
            let header: BlockHeader = serde_json::from_slice(&v)?;
            headers.insert(hash, header);
        }
        for item in heights_tree.iter() {
            let (k, v) = item?;
            let hash = decode_hash(&k)?;
            if v.len() == 4 {
                let h = u32::from_le_bytes(
                    v.as_ref()
                        .try_into()
                        .map_err(|_| P2pError::InvalidBlock("invalid height bytes".to_string()))?,
                );
                heights.insert(hash, h);
            }
        }
        for item in work_tree.iter() {
            let (k, v) = item?;
            let hash = decode_hash(&k)?;
            if v.len() == 16 {
                let w = u128::from_le_bytes(
                    v.as_ref()
                        .try_into()
                        .map_err(|_| P2pError::InvalidBlock("invalid work bytes".to_string()))?,
                );
                work.insert(hash, w);
            }
        }

        let mut tip = [0u8; 32];
        let mut tip_work = 0u128;
        let mut tip_height = 0u32;
        for (hash, w) in work.iter() {
            let h = *heights.get(hash).unwrap_or(&0);
            if *w > tip_work || (*w == tip_work && h > tip_height) {
                tip_work = *w;
                tip_height = h;
                tip = *hash;
            }
        }
        if tip == [0u8; 32] && !headers.is_empty() {
            tip = *headers.keys().next().unwrap();
        }
        if let Some((meta_tip, meta_height)) = load_tip_meta(&db)? {
            if heights.get(&meta_tip) == Some(&meta_height) {
                tip = meta_tip;
                tip_height = meta_height;
            }
        }
        let meta = db.open_tree("meta")?;
        meta.insert("tip_hash", tip.to_vec())?;
        meta.insert("tip_height", tip_height.to_le_bytes().to_vec())?;
        meta.flush()?;

        Ok(Self {
            headers,
            heights,
            work,
            tip,
            db: Some(db),
        })
    }

    fn tip_hash(&self) -> [u8; 32] {
        self.tip
    }

    fn add_header(&mut self, header: &BlockHeader, no_pow_check: bool) -> Result<(), P2pError> {
        let hash = header_hash(header);
        if self.headers.contains_key(&hash) {
            return Ok(());
        }
        let prev = header.prev_block_hash;
        let (height, prev_work, prev_header) = if let Some(prev_h) = self.heights.get(&prev) {
            let prev_work = *self.work.get(&prev).unwrap_or(&0);
            let prev_header = self
                .headers
                .get(&prev)
                .ok_or_else(|| P2pError::InvalidBlock("missing prev header".to_string()))?;
            (prev_h + 1, prev_work, Some(prev_header))
        } else if prev == [0u8; 32] {
            (0, 0, None)
        } else {
            return Err(P2pError::InvalidBlock("unknown prev header".to_string()));
        };

        let expected_bits = self.expected_bits(prev_header, height)?;
        validate_header_rules(header, prev_header, no_pow_check, expected_bits)?;
        let work = prev_work.saturating_add(work_from_bits(header.bits)?);
        self.headers.insert(hash, header.clone());
        self.heights.insert(hash, height);
        self.work.insert(hash, work);
        let tip_height = *self.heights.get(&self.tip).unwrap_or(&0);
        let tip_work = *self.work.get(&self.tip).unwrap_or(&0);
        if work > tip_work || (work == tip_work && height > tip_height) {
            self.tip = hash;
        }
        self.persist_header(hash, header, height, work)?;
        self.persist_tip()?;
        Ok(())
    }

    #[allow(dead_code)]
    fn next_height(&self, prev_hash: &[u8; 32]) -> Result<u32, P2pError> {
        if *prev_hash == [0u8; 32] {
            return Ok(0);
        }
        self.heights
            .get(prev_hash)
            .map(|h| h + 1)
            .ok_or_else(|| P2pError::InvalidBlock("unknown prev header".to_string()))
    }

    fn headers_after(&self, locator: Vec<[u8; 32]>, limit: usize) -> Vec<BlockHeader> {
        let mut start = None;
        for hash in locator {
            if self.headers.contains_key(&hash) {
                start = Some(hash);
                break;
            }
        }
        let Some(start_hash) = start else {
            return Vec::new();
        };
        let start_height = match self.heights.get(&start_hash) {
            Some(h) => *h,
            None => return Vec::new(),
        };
        let mut out = Vec::new();
        let mut current_height = start_height + 1;
        while out.len() < limit {
            let next = self
                .heights
                .iter()
                .find(|(_, h)| **h == current_height)
                .map(|(hash, _)| *hash);
            let Some(next_hash) = next else {
                break;
            };
            if let Some(header) = self.headers.get(&next_hash) {
                out.push(header.clone());
            }
            current_height += 1;
        }
        out
    }

    fn expected_bits(&self, prev: Option<&BlockHeader>, height: u32) -> Result<u32, P2pError> {
        let Some(prev) = prev else {
            return Ok(INITIAL_BITS);
        };
        if height == 0 || height % DIFFICULTY_WINDOW != 0 {
            return Ok(prev.bits);
        }

        let window_start_height = height.saturating_sub(DIFFICULTY_WINDOW);
        let start_hash = self
            .heights
            .iter()
            .find(|(_, h)| **h == window_start_height)
            .map(|(hash, _)| *hash)
            .ok_or_else(|| P2pError::InvalidBlock("missing window start".to_string()))?;
        let start_header = self
            .headers
            .get(&start_hash)
            .ok_or_else(|| P2pError::InvalidBlock("missing window header".to_string()))?;

        let actual_time = prev.time.saturating_sub(start_header.time);
        let expected_time = TARGET_BLOCK_TIME_SECS.saturating_mul(DIFFICULTY_WINDOW);

        let prev_target = bits_to_target_u128(prev.bits)?;
        let mut new_target =
            prev_target.saturating_mul(actual_time as u128) / (expected_time.max(1) as u128);

        let max_target = bits_to_target_u128(prev.bits)?; // clamp to previous for now
        if new_target > max_target {
            new_target = max_target;
        }

        Ok(target_to_bits(new_target))
    }

    fn persist_header(
        &self,
        hash: [u8; 32],
        header: &BlockHeader,
        height: u32,
        work: u128,
    ) -> Result<(), P2pError> {
        let Some(db) = self.db.as_ref() else {
            return Ok(());
        };
        let headers_tree = db.open_tree("headers")?;
        let heights_tree = db.open_tree("heights")?;
        let work_tree = db.open_tree("work")?;
        headers_tree.insert(hash.to_vec(), serde_json::to_vec(header)?)?;
        heights_tree.insert(hash.to_vec(), height.to_le_bytes().to_vec())?;
        work_tree.insert(hash.to_vec(), work.to_le_bytes().to_vec())?;
        Ok(())
    }

    fn persist_tip(&self) -> Result<(), P2pError> {
        let Some(db) = self.db.as_ref() else {
            return Ok(());
        };
        let meta = db.open_tree("meta")?;
        let tip_height = *self.heights.get(&self.tip).unwrap_or(&0);
        meta.insert("tip_hash", self.tip.to_vec())?;
        meta.insert("tip_height", tip_height.to_le_bytes().to_vec())?;
        meta.flush()?;
        Ok(())
    }

    fn height_of(&self, hash: &[u8; 32]) -> Option<u32> {
        self.heights.get(hash).copied()
    }

    fn header_of(&self, hash: &[u8; 32]) -> Option<&BlockHeader> {
        self.headers.get(hash)
    }
}

fn decode_hash(bytes: &[u8]) -> Result<[u8; 32], P2pError> {
    if bytes.len() != 32 {
        return Err(P2pError::InvalidBlock("hash length mismatch".to_string()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(out)
}

fn reorg_to_tip(
    applied: &mut AppliedState,
    chain: &ChainState,
    blocks: &BlockStore,
    utxos: &mut InMemoryUtxoSet,
    no_pow_check: bool,
    evicted: &mut Vec<Transaction>,
) -> Result<(), P2pError> {
    let new_tip = chain.tip_hash();
    if new_tip == applied.tip {
        return Ok(());
    }
    let ancestor = common_ancestor(chain, applied.tip, new_tip)?;
    let old_path = path_to_ancestor(chain, applied.tip, ancestor)?;
    let new_path = path_from_ancestor(chain, ancestor, new_tip)?;

    for hash in old_path.iter() {
        if let Some(block) = blocks.get(hash) {
            for tx in block.txs.iter().skip(1) {
                evicted.push(tx.clone());
            }
        }
        let receipts = applied
            .undo
            .remove(hash)
            .ok_or_else(|| P2pError::InvalidBlock("missing undo data".to_string()))?;
        for receipt in receipts.into_iter().rev() {
            utxos.rollback(receipt)?;
        }
    }

    for hash in new_path.iter() {
        let block = blocks
            .get(hash)
            .ok_or_else(|| P2pError::InvalidBlock("missing block data".to_string()))?;
        let height = chain
            .height_of(hash)
            .ok_or_else(|| P2pError::InvalidBlock("missing height".to_string()))?;
        let subsidy = block_subsidy(height);
        let receipts = apply_block_with_undo(&block, utxos, no_pow_check, subsidy)?;
        applied.undo.insert(*hash, receipts);
    }

    applied.tip = new_tip;
    Ok(())
}

fn common_ancestor(
    chain: &ChainState,
    mut a: [u8; 32],
    mut b: [u8; 32],
) -> Result<[u8; 32], P2pError> {
    let mut ha = chain
        .height_of(&a)
        .ok_or_else(|| P2pError::InvalidBlock("missing height".to_string()))?;
    let mut hb = chain
        .height_of(&b)
        .ok_or_else(|| P2pError::InvalidBlock("missing height".to_string()))?;
    while ha > hb {
        a = chain
            .header_of(&a)
            .ok_or_else(|| P2pError::InvalidBlock("missing header".to_string()))?
            .prev_block_hash;
        ha -= 1;
    }
    while hb > ha {
        b = chain
            .header_of(&b)
            .ok_or_else(|| P2pError::InvalidBlock("missing header".to_string()))?
            .prev_block_hash;
        hb -= 1;
    }
    while a != b {
        a = chain
            .header_of(&a)
            .ok_or_else(|| P2pError::InvalidBlock("missing header".to_string()))?
            .prev_block_hash;
        b = chain
            .header_of(&b)
            .ok_or_else(|| P2pError::InvalidBlock("missing header".to_string()))?
            .prev_block_hash;
    }
    Ok(a)
}

fn path_to_ancestor(
    chain: &ChainState,
    mut from: [u8; 32],
    ancestor: [u8; 32],
) -> Result<Vec<[u8; 32]>, P2pError> {
    let mut out = Vec::new();
    while from != ancestor {
        out.push(from);
        from = chain
            .header_of(&from)
            .ok_or_else(|| P2pError::InvalidBlock("missing header".to_string()))?
            .prev_block_hash;
    }
    Ok(out)
}

fn path_from_ancestor(
    chain: &ChainState,
    ancestor: [u8; 32],
    mut to: [u8; 32],
) -> Result<Vec<[u8; 32]>, P2pError> {
    let mut rev = Vec::new();
    while to != ancestor {
        rev.push(to);
        to = chain
            .header_of(&to)
            .ok_or_else(|| P2pError::InvalidBlock("missing header".to_string()))?
            .prev_block_hash;
    }
    rev.reverse();
    Ok(rev)
}

fn block_subsidy(height: u32) -> u64 {
    let halvings = height / HALVING_INTERVAL;
    if halvings >= 64 {
        return 0;
    }
    INITIAL_SUBSIDY >> halvings
}

fn work_from_bits(bits: u32) -> Result<u128, P2pError> {
    if bits == 0 {
        return Err(P2pError::InvalidBlock("invalid bits".to_string()));
    }
    let exponent = (bits >> 24) as u32;
    let mantissa = bits & 0x007f_ffff;
    if mantissa == 0 {
        return Err(P2pError::InvalidBlock("invalid bits".to_string()));
    }

    let target_u128 = if exponent <= 3 {
        let shift = 8 * (3 - exponent);
        (mantissa as u128) >> shift
    } else {
        let shift = 8 * (exponent - 3);
        if shift >= 128 {
            u128::MAX
        } else {
            (mantissa as u128) << shift
        }
    };

    if target_u128 == 0 {
        return Ok(1u128 << 120);
    }
    Ok((1u128 << 120) / (target_u128.saturating_add(1)))
}

fn validate_header_rules(
    header: &BlockHeader,
    prev: Option<&BlockHeader>,
    no_pow_check: bool,
    expected_bits: u32,
) -> Result<(), P2pError> {
    if !no_pow_check {
        let ok = check_pow(header)?;
        if !ok {
            return Err(P2pError::InvalidBlock("invalid PoW".to_string()));
        }
    }

    if header.bits == 0 {
        return Err(P2pError::InvalidBlock("invalid bits".to_string()));
    }

    if let Some(prev) = prev {
        if header.time < prev.time {
            return Err(P2pError::InvalidBlock("time too old".to_string()));
        }
        if header.bits != expected_bits {
            return Err(P2pError::InvalidBlock(
                "unexpected difficulty bits".to_string(),
            ));
        }
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| P2pError::InvalidBlock("invalid system time".to_string()))?
        .as_secs() as u32;
    if header.time > now.saturating_add(MAX_FUTURE_DRIFT_SECS) {
        return Err(P2pError::InvalidBlock("time too far in future".to_string()));
    }
    Ok(())
}

fn bits_to_target_u128(bits: u32) -> Result<u128, P2pError> {
    if bits == 0 {
        return Err(P2pError::InvalidBlock("invalid bits".to_string()));
    }
    let exponent = (bits >> 24) as u32;
    let mantissa = bits & 0x007f_ffff;
    if mantissa == 0 {
        return Err(P2pError::InvalidBlock("invalid bits".to_string()));
    }
    if exponent <= 3 {
        let shift = 8 * (3 - exponent);
        Ok((mantissa as u128) >> shift)
    } else {
        let shift = 8 * (exponent - 3);
        if shift >= 128 {
            Ok(u128::MAX)
        } else {
            Ok((mantissa as u128) << shift)
        }
    }
}

fn target_to_bits(target: u128) -> u32 {
    if target == 0 {
        return 0;
    }
    let mut tmp = target;
    let mut exponent: u32 = 0;
    while tmp > 0 {
        exponent += 1;
        tmp >>= 8;
    }
    let exponent = exponent.max(1);
    let shift = 8 * (exponent.saturating_sub(3));
    let mantissa = if shift >= 128 {
        0x007f_ffff
    } else {
        ((target >> shift) as u32) & 0x007f_ffff
    };
    (exponent << 24) | mantissa
}

#[cfg(test)]
mod reorg_tests {
    use super::*;
    use tenebrium_utxo::{OutPoint, Transaction, TxOut, UtxoSet};

    fn make_coinbase(value: u64, tag: u8) -> Transaction {
        Transaction {
            version: 1,
            vin: vec![],
            vout: vec![TxOut {
                value,
                script_pubkey: vec![tag],
            }],
            lock_time: 0,
        }
    }

    fn make_header(prev: [u8; 32], time: u32) -> BlockHeader {
        BlockHeader {
            version: 1,
            prev_block_hash: prev,
            merkle_root: [0u8; 32],
            time,
            bits: INITIAL_BITS,
            nonce: 0,
        }
    }

    #[test]
    fn expected_bits_window_boundary_keeps_bits_when_on_target() {
        let mut chain = ChainState::with_genesis(None);
        let mut prev_hash = chain.tip_hash();
        let expected_time = TARGET_BLOCK_TIME_SECS * DIFFICULTY_WINDOW;
        let base_time = GENESIS_TIME;
        for i in 0..9u32 {
            let time = if i == 8 {
                base_time + expected_time
            } else {
                base_time
            };
            let header = make_header(prev_hash, time);
            chain.add_header(&header, true).unwrap();
            prev_hash = header_hash(&header);
        }
        let prev_header = chain.headers.get(&prev_hash).unwrap();
        let expected = chain.expected_bits(Some(prev_header), 10).unwrap();
        assert_ne!(expected, 0);
        let header = BlockHeader {
            version: 1,
            prev_block_hash: prev_hash,
            merkle_root: [0u8; 32],
            time: base_time + expected_time,
            bits: expected,
            nonce: 0,
        };
        validate_header_rules(&header, Some(prev_header), true, expected).unwrap();
    }

    #[test]
    fn reorg_switches_tip_and_utxo_state() {
        let mut chain = ChainState::with_genesis(None);
        let genesis = chain.tip_hash();
        let base_time = GENESIS_TIME;

        let header_a1 = make_header(genesis, base_time + 1);
        chain.add_header(&header_a1, true).unwrap();
        let hash_a1 = header_hash(&header_a1);

        let header_a2 = make_header(hash_a1, base_time + 2);
        chain.add_header(&header_a2, true).unwrap();
        let hash_a2 = header_hash(&header_a2);

        let header_b1 = make_header(genesis, base_time + 3);
        chain.add_header(&header_b1, true).unwrap();
        let hash_b1 = header_hash(&header_b1);

        chain.work.insert(hash_a1, 1);
        chain.work.insert(hash_a2, 2);
        chain.work.insert(hash_b1, 3);
        chain.tip = hash_b1;

        let block_a1 =
            Block::new(1, genesis, 1, INITIAL_BITS, 0, vec![make_coinbase(50, 1)]).unwrap();
        let block_a2 =
            Block::new(1, hash_a1, 2, INITIAL_BITS, 0, vec![make_coinbase(50, 2)]).unwrap();
        let block_b1 =
            Block::new(1, genesis, 1, INITIAL_BITS, 0, vec![make_coinbase(50, 3)]).unwrap();

        let mut blocks = BlockStore::default();
        blocks.insert(hash_a1, block_a1.clone());
        blocks.insert(hash_a2, block_a2.clone());
        blocks.insert(hash_b1, block_b1.clone());

        let mut utxos = InMemoryUtxoSet::new();
        let mut applied = AppliedState::new(hash_a2);
        let receipts_a1 = apply_block_with_undo(&block_a1, &mut utxos, true, 50).unwrap();
        let receipts_a2 = apply_block_with_undo(&block_a2, &mut utxos, true, 50).unwrap();
        applied.undo.insert(hash_a1, receipts_a1);
        applied.undo.insert(hash_a2, receipts_a2);

        let mut evicted = Vec::new();
        reorg_to_tip(
            &mut applied,
            &chain,
            &blocks,
            &mut utxos,
            true,
            &mut evicted,
        )
        .unwrap();

        assert_eq!(applied.tip, hash_b1);

        let out_a2 = OutPoint {
            txid: block_a2.txs[0].txid_v2().unwrap(),
            vout: 0,
        };
        let out_b1 = OutPoint {
            txid: block_b1.txs[0].txid_v2().unwrap(),
            vout: 0,
        };
        assert!(utxos.get(&out_a2).is_none());
        assert!(utxos.get(&out_b1).is_some());
    }
}

#[derive(Debug, Default)]
struct BlockStore {
    map: HashMap<[u8; 32], Block>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_with_time(time: u32, bits: u32) -> BlockHeader {
        BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: [0u8; 32],
            time,
            bits,
            nonce: 0,
        }
    }

    #[test]
    fn header_time_too_old_rejected() {
        let prev = header_with_time(100, INITIAL_BITS);
        let header = header_with_time(99, INITIAL_BITS);
        let err = validate_header_rules(&header, Some(&prev), true, INITIAL_BITS).unwrap_err();
        match err {
            P2pError::InvalidBlock(msg) => assert!(msg.contains("time too old")),
            _ => panic!("expected time too old"),
        }
    }

    #[test]
    fn header_time_too_far_future_rejected() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32;
        let header = header_with_time(now.saturating_add(MAX_FUTURE_DRIFT_SECS + 1), INITIAL_BITS);
        let err = validate_header_rules(&header, None, true, INITIAL_BITS).unwrap_err();
        match err {
            P2pError::InvalidBlock(msg) => assert!(msg.contains("time too far in future")),
            _ => panic!("expected future time rejection"),
        }
    }

    #[test]
    fn header_unexpected_bits_rejected() {
        let prev = header_with_time(100, INITIAL_BITS);
        let header = header_with_time(100, INITIAL_BITS + 1);
        let err = validate_header_rules(&header, Some(&prev), true, INITIAL_BITS).unwrap_err();
        match err {
            P2pError::InvalidBlock(msg) => assert!(msg.contains("unexpected difficulty bits")),
            _ => panic!("expected unexpected difficulty bits"),
        }
    }
}

impl BlockStore {
    fn insert(&mut self, hash: [u8; 32], block: Block) {
        self.map.insert(hash, block);
    }

    fn get(&self, hash: &[u8; 32]) -> Option<Block> {
        self.map.get(hash).cloned()
    }

    fn contains(&self, hash: &[u8; 32]) -> bool {
        self.map.contains_key(hash)
    }
}
