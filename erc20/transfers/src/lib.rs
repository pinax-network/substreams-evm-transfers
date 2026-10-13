//! ERC-20 `Transfer` and `Approval` logs of a block's successful
//! transactions, each with its transaction, as `erc20.transfers.v1.Events`.
//!
//! Ported from substreams-evm `erc20-transfers` at tag `evm-transfers-v0.3.3`
//! (`d8a731d`), the source of the `erc20_transfers:map_events` module that the
//! production `evm-transfers` v0.3.3 runs; see the package README for what
//! changed in the port and the v0.3.3 behaviour it keeps on purpose.
//!
//! [`project`] is the whole module: a host program that decodes blocks itself
//! gets the same `Events` without the Substreams runtime. The WASM-only
//! `map_events` handler wraps it.

use common::create::{CreateLog, CreateTransaction};
/// The `erc20.transfers.v1` messages [`project`] returns.
pub use proto::pb::erc20::transfers::v1 as pb;
use substreams::errors::Error;
use substreams_abis::standard::erc20::events;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::Event;

/// Decodes the ERC-20 `Transfer` and `Approval` logs of `block`'s successful
/// transactions.
///
/// Each successful transaction with at least one such log becomes one
/// `Transaction`, in block order, holding its logs in the order
/// [`common::logs_with_calls`] gives them: the persisted logs of its calls by
/// ordinal, each with its call, or, for a transaction without call traces
/// (detail level BASE), its receipt logs without a call.
///
/// A log matches when its topic 0 is the event's signature hash, it has three
/// topics and exactly 32 bytes of data; addresses are the low 20 bytes of
/// their topic, whatever the padding.
///
/// # Errors
///
/// A successful transaction without a receipt, where v0.3.3 panicked.
pub fn project(block: &eth::Block) -> Result<pb::Events, Error> {
    common::check_receipts(block)?;
    let mut events = pb::Events::default();

    for trx in block.transactions() {
        let mut transaction = pb::Transaction::create_transaction(trx);

        for (log, call) in common::logs_with_calls(trx) {
            // ERC-20 Transfer event
            if let Some(event) = events::Transfer::match_and_decode(log) {
                let event = pb::log::Log::from(pb::Transfer {
                    from: event.from,
                    to: event.to,
                    amount: event.value.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }

            // ERC-20 Approval event
            if let Some(event) = events::Approval::match_and_decode(log) {
                let event = pb::log::Log::from(pb::Approval {
                    owner: event.owner,
                    spender: event.spender,
                    value: event.value.to_string(),
                });
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }
        }

        // Only include transactions with logs
        if !transaction.logs.is_empty() {
            events.transactions.push(transaction);
        }
    }
    Ok(events)
}

// The export exists only in the WASM build so host crates can link this
// crate next to other packages.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;

    #[substreams::handlers::map]
    fn map_events(block: eth::Block) -> Result<pb::Events, Error> {
        project(&block)
    }
}

#[cfg(test)]
mod tests;
