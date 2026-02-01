use clap::{Parser, Subcommand};
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use tenebrium_core::{
    address_from_pubkey_hex, generate_keypair, sign_message_hex, verify_message_hex,
    wallet_file_from_secret, wallet_file_reencrypt, wallet_keypair_from_file, WalletError,
    WalletFile, WalletKeypair,
};
use tenebrium_utxo::{tx_sighash_v2, OutPoint, Transaction, TxIn, TxOut, UtxoError};

#[derive(Parser)]
#[command(name = "tenebrium-cli")]
#[command(version = "0.1.0")]
#[command(about = "Tenebrium CLI tool")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Wallet {
        #[command(subcommand)]
        command: WalletCommand,
    },
    Tx {
        #[command(subcommand)]
        command: TxCommand,
    },
}

#[derive(Subcommand)]
enum WalletCommand {
    /// Generate a new keypair
    Keygen {
        /// Write JSON output to file
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Derive address from public or secret key
    Address {
        /// Public key hex (32 bytes)
        #[arg(long)]
        pubkey: Option<String>,
        /// Secret key hex (32 bytes)
        #[arg(long)]
        secret: Option<String>,
    },
    /// Sign a message
    Sign {
        /// Secret key hex (32 bytes)
        #[arg(long)]
        secret: String,
        /// Message string to sign
        #[arg(long)]
        message: String,
    },
    /// Verify a message signature
    Verify {
        /// Public key hex (32 bytes)
        #[arg(long)]
        pubkey: String,
        /// Message string to verify
        #[arg(long)]
        message: String,
        /// Signature hex (64 bytes)
        #[arg(long)]
        signature: String,
    },
    /// Save encrypted wallet file
    Save {
        /// Secret key hex (32 bytes)
        #[arg(long)]
        secret: String,
        /// Passphrase for encryption
        #[arg(long)]
        passphrase: Option<String>,
        /// Output path (JSON)
        #[arg(long)]
        out: PathBuf,
    },
    /// Load and decrypt wallet file
    Load {
        /// Input path (JSON)
        #[arg(long)]
        input: PathBuf,
        /// Passphrase for decryption
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Re-encrypt wallet file (backup with new passphrase)
    Backup {
        /// Input wallet path (JSON)
        #[arg(long)]
        input: PathBuf,
        /// Output wallet path (JSON)
        #[arg(long)]
        out: PathBuf,
        /// Current passphrase
        #[arg(long)]
        passphrase: Option<String>,
        /// New passphrase (prompt if omitted)
        #[arg(long)]
        new_passphrase: Option<String>,
    },
}

#[derive(Subcommand)]
enum TxCommand {
    /// Create a transaction JSON (optionally signed)
    Create {
        /// Input tx template JSON
        #[arg(long)]
        input: PathBuf,
        /// Output path (JSON). If omitted, prints to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Sign all inputs with this secret key hex
        #[arg(long)]
        sign_secret: Option<String>,
    },
    /// Sign an existing transaction JSON (overwrites script_sig)
    Sign {
        /// Input tx JSON
        #[arg(long)]
        input: PathBuf,
        /// Output path (JSON). If omitted, prints to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Secret key hex
        #[arg(long)]
        secret: String,
    },
    /// Print txid (v2) and sighash
    Info {
        /// Input tx JSON
        #[arg(long)]
        input: PathBuf,
    },
    /// Build a transaction from UTXO JSONL
    Build {
        /// UTXO JSONL input (each line is {outpoint, txout})
        #[arg(long)]
        utxo: PathBuf,
        /// Recipient script_pubkey hex
        #[arg(long)]
        to_script: String,
        /// Amount to send
        #[arg(long)]
        amount: u64,
        /// Change script_pubkey hex
        #[arg(long)]
        change_script: String,
        /// Fee to pay (absolute)
        #[arg(long)]
        fee: Option<u64>,
        /// Fee rate (satoshis per byte). If set, fee is auto-estimated.
        #[arg(long)]
        fee_rate: Option<u64>,
        /// Coin selection strategy
        #[arg(long, value_enum, default_value_t = CoinSelect::LargestFirst)]
        strategy: CoinSelect,
        /// Output path (JSON). If omitted, prints to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Sign all inputs with this secret key hex
        #[arg(long)]
        sign_secret: Option<String>,
    },
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("wallet error: {0}")]
    Wallet(#[from] WalletError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("UTXO error: {0}")]
    Utxo(#[from] UtxoError),
    #[error("hex error: {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
}

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
enum CoinSelect {
    LargestFirst,
    SmallestFirst,
    Random,
    BestFit,
}

#[derive(Serialize)]
struct KeygenOutput {
    secret_key_hex: String,
    public_key_hex: String,
    address: String,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), CliError> {
    let args = Args::parse();
    match args.command {
        Command::Wallet { command } => run_wallet(command),
        Command::Tx { command } => run_tx(command),
    }
}

fn run_wallet(command: WalletCommand) -> Result<(), CliError> {
    match command {
        WalletCommand::Keygen { out } => {
            let kp = generate_keypair();
            let output = keygen_output(&kp)?;
            let json = serde_json::to_string_pretty(&output)?;
            match out {
                Some(path) => {
                    std::fs::write(path, json)?;
                }
                None => {
                    println!("{json}");
                }
            }
            Ok(())
        }
        WalletCommand::Address { pubkey, secret } => {
            let address = match (pubkey, secret) {
                (Some(pubkey), None) => address_from_pubkey_hex(&pubkey)?,
                (None, Some(secret)) => {
                    let kp = WalletKeypair::from_secret_hex(&secret)?;
                    kp.address()?
                }
                _ => {
                    return Err(CliError::InvalidArgs(
                        "provide exactly one of --pubkey or --secret".to_string(),
                    ))
                }
            };
            println!("{address}");
            Ok(())
        }
        WalletCommand::Sign { secret, message } => {
            let sig = sign_message_hex(&secret, message.as_bytes())?;
            println!("{sig}");
            Ok(())
        }
        WalletCommand::Verify {
            pubkey,
            message,
            signature,
        } => {
            let ok = verify_message_hex(&pubkey, message.as_bytes(), &signature)?;
            println!("{ok}");
            Ok(())
        }
        WalletCommand::Save {
            secret,
            passphrase,
            out,
        } => {
            let passphrase = resolve_passphrase(passphrase)?;
            let wallet = wallet_file_from_secret(&secret, &passphrase)?;
            let json = serde_json::to_string_pretty(&wallet)?;
            std::fs::write(out, json)?;
            Ok(())
        }
        WalletCommand::Load { input, passphrase } => {
            let data = std::fs::read_to_string(input)?;
            let wallet: WalletFile = serde_json::from_str(&data)?;
            let passphrase = resolve_passphrase(passphrase)?;
            let kp = wallet_keypair_from_file(&wallet, &passphrase)?;
            let output = keygen_output(&kp)?;
            let json = serde_json::to_string_pretty(&output)?;
            println!("{json}");
            Ok(())
        }
        WalletCommand::Backup {
            input,
            out,
            passphrase,
            new_passphrase,
        } => {
            let data = std::fs::read_to_string(input)?;
            let wallet: WalletFile = serde_json::from_str(&data)?;
            let passphrase = resolve_passphrase(passphrase)?;
            let new_passphrase = resolve_new_passphrase(new_passphrase)?;
            let new_wallet = wallet_file_reencrypt(&wallet, &passphrase, &new_passphrase)?;
            let json = serde_json::to_string_pretty(&new_wallet)?;
            std::fs::write(out, json)?;
            Ok(())
        }
    }
}

fn run_tx(command: TxCommand) -> Result<(), CliError> {
    match command {
        TxCommand::Create {
            input,
            out,
            sign_secret,
        } => {
            let data = std::fs::read_to_string(input)?;
            let tx_file: TxFile = serde_json::from_str(data.trim_start_matches('\u{feff}'))?;
            let mut tx = tx_file.to_transaction()?;
            if let Some(secret) = sign_secret {
                sign_all_inputs(&mut tx, &secret)?;
            }
            let out_file = TxFile::from_transaction(&tx);
            write_json(out_file, out)?;
            Ok(())
        }
        TxCommand::Sign { input, out, secret } => {
            let data = std::fs::read_to_string(input)?;
            let tx_file: TxFile = serde_json::from_str(data.trim_start_matches('\u{feff}'))?;
            let mut tx = tx_file.to_transaction()?;
            sign_all_inputs(&mut tx, &secret)?;
            let out_file = TxFile::from_transaction(&tx);
            write_json(out_file, out)?;
            Ok(())
        }
        TxCommand::Info { input } => {
            let data = std::fs::read_to_string(input)?;
            let tx_file: TxFile = serde_json::from_str(data.trim_start_matches('\u{feff}'))?;
            let tx = tx_file.to_transaction()?;
            let txid = tx.txid_v2()?;
            let sighash = tx_sighash_v2(&tx)?;
            println!("txid_v2={}", hex::encode(txid));
            println!("sighash_v2={}", hex::encode(sighash));
            Ok(())
        }
        TxCommand::Build {
            utxo,
            to_script,
            amount,
            change_script,
            fee,
            fee_rate,
            strategy,
            out,
            sign_secret,
        } => {
            let utxos = read_utxo_jsonl(&utxo)?;
            let to_script = hex::decode(&to_script)?;
            let change_script = hex::decode(&change_script)?;
            let fee = resolve_fee(fee, fee_rate)?;
            let script_sig_len = if sign_secret.is_some() { 96 } else { 0 };
            let (selected, input_sum, fee) = select_utxos(
                &utxos,
                amount,
                fee,
                fee_rate,
                &to_script,
                &change_script,
                script_sig_len,
                strategy,
            )?;

            let mut vin = Vec::with_capacity(selected.len());
            for entry in &selected {
                vin.push(TxIn {
                    prevout: entry.outpoint.clone(),
                    script_sig: Vec::new(),
                    sequence: 0xffff_ffff,
                });
            }

            let mut vout = Vec::new();
            vout.push(TxOut {
                value: amount,
                script_pubkey: to_script,
            });
            let change_value = input_sum - amount - fee;
            if change_value > 0 {
                vout.push(TxOut {
                    value: change_value,
                    script_pubkey: change_script,
                });
            }

            let mut tx = Transaction {
                version: 1,
                vin,
                vout,
                lock_time: 0,
            };

            if let Some(secret) = sign_secret {
                sign_all_inputs(&mut tx, &secret)?;
            }

            let out_file = TxFile::from_transaction(&tx);
            write_json(out_file, out)?;
            Ok(())
        }
    }
}

fn keygen_output(kp: &WalletKeypair) -> Result<KeygenOutput, WalletError> {
    Ok(KeygenOutput {
        secret_key_hex: kp.secret_key_hex(),
        public_key_hex: kp.public_key_hex(),
        address: kp.address()?,
    })
}

fn write_json<T: Serialize>(value: T, out: Option<PathBuf>) -> Result<(), CliError> {
    let json = serde_json::to_string_pretty(&value)?;
    match out {
        Some(path) => {
            std::fs::write(path, json)?;
        }
        None => {
            println!("{json}");
        }
    }
    Ok(())
}

fn sign_all_inputs(tx: &mut Transaction, secret_hex: &str) -> Result<(), CliError> {
    let sighash = tx_sighash_v2(tx)?;
    let sig_hex = sign_message_hex(secret_hex, &sighash)?;
    let pubkey_hex = WalletKeypair::from_secret_hex(secret_hex)?.public_key_hex();
    let sig_bytes = hex::decode(sig_hex)?;
    let pub_bytes = hex::decode(pubkey_hex)?;
    let mut script = Vec::with_capacity(sig_bytes.len() + pub_bytes.len());
    script.extend(sig_bytes);
    script.extend(pub_bytes);
    for vin in &mut tx.vin {
        vin.script_sig = script.clone();
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct TxFile {
    version: i32,
    lock_time: u32,
    vin: Vec<TxInFile>,
    vout: Vec<TxOutFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TxInFile {
    prevout: PrevoutFile,
    sequence: u32,
    #[serde(default)]
    script_sig_hex: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PrevoutFile {
    txid_hex: String,
    vout: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct TxOutFile {
    value: u64,
    script_pubkey_hex: String,
}

impl TxFile {
    fn to_transaction(&self) -> Result<Transaction, CliError> {
        let mut vin: Vec<TxIn> = Vec::with_capacity(self.vin.len());
        for input in &self.vin {
            let txid = decode_32(&input.prevout.txid_hex)?;
            let script_sig = match &input.script_sig_hex {
                Some(hex_str) => hex::decode(hex_str)?,
                None => Vec::new(),
            };
            vin.push(TxIn {
                prevout: OutPoint {
                    txid,
                    vout: input.prevout.vout,
                },
                script_sig,
                sequence: input.sequence,
            });
        }

        let mut vout: Vec<TxOut> = Vec::with_capacity(self.vout.len());
        for output in &self.vout {
            let script_pubkey = hex::decode(&output.script_pubkey_hex)?;
            vout.push(TxOut {
                value: output.value,
                script_pubkey,
            });
        }

        Ok(Transaction {
            version: self.version,
            vin,
            vout,
            lock_time: self.lock_time,
        })
    }

    fn from_transaction(tx: &Transaction) -> Self {
        let vin = tx
            .vin
            .iter()
            .map(|input| TxInFile {
                prevout: PrevoutFile {
                    txid_hex: hex::encode(input.prevout.txid),
                    vout: input.prevout.vout,
                },
                sequence: input.sequence,
                script_sig_hex: Some(hex::encode(&input.script_sig)),
            })
            .collect();

        let vout = tx
            .vout
            .iter()
            .map(|output| TxOutFile {
                value: output.value,
                script_pubkey_hex: hex::encode(&output.script_pubkey),
            })
            .collect();

        Self {
            version: tx.version,
            lock_time: tx.lock_time,
            vin,
            vout,
        }
    }
}

fn decode_32(hex_str: &str) -> Result<[u8; 32], CliError> {
    let bytes = hex::decode(hex_str)?;
    if bytes.len() != 32 {
        return Err(CliError::InvalidArgs(format!(
            "expected 32-byte hex, got {} bytes",
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UtxoEntry {
    outpoint: OutPoint,
    txout: TxOut,
}

fn read_utxo_jsonl(path: &PathBuf) -> Result<Vec<UtxoEntry>, CliError> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: UtxoEntry = serde_json::from_str(line)?;
        out.push(entry);
    }
    Ok(out)
}

fn select_utxos(
    utxos: &[UtxoEntry],
    amount: u64,
    fee: u64,
    fee_rate: Option<u64>,
    to_script: &[u8],
    change_script: &[u8],
    script_sig_len: usize,
    strategy: CoinSelect,
) -> Result<(Vec<UtxoEntry>, u64, u64), CliError> {
    let mut pool: Vec<UtxoEntry> = utxos.to_vec();
    if let CoinSelect::BestFit = strategy {
        if let Some(rate) = fee_rate {
            if let Some((entry, calc_fee)) = best_fit_single(
                &pool,
                amount,
                rate,
                script_sig_len,
                to_script.len(),
                change_script.len(),
            ) {
                return Ok((vec![entry.clone()], entry.txout.value, calc_fee));
            }
        } else {
            if let Some(entry) = pool
                .iter()
                .filter(|e| e.txout.value >= amount + fee)
                .min_by_key(|e| e.txout.value - (amount + fee))
            {
                return Ok((vec![entry.clone()], entry.txout.value, fee));
            }
        }
    }
    match strategy {
        CoinSelect::LargestFirst => {
            pool.sort_by(|a, b| b.txout.value.cmp(&a.txout.value));
        }
        CoinSelect::SmallestFirst => {
            pool.sort_by(|a, b| a.txout.value.cmp(&b.txout.value));
        }
        CoinSelect::Random => {
            pool.shuffle(&mut thread_rng());
        }
        CoinSelect::BestFit => {
            pool.sort_by(|a, b| b.txout.value.cmp(&a.txout.value));
        }
    }

    let mut selected = Vec::new();
    let mut sum = 0u64;
    for entry in pool {
        selected.push(entry.clone());
        sum = sum
            .checked_add(entry.txout.value)
            .ok_or_else(|| CliError::InvalidArgs("input sum overflow".to_string()))?;

        let calc_fee = match fee_rate {
            Some(rate) => {
                let fee_with_change = rate.saturating_mul(estimate_tx_size(
                    selected.len(),
                    true,
                    script_sig_len,
                    to_script.len(),
                    change_script.len(),
                ) as u64);
                let target_with_change = amount
                    .checked_add(fee_with_change)
                    .ok_or_else(|| CliError::InvalidArgs("amount+fee overflow".to_string()))?;
                if sum >= target_with_change {
                    let change_value = sum - amount - fee_with_change;
                    if change_value > 0 {
                        fee_with_change
                    } else {
                        let fee_no_change = rate.saturating_mul(estimate_tx_size(
                            selected.len(),
                            false,
                            script_sig_len,
                            to_script.len(),
                            change_script.len(),
                        ) as u64);
                        fee_no_change
                    }
                } else {
                    fee_with_change
                }
            }
            None => fee,
        };
        let target = amount
            .checked_add(calc_fee)
            .ok_or_else(|| CliError::InvalidArgs("amount+fee overflow".to_string()))?;
        if sum >= target {
            return Ok((selected, sum, calc_fee));
        }
    }

    Err(CliError::InvalidArgs("insufficient funds".to_string()))
}

fn best_fit_single(
    pool: &[UtxoEntry],
    amount: u64,
    rate: u64,
    script_sig_len: usize,
    to_script_len: usize,
    change_script_len: usize,
) -> Option<(UtxoEntry, u64)> {
    let mut best: Option<(UtxoEntry, u64, u64)> = None; // (entry, fee, excess)
    for entry in pool {
        let sum = entry.txout.value;
        let fee_with_change = rate.saturating_mul(estimate_tx_size(
            1,
            true,
            script_sig_len,
            to_script_len,
            change_script_len,
        ) as u64);
        let fee_no_change = rate.saturating_mul(estimate_tx_size(
            1,
            false,
            script_sig_len,
            to_script_len,
            change_script_len,
        ) as u64);

        let target_with_change = amount.saturating_add(fee_with_change);
        let target_no_change = amount.saturating_add(fee_no_change);

        let (fee, target) = if sum >= target_with_change {
            let change_value = sum - target_with_change;
            if change_value > 0 {
                (fee_with_change, target_with_change)
            } else {
                (fee_no_change, target_no_change)
            }
        } else if sum >= target_no_change {
            (fee_no_change, target_no_change)
        } else {
            continue;
        };

        let excess = sum - target;
        match &best {
            Some((_, _, best_excess)) if excess >= *best_excess => {}
            _ => {
                best = Some((entry.clone(), fee, excess));
            }
        }
    }
    best.map(|(entry, fee, _)| (entry, fee))
}

fn estimate_tx_size(
    vin_count: usize,
    has_change: bool,
    script_sig_len: usize,
    to_script_len: usize,
    change_script_len: usize,
) -> usize {
    let header = 4 + 8 + 8 + 4;
    let vin_size = 32 + 4 + 8 + script_sig_len + 4;
    let to_out = 8 + 8 + to_script_len;
    let change_out = if has_change {
        8 + 8 + change_script_len
    } else {
        0
    };
    header + vin_size * vin_count + to_out + change_out
}

fn resolve_fee(fee: Option<u64>, fee_rate: Option<u64>) -> Result<u64, CliError> {
    match (fee, fee_rate) {
        (Some(_), Some(_)) => Err(CliError::InvalidArgs(
            "use only one of --fee or --fee-rate".to_string(),
        )),
        (Some(value), None) => Ok(value),
        (None, Some(_)) => Ok(0),
        (None, None) => Err(CliError::InvalidArgs(
            "provide --fee or --fee-rate".to_string(),
        )),
    }
}

fn resolve_passphrase(passphrase: Option<String>) -> Result<String, CliError> {
    match passphrase {
        Some(value) => Ok(value),
        None => {
            let value = rpassword::prompt_password("Passphrase: ")?;
            if value.is_empty() {
                return Err(CliError::InvalidArgs("passphrase is empty".to_string()));
            }
            Ok(value)
        }
    }
}

fn resolve_new_passphrase(passphrase: Option<String>) -> Result<String, CliError> {
    match passphrase {
        Some(value) => Ok(value),
        None => {
            let first = rpassword::prompt_password("New passphrase: ")?;
            if first.is_empty() {
                return Err(CliError::InvalidArgs("passphrase is empty".to_string()));
            }
            let second = rpassword::prompt_password("Confirm passphrase: ")?;
            if first != second {
                return Err(CliError::InvalidArgs("passphrase mismatch".to_string()));
            }
            Ok(first)
        }
    }
}
