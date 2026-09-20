//! One RPC-free `map_events` emitting `erc20.events.v1.Events`: every log
//! signed `Transfer(address,address,uint256)` or
//! `Approval(address,address,uint256)`, classified by encoding shape only and
//! marked persisted or attempted. No contract is asserted to be an ERC-20
//! token, no balance is derived, and no wallet label is produced.
use proto::pb::erc20::events::v1 as pb;
use serde::Deserialize;
use sha2::Digest;
use substreams::{errors::Error, scalar::BigInt};
use substreams_ethereum::pb::eth::v2 as eth;

pub const PACKAGE: &str = "erc20_events";
pub const SPEC_REVISION: u32 = 1;
/// keccak256("Transfer(address,address,uint256)")
pub const TRANSFER_TOPIC: [u8; 32] = hex_literal(b"ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");
/// keccak256("Approval(address,address,uint256)")
pub const APPROVAL_TOPIC: [u8; 32] = hex_literal(b"8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925");

const fn hex_literal(s: &[u8; 64]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        let hi = hex_nibble(s[2 * i]);
        let lo = hex_nibble(s[2 * i + 1]);
        out[i] = (hi << 4) | lo;
        i += 1;
    }
    out
}
const fn hex_nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("invalid hex"),
    }
}

fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error::msg(message.to_string()))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub chain_id: u64,
    pub producer_versions: Vec<i32>,
    /// Emit rows for attempted logs in reverted frames (default true); the
    /// `persisted` flag distinguishes them either way.
    #[serde(default = "yes")]
    pub include_attempted: bool,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Debug)]
pub struct Config {
    pub params: Params,
    pub parameters_sha256: String,
}
pub fn parse(params: &str) -> Result<Config, Error> {
    let parsed: Params = serde_json::from_str(params).map_err(|e| Error::msg(format!("invalid erc20 events params: {e}")))?;
    require(parsed.chain_id > 0, "chain_id required")?;
    require(
        !parsed.producer_versions.is_empty() && parsed.producer_versions.iter().all(|v| *v > 0),
        "qualified producer versions required",
    )?;
    Ok(Config {
        params: parsed,
        parameters_sha256: hex::encode(sha2::Sha256::digest(params.as_bytes())),
    })
}

pub fn validate_block(block: &eth::Block, config: &Config) -> Result<u64, Error> {
    require(
        block.detail_level == eth::block::DetailLevel::DetaillevelExtended as i32,
        "Extended blocks required",
    )?;
    require(config.params.producer_versions.contains(&block.ver), "Extended producer version not qualified")?;
    let header = block.header.as_ref().ok_or_else(|| Error::msg("missing header"))?;
    require(
        block.hash.len() == 32 && header.parent_hash.len() == 32 && header.state_root.len() == 32,
        "invalid block identity",
    )?;
    require(header.number == block.number, "header number mismatch")?;
    let timestamp = header.timestamp.as_ref().ok_or_else(|| Error::msg("missing timestamp"))?;
    require(timestamp.seconds >= 0, "negative timestamp")?;
    for tx in &block.transaction_traces {
        require((1..=3).contains(&tx.status) && !tx.calls.is_empty(), "incomplete transaction persistence data")?;
        require(tx.hash.len() == 32, "invalid transaction hash")?;
    }
    Ok(timestamp.seconds as u64)
}

/// Decoded participants of a standard-shaped log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub shape: pb::LogShape,
    pub first: Vec<u8>,
    pub second: Vec<u8>,
    pub quantity: String,
}
fn padded_address(topic: &[u8]) -> Option<Vec<u8>> {
    (topic.len() == 32 && topic[..12] == [0; 12]).then(|| topic[12..].to_vec())
}
/// Classify a `Transfer`/`Approval`-signed log by its encoding shape.
pub fn decode(log: &eth::Log) -> Decoded {
    let nonstandard = Decoded {
        shape: pb::LogShape::Nonstandard,
        first: Vec::new(),
        second: Vec::new(),
        quantity: String::new(),
    };
    match (log.topics.len(), log.data.len()) {
        (3, 32) => match (padded_address(&log.topics[1]), padded_address(&log.topics[2])) {
            (Some(first), Some(second)) => Decoded {
                shape: pb::LogShape::Erc20,
                first,
                second,
                quantity: BigInt::from_unsigned_bytes_be(&log.data).to_string(),
            },
            _ => nonstandard,
        },
        (4, 0) => match (padded_address(&log.topics[1]), padded_address(&log.topics[2])) {
            (Some(first), Some(second)) if log.topics[3].len() == 32 => Decoded {
                shape: pb::LogShape::Erc721,
                first,
                second,
                quantity: BigInt::from_unsigned_bytes_be(&log.topics[3]).to_string(),
            },
            _ => nonstandard,
        },
        _ => nonstandard,
    }
}
fn is_zero(address: &[u8]) -> bool {
    !address.is_empty() && address.iter().all(|b| *b == 0)
}
const UINT256_MAX: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935";

struct Frame<'a> {
    scope: pb::Scope,
    tx_hash: &'a [u8],
    tx_index: u32,
    persisted_tx: bool,
}
fn push_logs(events: &mut pb::Events, config: &Config, frame: &Frame, call: &eth::Call) {
    let persisted = frame.persisted_tx && !call.state_reverted;
    if !persisted && !config.params.include_attempted {
        return;
    }
    for log in &call.logs {
        let Some(topic0) = log.topics.first() else { continue };
        let is_transfer = topic0.as_slice() == TRANSFER_TOPIC;
        let is_approval = topic0.as_slice() == APPROVAL_TOPIC;
        if !is_transfer && !is_approval {
            continue;
        }
        let d = decode(log);
        if is_transfer {
            events.transfers.push(pb::Transfer {
                chain_id: config.params.chain_id,
                scope: frame.scope as i32,
                transaction_hash: frame.tx_hash.to_vec(),
                transaction_index: frame.tx_index,
                call_index: call.index,
                log_index: log.index,
                block_index: log.block_index,
                ordinal: log.ordinal,
                token: log.address.clone(),
                shape: d.shape as i32,
                from_zero: is_zero(&d.first),
                to_zero: is_zero(&d.second),
                self_transfer: !d.first.is_empty() && d.first == d.second,
                from: d.first,
                to: d.second,
                amount: d.quantity,
                topic_count: log.topics.len() as u32,
                data_size: log.data.len() as u32,
                persisted,
            });
        } else {
            events.approvals.push(pb::Approval {
                chain_id: config.params.chain_id,
                scope: frame.scope as i32,
                transaction_hash: frame.tx_hash.to_vec(),
                transaction_index: frame.tx_index,
                call_index: call.index,
                log_index: log.index,
                block_index: log.block_index,
                ordinal: log.ordinal,
                token: log.address.clone(),
                shape: d.shape as i32,
                unlimited: d.shape == pb::LogShape::Erc20 && d.quantity == UINT256_MAX,
                zero_value: d.shape == pb::LogShape::Erc20 && d.quantity == "0",
                owner: d.first,
                spender: d.second,
                value: d.quantity,
                topic_count: log.topics.len() as u32,
                data_size: log.data.len() as u32,
                persisted,
            });
        }
    }
}

pub fn project(block: &eth::Block, config: &Config) -> Result<pb::Events, Error> {
    let timestamp = validate_block(block, config)?;
    let header = block.header.as_ref().unwrap();
    let mut events = pb::Events::default();
    for tx in &block.transaction_traces {
        let frame = Frame {
            scope: pb::Scope::Transaction,
            tx_hash: &tx.hash,
            tx_index: tx.index,
            persisted_tx: tx.status == eth::TransactionTraceStatus::Succeeded as i32,
        };
        for call in &tx.calls {
            push_logs(&mut events, config, &frame, call);
        }
    }
    for call in &block.system_calls {
        let frame = Frame {
            scope: pb::Scope::SystemCall,
            tx_hash: &[],
            tx_index: 0,
            persisted_tx: true,
        };
        push_logs(&mut events, config, &frame, call);
    }
    events
        .transfers
        .sort_by_key(|t| (t.scope, t.transaction_index, t.ordinal, t.call_index, t.log_index));
    events
        .approvals
        .sort_by_key(|a| (a.scope, a.transaction_index, a.ordinal, a.call_index, a.log_index));
    events.clocks.push(pb::BlockClock {
        chain_id: config.params.chain_id,
        number: block.number,
        hash: block.hash.clone(),
        parent_hash: header.parent_hash.clone(),
        timestamp,
        state_root: header.state_root.clone(),
        producer_version: block.ver as u32,
        spec_revision: SPEC_REVISION,
        package: PACKAGE.into(),
        package_version: env!("CARGO_PKG_VERSION").into(),
        parameters_sha256: config.parameters_sha256.clone(),
        transfer_count: events.transfers.len() as u32,
        approval_count: events.approvals.len() as u32,
    });
    Ok(events)
}

// The export exists only in the WASM build so host crates can link this
// crate next to other packages.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;
    #[substreams::handlers::map]
    fn map_events(params: String, block: eth::Block) -> Result<pb::Events, Error> {
        project(&block, &parse(&params)?)
    }
}

#[cfg(test)]
mod tests;
