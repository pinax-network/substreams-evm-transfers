//! One `db_out` that writes the events of the three transfer maps into the
//! Token API's `evm-transfers` ClickHouse tables.
//!
//! Ported from substreams-evm `db-evm-transfers` at tag `evm-transfers-v0.3.3`
//! (`d8a731d`), the `db_out` that production `evm-transfers` v0.3.3 runs; the
//! rows are v0.3.3's, including the behaviour the package README lists under
//! "Kept from v0.3.3". Every row is a CREATE, in this order:
//! - `erc20_transfers` and `erc20_approvals`, one per `Transfer` and
//!   `Approval` of `erc20_transfers:map_events`;
//! - one row per WETH, USDC, USDT, WBTC, SAI and stETH event of
//!   `erc20_tokens:map_events` that v0.3.3 stored (26 tables, such as
//!   `weth_deposit` or `usdt_issue`);
//! - `block_rewards`, then `transactions` each followed by its `calls`, from
//!   `native_transfers:map_events`;
//! - one `blocks` row, keyed by `block_num`, when the block wrote any other
//!   row.
//!
//! Every row carries the clock columns; log rows carry the transaction and
//! log columns, `transactions` and `calls` the transaction columns. Addresses
//! follow the [`Encoding`] of the `db_out` param (`hex` or `tron_base58`);
//! hashes, topics and data are always hex.
//!
//! Where v0.3.3 panicked, [`changes`] returns an error, and the block fails
//! either way: an unknown param (in the handler), or a clock without a
//! timestamp in a block that writes rows. The changes are encoded by
//! `substreams-database-change` 5.0.0, where every `Field` also carries
//! `update_op = UPDATE_OP_SET` (field 4); v0.3.3's 3.0.0 has no such field,
//! and the SQL sink uses only a CREATE field's name and value. Fields are
//! sorted by name.
mod columns;
mod encoding;
mod erc20_tokens;
mod erc20_transfers;
mod native_transfers;

use columns::BlockColumns;
pub use encoding::Encoding;
/// The `erc20.tokens.v1` messages of `erc20_tokens:map_events`.
pub use proto::pb::erc20::tokens::v1 as tokens_pb;
/// The `erc20.transfers.v1` messages of `erc20_transfers:map_events`.
pub use proto::pb::erc20::transfers::v1 as transfers_pb;
/// The `native.transfers.v1` messages of `native_transfers:map_events`.
pub use proto::pb::native::transfers::v1 as native_pb;
use substreams::{errors::Error, pb::substreams::Clock};
use substreams_database_change::{pb::sf::substreams::sink::database::v1::DatabaseChanges, tables::Tables};

/// The `db_out` body: the rows of the three maps' events, in v0.3.3's order,
/// then the `blocks` row.
///
/// # Errors
///
/// A clock without a timestamp, when the events write a row (v0.3.3
/// panicked). A block without rows writes nothing whatever its clock.
pub fn changes(
    clock: &Clock,
    erc20_transfers: &transfers_pb::Events,
    erc20_tokens: &tokens_pb::Events,
    native_transfers: &native_pb::Events,
    encoding: Encoding,
) -> Result<DatabaseChanges, Error> {
    let block = BlockColumns::new(clock);
    let mut tables = Tables::new();

    // Process logs (ERC20 transfers)
    erc20_transfers::process_events(encoding, &mut tables, &block, erc20_transfers)?;

    // Process logs (ERC20 token-specific events)
    erc20_tokens::process_events(encoding, &mut tables, &block, erc20_tokens)?;

    // Process transactions (Native transfers)
    native_transfers::process_events(encoding, &mut tables, &block, native_transfers)?;

    // ONLY include blocks if events are present. Every table `Tables` holds
    // has at least one row, so this is v0.3.3's `!tables.tables.is_empty()`.
    if tables.all_row_count() != 0 {
        block.set(tables.create_row("blocks", [("block_num", block.block_num.clone())]));
    }

    let mut changes = tables.to_database_changes();
    // The crate emits a row's fields in `HashMap` order, which changes with
    // every WASM build; the sink reads them by name. Sorting keeps the output
    // bytes stable across builds. Composite keys stay in map order.
    for change in &mut changes.table_changes {
        change.fields.sort_by(|a, b| a.name.cmp(&b.name));
    }
    Ok(changes)
}

// The SDK macro generates raw-pointer parameter decoding and discards function
// attributes. As in the map packages, the export exists only in the WASM build.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;

    /// `params` is the address encoding: `hex` (the manifest default) or
    /// `tron_base58`.
    #[substreams::handlers::map]
    fn db_out(
        params: String,
        clock: Clock,
        erc20_transfers: transfers_pb::Events,
        erc20_tokens: tokens_pb::Events,
        native_transfers: native_pb::Events,
    ) -> Result<DatabaseChanges, Error> {
        // Handle support both EVM & TVM address encoding
        let encoding = Encoding::from_param(&params)?;
        let changes = changes(&clock, &erc20_transfers, &erc20_tokens, &native_transfers, encoding)?;
        substreams::log::info!("Total rows {}", changes.table_changes.len());
        Ok(changes)
    }
}
