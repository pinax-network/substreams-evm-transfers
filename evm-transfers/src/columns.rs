//! The block, transaction and log columns the rows carry, and the rows' keys.
//! Ported from substreams-evm `db-evm-transfers/src` (`d8a731d`): `set_clock`,
//! `log_key`, `tx_key`, `call_key` and `reward_key` (`lib.rs`),
//! `set_template_log` and its `LogAddress` trait (`logs.rs`), and the three
//! identical `set_template_*_tx` functions (`transactions.rs`).

use substreams::{errors::Error, pb::substreams::Clock};
use substreams_database_change::tables::Row;

use crate::encoding::{hex, Encoding};
use crate::{native_pb, tokens_pb, transfers_pb};

/// The clock columns (v0.3.3's `set_clock`) and the clock part of the keys.
pub(crate) struct BlockColumns {
    pub(crate) block_num: String,
    block_hash: String,
    /// `timestamp` (seconds) and `minute` (seconds / 60), when the clock has
    /// a timestamp.
    time: Option<(String, String)>,
}

impl BlockColumns {
    pub(crate) fn new(clock: &Clock) -> Self {
        Self {
            block_num: clock.number.to_string(),
            block_hash: format!("0x{}", clock.id),
            time: clock.timestamp.as_option().map(|t| (t.seconds.to_string(), (t.seconds / 60).to_string())),
        }
    }

    /// `minute`, `timestamp` and `block_num`, which every key but the `blocks`
    /// key starts with.
    ///
    /// # Errors
    ///
    /// A clock without a timestamp, where v0.3.3 panicked
    /// (`clock.timestamp is required`).
    fn clock_key(&self) -> Result<[(&'static str, String); 3], Error> {
        let (timestamp, minute) = self.time.clone().ok_or_else(|| Error::msg("clock.timestamp is required"))?;
        Ok([("minute", minute), ("timestamp", timestamp), ("block_num", self.block_num.clone())])
    }

    /// The key of a log row (v0.3.3's `log_key`). `tx_index` and `log_index`
    /// are positions in the map's `Events.transactions` and
    /// `Transaction.logs`.
    pub(crate) fn log_key(&self, tx_index: usize, log_index: usize) -> Result<[(&'static str, String); 6], Error> {
        let [minute, timestamp, block_num] = self.clock_key()?;
        Ok([
            minute,
            timestamp,
            block_num,
            ("tx_index", tx_index.to_string()),
            ("log_index", log_index.to_string()),
            ("block_hash", self.block_hash.clone()),
        ])
    }

    /// The key of a `transactions` row (v0.3.3's `tx_key`).
    pub(crate) fn tx_key(&self, tx_index: usize) -> Result<[(&'static str, String); 5], Error> {
        let [minute, timestamp, block_num] = self.clock_key()?;
        Ok([
            minute,
            timestamp,
            block_num,
            ("tx_index", tx_index.to_string()),
            ("block_hash", self.block_hash.clone()),
        ])
    }

    /// The key of a `calls` row (v0.3.3's `call_key`).
    pub(crate) fn call_key(&self, tx_index: usize, call_index: usize) -> Result<[(&'static str, String); 6], Error> {
        let [minute, timestamp, block_num] = self.clock_key()?;
        Ok([
            minute,
            timestamp,
            block_num,
            ("tx_index", tx_index.to_string()),
            ("call_index", call_index.to_string()),
            ("block_hash", self.block_hash.clone()),
        ])
    }

    /// The key of a `block_rewards` row (v0.3.3's `reward_key`).
    pub(crate) fn reward_key(&self, index: usize) -> Result<[(&'static str, String); 5], Error> {
        let [minute, timestamp, block_num] = self.clock_key()?;
        Ok([
            minute,
            timestamp,
            block_num,
            ("index", index.to_string()),
            ("block_hash", self.block_hash.clone()),
        ])
    }

    /// `block_num`, `block_hash` (`0x` + clock id), and `timestamp` (seconds)
    /// and `minute` (seconds / 60) when the clock has a timestamp.
    pub(crate) fn set(&self, row: &mut Row) {
        row.set("block_num", &self.block_num).set("block_hash", &self.block_hash);
        if let Some((timestamp, minute)) = &self.time {
            row.set("timestamp", timestamp).set("minute", minute);
        }
    }
}

/// The transaction fields the `tx_*` columns read, alike in the three maps'
/// `Transaction`s.
pub(crate) trait TransactionFields {
    fn hash(&self) -> &[u8];
    fn from(&self) -> &[u8];
    fn to(&self) -> Option<&[u8]>;
    fn nonce(&self) -> u64;
    fn gas_price(&self) -> &str;
    fn gas_limit(&self) -> u64;
    fn gas_used(&self) -> u64;
    fn value(&self) -> &str;
}

macro_rules! impl_transaction_fields {
    ($($transaction:ty),*) => {
        $(
            impl TransactionFields for $transaction {
                fn hash(&self) -> &[u8] {
                    &self.hash
                }
                fn from(&self) -> &[u8] {
                    &self.from
                }
                fn to(&self) -> Option<&[u8]> {
                    self.to.as_deref()
                }
                fn nonce(&self) -> u64 {
                    self.nonce
                }
                fn gas_price(&self) -> &str {
                    &self.gas_price
                }
                fn gas_limit(&self) -> u64 {
                    self.gas_limit
                }
                fn gas_used(&self) -> u64 {
                    self.gas_used
                }
                fn value(&self) -> &str {
                    &self.value
                }
            }
        )*
    };
}

impl_transaction_fields!(transfers_pb::Transaction, tokens_pb::Transaction, native_pb::Transaction);

/// The `tx_*` columns (v0.3.3's `set_template_*_tx`). `tx_index` is the
/// position in the map's `Events.transactions`; `tx_hash` is hex in either
/// encoding, and a contract creation has an empty `tx_to`.
pub(crate) fn set_transaction(encoding: Encoding, tx: &impl TransactionFields, tx_index: usize, row: &mut Row) {
    let tx_to = tx.to().map(|to| encoding.address(to)).unwrap_or_default();
    row.set("tx_index", tx_index as u32)
        .set("tx_hash", hex(tx.hash()))
        .set("tx_from", encoding.address(tx.from()))
        .set("tx_to", tx_to)
        .set("tx_nonce", tx.nonce())
        .set("tx_gas_price", tx.gas_price())
        .set("tx_gas_limit", tx.gas_limit())
        .set("tx_gas_used", tx.gas_used())
        .set("tx_value", tx.value());
}

/// The log fields the `log_*` columns read (v0.3.3's `LogAddress`), alike in
/// the two ERC-20 maps' `Log`s.
pub(crate) trait LogFields {
    fn address(&self) -> &[u8];
    fn ordinal(&self) -> u64;
    fn topics(&self) -> &[Vec<u8>];
    fn data(&self) -> &[u8];
}

macro_rules! impl_log_fields {
    ($($log:ty),*) => {
        $(
            impl LogFields for $log {
                fn address(&self) -> &[u8] {
                    &self.address
                }
                fn ordinal(&self) -> u64 {
                    self.ordinal
                }
                fn topics(&self) -> &[Vec<u8>] {
                    &self.topics
                }
                fn data(&self) -> &[u8] {
                    &self.data
                }
            }
        )*
    };
}

impl_log_fields!(transfers_pb::Log, tokens_pb::Log);

/// The `log_*` columns (v0.3.3's `set_template_log`). `log_index` is the
/// position in the map's `Transaction.logs`; topics (comma-separated) and
/// data are hex in either encoding. The call metadata is not written.
pub(crate) fn set_log(encoding: Encoding, log: &impl LogFields, log_index: usize, row: &mut Row) {
    let topics: Vec<String> = log.topics().iter().map(|topic| hex(topic)).collect();
    row.set("log_index", log_index as u32)
        .set("log_address", encoding.address(log.address()))
        .set("log_ordinal", log.ordinal())
        .set("log_topics", topics.join(","))
        .set("log_data", hex(log.data()));
}
