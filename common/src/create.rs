//! Templates that copy transaction and log metadata from a Firehose block into
//! `erc20.transfers.v1` and `erc20.tokens.v1` messages.
//!
//! Ported from substreams-evm `common/src/create.rs` (`d8a731d`), where one
//! macro, `impl_create_log_with_call_metadata!`, implemented them for these
//! two packages (and stETH); only these two implementations are kept. Field
//! values are those of v0.3.3.

use substreams::scalar::BigInt;
use substreams_ethereum::pb::eth::v2::{self as eth, Call, Log, TransactionTrace};

/// Creates a log entry from a raw log and the event decoded from it.
pub trait CreateLog<E> {
    fn create_log(log: &Log, event: E) -> Self;
    fn create_log_with_call(log: &Log, event: E, call: Option<&Call>) -> Self;
}

/// Creates a transaction from a transaction trace.
pub trait CreateTransaction {
    fn create_transaction(trx: &TransactionTrace) -> Self;
}

/// A Firehose integer as a decimal string; an unset value is `"0"`.
///
/// v0.3.3 wrote `value.unwrap_or_default().with_decimal(0).to_string()`: a
/// division by 1 that keeps the unsigned integer at scale 0, which prints as
/// its decimal digits.
pub fn decimal(value: Option<&eth::BigInt>) -> String {
    value.map_or_else(|| "0".to_string(), |value| BigInt::from_unsigned_bytes_be(&value.bytes).to_string())
}

macro_rules! impl_create_log_with_call_metadata {
    ($module:path) => {
        use $module as pb;

        impl CreateLog<pb::log::Log> for pb::Log {
            fn create_log(log: &Log, event: pb::log::Log) -> Self {
                Self::create_log_with_call(log, event, None)
            }

            fn create_log_with_call(log: &Log, event: pb::log::Log, call: Option<&Call>) -> Self {
                Self {
                    address: log.address.to_vec(),
                    ordinal: log.ordinal,
                    topics: log.topics.iter().map(|t| t.to_vec()).collect(),
                    data: log.data.to_vec(),
                    call: call
                        .map(|c| pb::Call {
                            caller: c.caller.to_vec(),
                            index: c.index,
                            depth: c.depth,
                            // Both enums number their variants alike; an
                            // unknown value is carried through unchanged, as
                            // prost's raw i32 was.
                            call_type: c.call_type.to_i32().into(),
                        })
                        .into(),
                    log: Some(event),
                }
            }
        }

        impl CreateTransaction for pb::Transaction {
            /// # Panics
            ///
            /// Panics if the transaction carries no receipt, as v0.3.3 did.
            /// The maps call [`crate::check_receipts`] first and fail the
            /// block instead.
            fn create_transaction(trx: &TransactionTrace) -> Self {
                Self {
                    from: trx.from.to_vec(),
                    to: if trx.to.is_empty() { None } else { Some(trx.to.to_vec()) },
                    hash: trx.hash.to_vec(),
                    nonce: trx.nonce,
                    gas_price: decimal(trx.gas_price.as_option()),
                    gas_limit: trx.gas_limit,
                    // v0.3.3 reports the receipt's cumulative gas, not the
                    // transaction's own `gas_used`; kept for parity.
                    gas_used: trx.receipt().receipt.cumulative_gas_used,
                    value: decimal(trx.value.as_option()),
                    logs: vec![],
                }
            }
        }
    };
}

mod erc20_transfers_impl {
    use super::*;
    impl_create_log_with_call_metadata!(proto::pb::erc20::transfers::v1);
}

mod erc20_tokens_impl {
    use super::*;
    impl_create_log_with_call_metadata!(proto::pb::erc20::tokens::v1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use buffa::MessageField;
    use proto::pb::erc20::tokens::v1 as tokens;
    use proto::pb::erc20::transfers::v1 as transfers;
    use substreams_ethereum::pb::eth::v2::{CallType, TransactionReceipt};

    fn sample_call() -> Call {
        Call {
            index: 7,
            parent_index: 3,
            depth: 2,
            call_type: CallType::Delegate.into(),
            caller: vec![0x22; 20],
            address: vec![0x33; 20],
            value: MessageField::some(eth::BigInt { bytes: vec![0x2a] }),
            ..Default::default()
        }
    }

    fn sample_transaction() -> TransactionTrace {
        TransactionTrace {
            to: vec![0x77; 20],
            nonce: 9,
            gas_price: MessageField::some(eth::BigInt { bytes: vec![0x64] }),
            gas_limit: 500,
            value: MessageField::some(eth::BigInt { bytes: vec![0x2a] }),
            gas_used: 321,
            hash: vec![0x88; 32],
            from: vec![0x99; 20],
            receipt: MessageField::some(TransactionReceipt {
                cumulative_gas_used: 654,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn sample_log() -> Log {
        Log {
            address: vec![0x11; 20],
            topics: vec![vec![0xaa; 32], vec![0xbb; 32]],
            data: vec![0xcc, 0xdd],
            index: 4,
            block_index: 12,
            ordinal: 42,
        }
    }

    #[test]
    fn decimal_writes_unsigned_integers_and_unset_as_zero() {
        assert_eq!(decimal(None), "0");
        assert_eq!(decimal(Some(&eth::BigInt { bytes: vec![] })), "0");
        assert_eq!(decimal(Some(&eth::BigInt { bytes: vec![0x00, 0x2a] })), "42");
        // 2^256 - 1 and 2^256: the top bit is never a sign.
        assert_eq!(
            decimal(Some(&eth::BigInt { bytes: vec![0xff; 32] })),
            "115792089237316195423570985008687907853269984665640564039457584007913129639935"
        );
        let mut wide = vec![1];
        wide.extend([0; 32]);
        assert_eq!(
            decimal(Some(&eth::BigInt { bytes: wide })),
            "115792089237316195423570985008687907853269984665640564039457584007913129639936"
        );
    }

    #[test]
    fn create_transaction_populates_metadata_fields() {
        let created = transfers::Transaction::create_transaction(&sample_transaction());

        assert_eq!(created.hash, vec![0x88; 32]);
        assert_eq!(created.from, vec![0x99; 20]);
        assert_eq!(created.to, Some(vec![0x77; 20]));
        assert_eq!(created.nonce, 9);
        assert_eq!(created.gas_price, "100");
        assert_eq!(created.gas_limit, 500);
        // v0.3.3 parity: the receipt's cumulative gas, not `gas_used`.
        assert_eq!(created.gas_used, 654);
        assert_eq!(created.value, "42");
        assert!(created.logs.is_empty());

        let tokens = tokens::Transaction::create_transaction(&sample_transaction());
        assert_eq!((tokens.gas_used, tokens.gas_price.as_str(), tokens.value.as_str()), (654, "100", "42"));
    }

    #[test]
    fn create_transaction_maps_contract_creation_and_unset_amounts() {
        let mut trx = sample_transaction();
        trx.to.clear();
        trx.gas_price = MessageField::none();
        trx.value = MessageField::none();

        let created = transfers::Transaction::create_transaction(&trx);

        assert_eq!(created.to, None);
        assert_eq!(created.gas_price, "0");
        assert_eq!(created.value, "0");
    }

    #[test]
    fn create_log_with_call_copies_the_log_and_the_call_metadata() {
        let event = transfers::log::Log::from(transfers::Transfer {
            from: vec![1; 20],
            to: vec![2; 20],
            amount: "5".into(),
        });

        let created = transfers::Log::create_log_with_call(&sample_log(), event.clone(), Some(&sample_call()));

        assert_eq!(created.address, vec![0x11; 20]);
        assert_eq!(created.topics, vec![vec![0xaa; 32], vec![0xbb; 32]]);
        assert_eq!(created.data, vec![0xcc, 0xdd]);
        assert_eq!(created.ordinal, 42);
        assert_eq!(created.log, Some(event));
        let call = created.call.as_option().expect("call metadata");
        assert_eq!(call.caller, vec![0x22; 20]);
        assert_eq!((call.index, call.depth), (7, 2));
        assert_eq!(call.call_type, transfers::CallType::Delegate);
    }

    #[test]
    fn create_log_keeps_unknown_call_types_and_leaves_call_unset_without_one() {
        let mut call = sample_call();
        call.call_type = 42.into();
        let event = tokens::log::Log::from(tokens::Pause {});

        let created = tokens::Log::create_log_with_call(&sample_log(), event.clone(), Some(&call));
        assert_eq!(created.call.as_option().unwrap().call_type.to_i32(), 42);

        let created = tokens::Log::create_log(&sample_log(), event);
        assert!(created.call.is_unset());
    }
}
