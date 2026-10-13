//! The parts of substreams-evm's `common` crate (tag `evm-transfers-v0.3.3`,
//! `d8a731d`) that `erc20-transfers` and `erc20-tokens` use: the transaction
//! and log templates of `erc20.transfers.v1` and `erc20.tokens.v1`
//! ([`create`]), and the log selection both maps repeated inline
//! ([`logs_with_calls`]).
//!
//! The Tron Base58 and hex encodings live in the `evm-transfers` `db_out`
//! crate; the templates of the DEX, NFT and stETH packages were not ported.

pub mod create;

use substreams::errors::Error;
use substreams_ethereum::pb::eth::v2::{Block, Call, Log, TransactionTrace};

/// The logs of `trx`, each with the call that emitted it, as v0.3.3's ERC-20
/// maps selected them.
///
/// A transaction with call traces gives the logs of its calls that were not
/// reverted (`state_reverted`), in log ordinal order, each with its call. A
/// transaction without call traces, as in blocks of detail level BASE (all of
/// Avalanche), falls back to its receipt logs, in receipt order and without
/// a call.
///
/// # Panics
///
/// Panics if `trx` has no call traces and no receipt. The ERC-20 maps call
/// [`check_receipts`] first and fail the block instead.
pub fn logs_with_calls(trx: &TransactionTrace) -> Vec<(&Log, Option<&Call>)> {
    if trx.calls.is_empty() {
        trx.receipt().logs().map(|log_view| (log_view.log, None)).collect()
    } else {
        trx.logs_with_calls().map(|(log, call_view)| (log, Some(call_view.call))).collect()
    }
}

/// Fails when a successful transaction of `block` has no receipt.
///
/// v0.3.3's ERC-20 maps read the receipt of every successful transaction
/// (for `Transaction.gas_used`, and for the receipt logs of a transaction
/// without call traces) and panicked without one, failing the block; the
/// ports return this error instead.
pub fn check_receipts(block: &Block) -> Result<(), Error> {
    if block.transactions().any(|trx| trx.receipt.is_unset()) {
        return Err(Error::msg("successful transaction has no receipt"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use buffa::MessageField;
    use substreams_ethereum::pb::eth::v2::{TransactionReceipt, TransactionTraceStatus};

    fn log(ordinal: u64, tag: u8) -> Log {
        Log {
            address: vec![tag; 20],
            ordinal,
            ..Default::default()
        }
    }

    fn call(index: u32, state_reverted: bool, logs: Vec<Log>) -> Call {
        Call {
            index,
            state_reverted,
            logs,
            ..Default::default()
        }
    }

    fn receipt(logs: Vec<Log>) -> TransactionReceipt {
        TransactionReceipt { logs, ..Default::default() }
    }

    #[test]
    fn calls_give_their_persisted_logs_in_ordinal_order() {
        let trx = TransactionTrace {
            calls: vec![
                call(0, false, vec![log(9, 1), log(2, 2)]),
                call(1, true, vec![log(3, 3)]),
                call(2, false, vec![log(5, 4)]),
            ],
            // Receipt logs are ignored when the transaction has call traces.
            receipt: MessageField::some(receipt(vec![log(1, 9)])),
            ..Default::default()
        };

        let logs: Vec<(u8, Option<u32>)> = logs_with_calls(&trx).into_iter().map(|(l, c)| (l.address[0], c.map(|c| c.index))).collect();

        assert_eq!(logs, [(2, Some(0)), (4, Some(2)), (1, Some(0))]);
    }

    #[test]
    fn transactions_without_calls_fall_back_to_receipt_logs() {
        // BASE blocks: receipt logs in receipt order, ordinals ignored.
        let trx = TransactionTrace {
            receipt: MessageField::some(receipt(vec![log(7, 1), log(3, 2)])),
            ..Default::default()
        };

        let logs: Vec<(u8, bool)> = logs_with_calls(&trx).into_iter().map(|(l, c)| (l.address[0], c.is_none())).collect();

        assert_eq!(logs, [(1, true), (2, true)]);
    }

    #[test]
    fn only_successful_transactions_need_a_receipt() {
        let succeeded = |receipt: Option<TransactionReceipt>| TransactionTrace {
            status: TransactionTraceStatus::Succeeded.into(),
            receipt: receipt.map(MessageField::some).unwrap_or_default(),
            ..Default::default()
        };
        let mut block = Block {
            transaction_traces: vec![succeeded(Some(receipt(vec![])))],
            ..Default::default()
        };
        assert!(check_receipts(&block).is_ok());

        block.transaction_traces.push(TransactionTrace {
            status: TransactionTraceStatus::Reverted.into(),
            ..Default::default()
        });
        assert!(check_receipts(&block).is_ok());

        block.transaction_traces.push(succeeded(None));
        assert_eq!(check_receipts(&block).unwrap_err().to_string(), "successful transaction has no receipt");
    }
}
