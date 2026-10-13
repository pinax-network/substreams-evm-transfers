//! Synthetic blocks through [`project`]: topic 0 is the Keccak-256 of each
//! event's canonical signature, so the tests also check the decoders'
//! signatures.
use super::*;
use buffa::MessageField;
use eth::{Block, Call, CallType, Log, TransactionReceipt, TransactionTrace, TransactionTraceStatus};
use tiny_keccak::{Hasher, Keccak};

const TRANSFER: &str = "Transfer(address,address,uint256)";
const APPROVAL: &str = "Approval(address,address,uint256)";
const MAX_UINT256: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935";

fn keccak(signature: &str) -> Vec<u8> {
    let mut out = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(signature.as_bytes());
    hasher.finalize(&mut out);
    out.to_vec()
}

fn address(byte: u8) -> Vec<u8> {
    vec![byte; 20]
}

/// An address left-padded to a 32-byte topic.
fn topic(byte: u8) -> Vec<u8> {
    let mut out = vec![0; 12];
    out.extend(address(byte));
    out
}

fn word(value: u128) -> Vec<u8> {
    let mut out = vec![0; 16];
    out.extend(value.to_be_bytes());
    out
}

/// A three-topic log of `signature` from `token`.
fn log(signature: &str, token: u8, first: u8, second: u8, data: Vec<u8>, ordinal: u64) -> Log {
    Log {
        address: address(token),
        topics: vec![keccak(signature), topic(first), topic(second)],
        data,
        ordinal,
        ..Default::default()
    }
}

fn call(index: u32, state_reverted: bool, logs: Vec<Log>) -> Call {
    Call {
        index,
        depth: index,
        caller: address(0xca),
        call_type: CallType::Call.into(),
        state_reverted,
        logs,
        ..Default::default()
    }
}

fn trx(hash: u8, status: TransactionTraceStatus, calls: Vec<Call>, receipt_logs: Vec<Log>) -> TransactionTrace {
    TransactionTrace {
        hash: vec![hash; 32],
        from: address(0xf0),
        to: address(0x70),
        nonce: 3,
        gas_limit: 90_000,
        gas_used: 21_000,
        status: status.into(),
        calls,
        receipt: MessageField::some(TransactionReceipt {
            cumulative_gas_used: 1_000_000,
            logs: receipt_logs,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn block(transaction_traces: Vec<TransactionTrace>) -> Block {
    Block {
        number: 100,
        transaction_traces,
        ..Default::default()
    }
}

#[test]
fn signatures_are_the_erc20_topics() {
    assert_eq!(
        substreams::Hex::encode(keccak(TRANSFER)),
        "ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
    );
    assert_eq!(
        substreams::Hex::encode(keccak(APPROVAL)),
        "8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925"
    );
}

#[test]
fn transfers_and_approvals_carry_the_log_the_call_and_the_transaction() {
    let transfer = log(TRANSFER, 0x10, 1, 2, word(1_000), 7);
    let approval = log(APPROVAL, 0x11, 3, 4, word(5), 8);
    let block = block(vec![trx(
        0xaa,
        TransactionTraceStatus::Succeeded,
        vec![call(0, false, vec![transfer.clone()]), call(1, false, vec![approval.clone()])],
        vec![],
    )]);

    let events = project(&block).unwrap();

    assert_eq!(events.transactions.len(), 1);
    let tx = &events.transactions[0];
    assert_eq!(tx.hash, vec![0xaa; 32]);
    assert_eq!((tx.from.clone(), tx.to.clone()), (address(0xf0), Some(address(0x70))));
    assert_eq!((tx.nonce, tx.gas_limit), (3, 90_000));
    // v0.3.3 parity: the receipt's cumulative gas, not `gas_used`.
    assert_eq!(tx.gas_used, 1_000_000);
    assert_eq!((tx.gas_price.as_str(), tx.value.as_str()), ("0", "0"));
    assert_eq!(tx.logs.len(), 2);

    let first = &tx.logs[0];
    assert_eq!(first.address, address(0x10));
    assert_eq!(first.ordinal, 7);
    assert_eq!(first.topics, transfer.topics);
    assert_eq!(first.data, transfer.data);
    let call = first.call.as_option().unwrap();
    assert_eq!((call.caller.clone(), call.index, call.depth), (address(0xca), 0, 0));
    assert_eq!(call.call_type, pb::CallType::Call);
    assert_eq!(
        first.log,
        Some(pb::log::Log::from(pb::Transfer {
            from: address(1),
            to: address(2),
            amount: "1000".into(),
        }))
    );

    let second = &tx.logs[1];
    assert_eq!((second.address.clone(), second.ordinal), (address(0x11), 8));
    assert_eq!(second.call.as_option().unwrap().index, 1);
    assert_eq!(
        second.log,
        Some(pb::log::Log::from(pb::Approval {
            owner: address(3),
            spender: address(4),
            value: "5".into(),
        }))
    );
}

#[test]
fn only_successful_transactions_with_matching_logs_are_kept() {
    let transfer = || log(TRANSFER, 0x10, 1, 2, word(1), 1);
    let unrelated = Log {
        topics: vec![keccak("Sync(uint112,uint112)")],
        data: [word(1), word(2)].concat(),
        ..Default::default()
    };
    let block = block(vec![
        trx(1, TransactionTraceStatus::Failed, vec![call(0, false, vec![transfer()])], vec![]),
        trx(2, TransactionTraceStatus::Succeeded, vec![call(0, false, vec![unrelated])], vec![]),
        trx(3, TransactionTraceStatus::Reverted, vec![call(0, false, vec![transfer()])], vec![]),
        trx(4, TransactionTraceStatus::Succeeded, vec![call(0, false, vec![transfer()])], vec![]),
        trx(5, TransactionTraceStatus::Unknown, vec![call(0, false, vec![transfer()])], vec![]),
    ]);

    let events = project(&block).unwrap();

    let hashes: Vec<u8> = events.transactions.iter().map(|tx| tx.hash[0]).collect();
    assert_eq!(hashes, [4]);
}

#[test]
fn reverted_calls_are_skipped_and_logs_follow_their_ordinals() {
    let block = block(vec![trx(
        1,
        TransactionTraceStatus::Succeeded,
        vec![
            call(0, false, vec![log(TRANSFER, 0x10, 1, 2, word(1), 30), log(TRANSFER, 0x10, 1, 2, word(2), 10)]),
            // A reverted frame's logs never reached the chain.
            call(1, true, vec![log(TRANSFER, 0x10, 1, 2, word(3), 15)]),
            call(2, false, vec![log(APPROVAL, 0x10, 1, 2, word(4), 20)]),
        ],
        vec![],
    )]);

    let events = project(&block).unwrap();

    let logs: Vec<(u64, u32)> = events.transactions[0]
        .logs
        .iter()
        .map(|l| (l.ordinal, l.call.as_option().unwrap().index))
        .collect();
    assert_eq!(logs, [(10, 0), (20, 2), (30, 0)]);
}

#[test]
fn transactions_without_call_traces_use_their_receipt_logs() {
    // Blocks of detail level BASE (Avalanche): no calls, receipt logs only, in
    // receipt order, without call metadata.
    let base = trx(
        1,
        TransactionTraceStatus::Succeeded,
        vec![],
        vec![log(APPROVAL, 0x10, 1, 2, word(9), 5), log(TRANSFER, 0x11, 3, 4, word(8), 2)],
    );
    // With call traces, the receipt logs are not read.
    let extended = trx(
        2,
        TransactionTraceStatus::Succeeded,
        vec![call(0, false, vec![log(TRANSFER, 0x12, 5, 6, word(7), 1)])],
        vec![log(TRANSFER, 0x13, 7, 8, word(6), 1)],
    );

    let events = project(&block(vec![base, extended])).unwrap();

    let base = &events.transactions[0];
    let rows: Vec<(u8, bool)> = base.logs.iter().map(|l| (l.address[0], l.call.is_unset())).collect();
    assert_eq!(rows, [(0x10, true), (0x11, true)]);
    let extended = &events.transactions[1];
    let rows: Vec<(u8, bool)> = extended.logs.iter().map(|l| (l.address[0], l.call.is_set())).collect();
    assert_eq!(rows, [(0x12, true)]);
}

#[test]
fn logs_match_on_topic_count_and_data_length_only() {
    let mut erc721 = log(TRANSFER, 0x10, 1, 2, vec![], 1);
    erc721.topics.push(word(5)); // four topics, no data
    let mut long_data = log(TRANSFER, 0x10, 1, 2, [word(1), word(2)].concat(), 2);
    long_data.ordinal = 2;
    let short_data = log(TRANSFER, 0x10, 1, 2, word(1)[..31].to_vec(), 3);
    // Non-canonical padding: the address is the low 20 bytes regardless.
    let mut dirty = log(TRANSFER, 0x10, 1, 2, word(4), 4);
    dirty.topics[1][0] = 0xff;
    dirty.topics[2][11] = 0x01;
    // A topic too short to hold a word fails to decode and is dropped.
    let mut short_topic = log(TRANSFER, 0x10, 1, 2, word(5), 5);
    short_topic.topics[2].pop();
    // A topic longer than a word decodes from its first word.
    let mut long_topic = log(APPROVAL, 0x10, 1, 2, word(6), 6);
    long_topic.topics[1].push(0xee);

    let block = block(vec![trx(
        1,
        TransactionTraceStatus::Succeeded,
        vec![call(0, false, vec![erc721, long_data, short_data, dirty, short_topic, long_topic])],
        vec![],
    )]);

    let events = project(&block).unwrap();

    let logs = &events.transactions[0].logs;
    assert_eq!(logs.iter().map(|l| l.ordinal).collect::<Vec<_>>(), [4, 6]);
    assert_eq!(
        logs[0].log,
        Some(pb::log::Log::from(pb::Transfer {
            from: address(1),
            to: address(2),
            amount: "4".into(),
        }))
    );
    assert_eq!(
        logs[1].log,
        Some(pb::log::Log::from(pb::Approval {
            owner: address(1),
            spender: address(2),
            value: "6".into(),
        }))
    );
}

#[test]
fn amounts_are_exact_unsigned_decimals() {
    let block = block(vec![trx(
        1,
        TransactionTraceStatus::Succeeded,
        vec![call(
            0,
            false,
            vec![log(TRANSFER, 0x10, 1, 2, vec![0xff; 32], 1), log(TRANSFER, 0x10, 1, 2, word(0), 2)],
        )],
        vec![],
    )]);

    let events = project(&block).unwrap();

    let amounts: Vec<String> = events.transactions[0]
        .logs
        .iter()
        .map(|l| match &l.log {
            Some(pb::log::Log::Transfer(t)) => t.amount.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(amounts, [MAX_UINT256, "0"]);
}

#[test]
fn contract_creations_have_no_recipient_and_amounts_are_decimals() {
    let mut creation = trx(
        1,
        TransactionTraceStatus::Succeeded,
        vec![call(0, false, vec![log(TRANSFER, 0x10, 1, 2, word(1), 1)])],
        vec![],
    );
    creation.to.clear();
    creation.gas_price = MessageField::some(eth::BigInt { bytes: vec![0x01, 0x00] });
    creation.value = MessageField::some(eth::BigInt { bytes: vec![0xff; 32] });

    let events = project(&block(vec![creation])).unwrap();

    let tx = &events.transactions[0];
    assert_eq!(tx.to, None);
    assert_eq!((tx.gas_price.as_str(), tx.value.as_str()), ("256", MAX_UINT256));
}

#[test]
fn a_successful_transaction_without_receipt_fails_the_block() {
    let mut broken = trx(1, TransactionTraceStatus::Succeeded, vec![], vec![]);
    broken.receipt = MessageField::none();
    assert!(project(&block(vec![broken.clone()])).is_err());

    // A failed transaction is never read, receipt or not.
    broken.status = TransactionTraceStatus::Failed.into();
    assert_eq!(project(&block(vec![broken])).unwrap(), pb::Events::default());
}

#[test]
fn a_block_without_matching_logs_gives_no_transactions() {
    assert_eq!(project(&Block::default()).unwrap(), pb::Events::default());
    let quiet = trx(1, TransactionTraceStatus::Succeeded, vec![call(0, false, vec![])], vec![]);
    assert_eq!(project(&block(vec![quiet])).unwrap(), pb::Events::default());
}
