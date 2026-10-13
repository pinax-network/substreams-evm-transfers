//! `erc20_transfers` and `erc20_approvals`. Ported from substreams-evm
//! `db-evm-transfers/src/erc20_transfers.rs` (`d8a731d`).

use substreams::errors::Error;
use substreams_database_change::tables::Tables;

use crate::columns::{set_log, set_transaction, BlockColumns};
use crate::{transfers_pb as pb, Encoding};

pub(crate) fn process_events(encoding: Encoding, tables: &mut Tables, block: &BlockColumns, events: &pb::Events) -> Result<(), Error> {
    for (tx_index, tx) in events.transactions.iter().enumerate() {
        for (log_index, log) in tx.logs.iter().enumerate() {
            let (table, columns) = match &log.log {
                Some(pb::log::Log::Transfer(transfer)) => (
                    "erc20_transfers",
                    [
                        ("from", encoding.address(&transfer.from)),
                        ("to", encoding.address(&transfer.to)),
                        ("amount", transfer.amount.clone()),
                    ],
                ),
                Some(pb::log::Log::Approval(approval)) => (
                    "erc20_approvals",
                    [
                        ("owner", encoding.address(&approval.owner)),
                        ("spender", encoding.address(&approval.spender)),
                        ("value", approval.value.clone()),
                    ],
                ),
                None => continue,
            };
            let row = tables.create_row(table, block.log_key(tx_index, log_index)?);
            block.set(row);
            set_log(encoding, log, log_index, row);
            set_transaction(encoding, tx, tx_index, row);
            for (name, value) in columns {
                row.set(name, value);
            }
        }
    }
    Ok(())
}
