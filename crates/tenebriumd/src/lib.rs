pub mod mempool;
pub mod p2p;
pub mod utxo_db;

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    pub fn allows(self, level: LogLevel) -> bool {
        (level as u8) <= (self as u8)
    }
}
