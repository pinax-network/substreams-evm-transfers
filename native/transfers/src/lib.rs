//! Native value transfers of a block's successful transactions and their
//! calls, and its block rewards, as `native.transfers.v1.Events`.
//!
//! Ported from substreams-evm `native-transfers` at `f576af1` (unchanged since
//! `aaae9dd`), the source of the committed `evm-native-transfers-v0.2.0.spkg`
//! that production `evm-transfers` v0.3.3 imports; see the package README.
//! That version predates the withdrawal, selfdestruct, genesis and DAO rows of
//! later substreams-evm versions, and so does this port.
//!
//! [`project`] is the whole module: a host program that decodes blocks itself
//! gets the same `Events` without the Substreams runtime. The WASM-only
//! `map_events` handler wraps it.

/// The `native.transfers.v1` messages [`project`] returns.
pub use proto::pb::native::transfers::v1 as pb;
use substreams::scalar::BigInt;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::pb::eth::v2::balance_change::Reason;

/// The block rewards and native value transfers of `block`.
///
/// - **Block rewards**: one per block-level balance change whose reason is a
///   mining, transaction-fee, blob-fee or uncle reward or an Optimism
///   `INCREASE_MINT`, and whose balance grew; the value is the increase.
/// - **Transactions**: each successful transaction, in block order, that
///   sends value or has a call that does; with the calls that move value:
///   not reverted or failed, value above zero, depth above zero (the root call
///   repeats the transaction) and type `CALL`.
///
/// It reads no receipt and never fails.
pub fn project(block: &eth::Block) -> pb::Events {
    let mut events = pb::Events::default();

    // EXTENDED
    // balance changes at block level
    for balance_change in &block.balance_changes {
        // Block Rewards
        if let Some(value) = block_reward_amount(balance_change) {
            events.block_rewards.push(pb::BlockReward {
                miner: balance_change.address.to_vec(),
                value: value.to_string(),
                // Both enums number their reasons alike, and only reward
                // reasons get here.
                reason: balance_change.reason.to_i32().into(),
            });
        }
    }

    // iterate over successful transactions
    for trx in block.transactions() {
        let value = unsigned(trx.value.as_option());
        let to = if trx.to.is_empty() { None } else { Some(trx.to.to_vec()) };

        let mut transaction = pb::Transaction {
            from: trx.from.to_vec(),
            to,
            hash: trx.hash.to_vec(),
            nonce: trx.nonce,
            gas_price: gas_price(trx.gas_price.as_option()).to_string(),
            gas_limit: trx.gas_limit,
            // The transaction's own gas, unlike the ERC-20 maps' cumulative
            // receipt gas.
            gas_used: trx.gas_used,
            value: value.to_string(),
            calls: vec![],
        };

        // EXTENDED
        // find all value transfers from successful calls
        for call in &trx.calls {
            if is_failed_call(call) {
                continue;
            }

            // ignore calls with no value
            let value = unsigned(call.value.as_option());
            if value <= BigInt::zero() {
                continue;
            }

            // ignore top-level calls
            if call.depth == 0 {
                continue;
            }

            // A DELEGATECALL executes another contract's code with the
            // calling contract's storage and balance: no value moves. Only
            // `CALL` calls are considered transfers.
            if call.call_type != eth::CallType::Call {
                continue;
            }
            transaction.calls.push(pb::Call {
                index: call.index,
                begin_ordinal: call.begin_ordinal,
                end_ordinal: call.end_ordinal,
                caller: call.caller.to_vec(),
                address: call.address.to_vec(),
                value: value.to_string(),
                gas_consumed: call.gas_consumed,
                gas_limit: call.gas_limit,
                depth: call.depth,
                parent_index: call.parent_index,
            });
        }
        // skip transactions with no value and no calls with value
        if value == BigInt::zero() && transaction.calls.is_empty() {
            continue;
        }
        events.transactions.push(transaction);
    }
    events
}

/// A Firehose integer as an unsigned integer; unset is zero.
fn unsigned(value: Option<&eth::BigInt>) -> BigInt {
    value.map_or_else(BigInt::zero, |value| BigInt::from_unsigned_bytes_be(&value.bytes))
}

/// The gas price, or zero when it is unset or wider than 20 bytes
/// (pinax-network/substreams-evm-tokens#34).
fn gas_price(gas_price: Option<&eth::BigInt>) -> BigInt {
    match gas_price {
        Some(price) if price.bytes.len() <= 20 => BigInt::from_unsigned_bytes_be(&price.bytes),
        _ => BigInt::zero(),
    }
}

fn is_failed_call(call: &eth::Call) -> bool {
    call.state_reverted || call.status_failed || call.status_reverted
}

/// The balance increase of a reward balance change, if it grew.
fn block_reward_amount(balance_change: &eth::BalanceChange) -> Option<BigInt> {
    if !is_block_reward_reason(balance_change.reason.as_known()) {
        return None;
    }
    let value = unsigned(balance_change.new_value.as_option()) - unsigned(balance_change.old_value.as_option());
    if value <= BigInt::zero() {
        return None;
    }
    Some(value)
}

/// A reason outside the enum is not a reward.
fn is_block_reward_reason(reason: Option<Reason>) -> bool {
    matches!(
        reason,
        Some(
            Reason::RewardMineBlock
                | Reason::RewardTransactionFee
                | Reason::RewardBlobFee
                | Reason::RewardMineUncle
                // Optimism sequencer "block rewards"
                | Reason::IncreaseMint
        )
    )
}

// The export exists only in the WASM build so host crates can link this
// crate next to other packages.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;
    use substreams::errors::Error;

    #[substreams::handlers::map]
    fn map_events(block: eth::Block) -> Result<pb::Events, Error> {
        Ok(project(&block))
    }
}

#[cfg(test)]
mod tests;
