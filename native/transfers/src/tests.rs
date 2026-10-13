//! Synthetic blocks through [`project`].
use super::*;
use buffa::{Inline, MessageField};
use eth::{BalanceChange, Block, Call, CallType, TransactionTrace, TransactionTraceStatus};

const MAX_UINT256: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935";

fn address(byte: u8) -> Vec<u8> {
    vec![byte; 20]
}

fn amount(bytes: &[u8]) -> MessageField<eth::BigInt, Inline<eth::BigInt>> {
    MessageField::some(eth::BigInt { bytes: bytes.to_vec() })
}

fn value_call(index: u32, depth: u32, call_type: CallType, value: &[u8]) -> Call {
    Call {
        index,
        parent_index: index.saturating_sub(1),
        depth,
        call_type: call_type.into(),
        caller: address(0xc0 + index as u8),
        address: address(0xd0 + index as u8),
        value: amount(value),
        gas_limit: 50_000,
        gas_consumed: 30_000,
        begin_ordinal: 10 * index as u64,
        end_ordinal: 10 * index as u64 + 5,
        ..Default::default()
    }
}

fn trx(hash: u8, value: &[u8], calls: Vec<Call>) -> TransactionTrace {
    TransactionTrace {
        hash: vec![hash; 32],
        from: address(0xf0),
        to: address(0x70),
        nonce: 11,
        gas_price: amount(&[0x3b, 0x9a, 0xca, 0x00]),
        gas_limit: 100_000,
        gas_used: 21_000,
        value: amount(value),
        status: TransactionTraceStatus::Succeeded.into(),
        calls,
        ..Default::default()
    }
}

fn reward(reason: Reason, old: &[u8], new: &[u8]) -> BalanceChange {
    BalanceChange {
        address: address(0xbb),
        old_value: amount(old),
        new_value: amount(new),
        reason: reason.into(),
        ..Default::default()
    }
}

#[test]
fn transactions_and_value_calls_follow_v0_2_0() {
    let calls = vec![
        // The root call repeats the transaction's own value.
        value_call(0, 0, CallType::Call, &[5]),
        value_call(1, 1, CallType::Call, &[7]),
        value_call(2, 1, CallType::Delegate, &[7]),
        value_call(3, 2, CallType::Create, &[7]),
        value_call(4, 1, CallType::Call, &[]),
        value_call(5, 3, CallType::Call, &[0xff; 32]),
    ];
    let block = Block {
        transaction_traces: vec![trx(1, &[5], calls)],
        ..Default::default()
    };

    let events = project(&block);

    assert!(events.block_rewards.is_empty());
    assert_eq!(events.transactions.len(), 1);
    let tx = &events.transactions[0];
    assert_eq!(tx.hash, vec![1; 32]);
    assert_eq!((tx.from.clone(), tx.to.clone()), (address(0xf0), Some(address(0x70))));
    assert_eq!((tx.nonce, tx.gas_limit), (11, 100_000));
    // The transaction's own gas, not the receipt's cumulative gas.
    assert_eq!(tx.gas_used, 21_000);
    assert_eq!((tx.gas_price.as_str(), tx.value.as_str()), ("1000000000", "5"));

    let calls: Vec<(u32, &str)> = tx.calls.iter().map(|c| (c.index, c.value.as_str())).collect();
    assert_eq!(calls, [(1, "7"), (5, MAX_UINT256)]);
    assert_eq!(
        tx.calls[0],
        pb::Call {
            index: 1,
            begin_ordinal: 10,
            end_ordinal: 15,
            caller: address(0xc1),
            address: address(0xd1),
            value: "7".into(),
            gas_consumed: 30_000,
            gas_limit: 50_000,
            depth: 1,
            parent_index: 0,
        }
    );
}

#[test]
fn failed_and_reverted_calls_move_no_value() {
    let mut reverted = value_call(1, 1, CallType::Call, &[1]);
    reverted.state_reverted = true;
    let mut failed = value_call(2, 1, CallType::Call, &[1]);
    failed.status_failed = true;
    let mut status_reverted = value_call(3, 1, CallType::Call, &[1]);
    status_reverted.status_reverted = true;
    let mut unknown_type = value_call(4, 1, CallType::Call, &[1]);
    unknown_type.call_type = 42.into();
    let block = Block {
        transaction_traces: vec![trx(1, &[], vec![reverted, failed, status_reverted, unknown_type])],
        ..Default::default()
    };

    // No value and no value call: the transaction is left out.
    assert_eq!(project(&block), pb::Events::default());
}

#[test]
fn only_successful_transactions_with_value_are_kept() {
    let mut failed = trx(1, &[9], vec![]);
    failed.status = TransactionTraceStatus::Failed.into();
    let mut reverted = trx(2, &[9], vec![]);
    reverted.status = TransactionTraceStatus::Reverted.into();
    let mut creation = trx(4, &[9], vec![]);
    creation.to.clear();
    // A gas price wider than 20 bytes reads as zero.
    creation.gas_price = amount(&[1; 21]);
    let mut unset = trx(5, &[], vec![value_call(1, 1, CallType::Call, &[3])]);
    unset.value = MessageField::none();
    unset.gas_price = MessageField::none();
    let block = Block {
        transaction_traces: vec![failed, reverted, trx(3, &[0, 0], vec![]), creation, unset],
        ..Default::default()
    };

    let events = project(&block);

    let txs: Vec<(u8, Option<Vec<u8>>, &str, &str)> = events
        .transactions
        .iter()
        .map(|tx| (tx.hash[0], tx.to.clone(), tx.value.as_str(), tx.gas_price.as_str()))
        .collect();
    assert_eq!(txs, [(4, None, "9", "0"), (5, Some(address(0x70)), "0", "0")]);
    assert_eq!(events.transactions[1].calls.len(), 1);
}

#[test]
fn block_rewards_are_balance_increases_with_reward_reasons() {
    let mut unknown = reward(Reason::RewardMineBlock, &[], &[1]);
    unknown.reason = 99.into();
    let block = Block {
        balance_changes: vec![
            reward(Reason::RewardMineBlock, &[1, 0], &[3, 0]),
            reward(Reason::RewardTransactionFee, &[], &[7]),
            reward(Reason::RewardBlobFee, &[5], &[6]),
            reward(Reason::RewardMineUncle, &[0xff; 32], &[0xff; 32]),
            reward(Reason::IncreaseMint, &[8], &[2]),
            reward(Reason::Transfer, &[], &[9]),
            reward(Reason::GasRefund, &[], &[9]),
            reward(Reason::Withdrawal, &[], &[9]),
            unknown,
            reward(Reason::IncreaseMint, &[], &[0xff; 32]),
        ],
        ..Default::default()
    };

    let events = project(&block);

    let rewards: Vec<(&str, pb::Reason)> = events.block_rewards.iter().map(|r| (r.value.as_str(), r.reason.as_known().unwrap())).collect();
    assert_eq!(
        rewards,
        [
            ("512", pb::Reason::RewardMineBlock),
            ("7", pb::Reason::RewardTransactionFee),
            ("1", pb::Reason::RewardBlobFee),
            (MAX_UINT256, pb::Reason::IncreaseMint),
        ]
    );
    assert!(events.block_rewards.iter().all(|r| r.miner == address(0xbb)));
    assert!(events.transactions.is_empty());
}

#[test]
fn an_empty_block_gives_no_events() {
    assert_eq!(project(&Block::default()), pb::Events::default());
}
