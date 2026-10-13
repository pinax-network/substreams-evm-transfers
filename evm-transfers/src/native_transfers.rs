//! `block_rewards`, `transactions` and `calls`. Ported from substreams-evm
//! `db-evm-transfers/src/native_transfers.rs` (`d8a731d`).
//!
//! v0.3.3's `db_out` also wrote `withdrawals`, `selfdestructs`,
//! `genesis_balances` and `dao_transfers` rows from fields of a later
//! `native.transfers.v1` than the one its native map emits (`aaae9dd`), so
//! production never wrote them; those messages are not in this repository's
//! proto, and their tables stay empty.

use buffa::Enumeration;
use substreams::errors::Error;
use substreams_database_change::tables::Tables;

use crate::columns::{set_transaction, BlockColumns};
use crate::{native_pb as pb, Encoding};

/// `calls.call_type` as production writes it. v0.3.3's `db_out` read
/// `Call.call_type` (field 11), which the native map it imports never sets:
/// every call row gets the name of 0.
const CALL_TYPE: &str = "CALL_TYPE_UNSPECIFIED";

pub(crate) fn process_events(encoding: Encoding, tables: &mut Tables, block: &BlockColumns, events: &pb::Events) -> Result<(), Error> {
    // Block Rewards
    for (index, reward) in events.block_rewards.iter().enumerate() {
        let row = tables.create_row("block_rewards", block.reward_key(index)?);
        block.set(row);
        row.set("index", index as u64)
            .set("miner", encoding.address(&reward.miner))
            .set("value", &reward.value)
            // The proto name; a value outside the enum is REASON_UNKNOWN, as
            // prost's getter read it.
            .set("reason", reward.reason.as_known().unwrap_or_default().proto_name());
    }

    for (tx_index, tx) in events.transactions.iter().enumerate() {
        // Transactions
        let row = tables.create_row("transactions", block.tx_key(tx_index)?);
        block.set(row);
        set_transaction(encoding, tx, tx_index, row);

        // Calls
        for (call_index, call) in tx.calls.iter().enumerate() {
            let row = tables.create_row("calls", block.call_key(tx_index, call_index)?);
            block.set(row);
            set_transaction(encoding, tx, tx_index, row);
            row.set("call_index", call_index as u64)
                .set("call_begin_ordinal", call.begin_ordinal)
                .set("call_end_ordinal", call.end_ordinal)
                .set("call_parent_index", call.parent_index)
                .set("call_caller", encoding.address(&call.caller))
                .set("call_address", encoding.address(&call.address))
                .set("call_value", &call.value)
                .set("call_gas_consumed", call.gas_consumed)
                .set("call_gas_limit", call.gas_limit)
                .set("call_depth", call.depth)
                .set("call_type", CALL_TYPE);
        }
    }
    Ok(())
}
