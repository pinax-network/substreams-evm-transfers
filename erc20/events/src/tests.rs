use super::*;
use prost::Message;

const FULL_BLOCK: &[u8] = include_bytes!("../../balances/tests/fixtures/bsc-122260950.pb");
const WBNB_DEPOSIT: &[u8] = include_bytes!("../../balances/tests/fixtures/wbnb-mutations/122288015-tx18-deposit-nested-no-transfer.pb");
const REVERTED_CHILD: &[u8] = include_bytes!("../../balances/tests/fixtures/wbnb-mutations/122288007-tx22-succeeded-with-reverted-child-write.pb");
const WBNB: &str = "bb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c";

fn config() -> Config {
    parse(r#"{"chain_id":56,"producer_versions":[5]}"#).unwrap()
}
fn block() -> eth::Block {
    eth::Block {
        ver: 5,
        number: 100,
        hash: vec![1; 32],
        detail_level: eth::block::DetailLevel::DetaillevelExtended as i32,
        header: Some(eth::BlockHeader {
            number: 100,
            parent_hash: vec![2; 32],
            state_root: vec![3; 32],
            timestamp: Some(prost_types::Timestamp { seconds: 1789000000, nanos: 0 }),
            ..Default::default()
        }),
        ..Default::default()
    }
}
fn tx(calls: Vec<eth::Call>) -> eth::TransactionTrace {
    eth::TransactionTrace {
        status: eth::TransactionTraceStatus::Succeeded as i32,
        hash: vec![7; 32],
        index: 3,
        calls,
        ..Default::default()
    }
}
fn padded(byte: u8) -> Vec<u8> {
    let mut t = vec![0u8; 32];
    t[12..].fill(byte);
    t
}
fn transfer_log(from: Vec<u8>, to: Vec<u8>, data: Vec<u8>, ordinal: u64) -> eth::Log {
    eth::Log {
        address: vec![0xaa; 20],
        topics: vec![TRANSFER_TOPIC.to_vec(), from, to],
        data,
        ordinal,
        ..Default::default()
    }
}

#[test]
fn captured_block_classifies_every_transfer_and_approval_signature() {
    let block = eth::Block::decode(FULL_BLOCK).unwrap();
    let events = project(&block, &config()).unwrap();
    assert_eq!((events.transfers.len(), events.approvals.len()), (121, 21));
    assert_eq!((events.clocks[0].transfer_count, events.clocks[0].approval_count), (121, 21));
    assert!(events
        .transfers
        .iter()
        .all(|t| t.shape == pb::LogShape::Erc20 as i32 && t.from.len() == 20 && t.to.len() == 20 && !t.amount.is_empty()));
    assert_eq!(events.transfers.iter().filter(|t| t.persisted).count(), 110);
    assert_eq!(events.approvals.iter().filter(|a| a.persisted).count(), 17);
    assert_eq!(events.approvals.iter().filter(|a| a.zero_value).count(), 8);
    assert!(events.approvals.iter().filter(|a| a.zero_value).all(|a| a.value == "0"));
    // Every Transfer/Approval-signed log of the block is accounted for, once.
    let signed = block
        .transaction_traces
        .iter()
        .flat_map(|t| &t.calls)
        .flat_map(|c| &c.logs)
        .filter(|l| {
            l.topics
                .first()
                .is_some_and(|t| t.as_slice() == TRANSFER_TOPIC || t.as_slice() == APPROVAL_TOPIC)
        })
        .count();
    assert_eq!(signed, events.transfers.len() + events.approvals.len());
    // Persisted rows are exactly the receipt logs with these signatures.
    let receipt_signed = block
        .transaction_traces
        .iter()
        .filter_map(|t| t.receipt.as_ref())
        .flat_map(|r| &r.logs)
        .filter(|l| {
            l.topics
                .first()
                .is_some_and(|t| t.as_slice() == TRANSFER_TOPIC || t.as_slice() == APPROVAL_TOPIC)
        })
        .count();
    assert_eq!(
        receipt_signed,
        events.transfers.iter().filter(|t| t.persisted).count() + events.approvals.iter().filter(|a| a.persisted).count()
    );
    assert!(events.transfers.iter().filter(|t| !t.persisted).all(|t| t.block_index == 0));
    // WBNB: 17 signed transfers, 14 persisted.
    let wbnb = hex::decode(WBNB).unwrap();
    assert_eq!(events.transfers.iter().filter(|t| t.token == wbnb).count(), 17);
    assert_eq!(events.transfers.iter().filter(|t| t.token == wbnb && t.persisted).count(), 14);
    // Deterministic order.
    assert!(events
        .transfers
        .windows(2)
        .all(|w| (w[0].scope, w[0].transaction_index, w[0].ordinal) <= (w[1].scope, w[1].transaction_index, w[1].ordinal)));
}

#[test]
fn wrapped_native_deposit_emits_no_transfer_evidence_while_the_balance_changed() {
    // The storage mapper emits the depositor's WBNB balance (see
    // erc20/balances wbnb_mutation_tests); event evidence alone cannot.
    let block = eth::Block::decode(WBNB_DEPOSIT).unwrap();
    let events = project(&block, &config()).unwrap();
    let wbnb = hex::decode(WBNB).unwrap();
    assert!(events.transfers.iter().all(|t| t.token != wbnb));
    assert_eq!(events.clocks.len(), 1);
}

#[test]
fn reverted_child_transfers_are_attempts_and_the_persisted_ones_are_receipt_logs() {
    let block = eth::Block::decode(REVERTED_CHILD).unwrap();
    let events = project(&block, &config()).unwrap();
    let wbnb = hex::decode(WBNB).unwrap();
    let rows: Vec<_> = events.transfers.iter().filter(|t| t.token == wbnb).collect();
    assert_eq!(rows.len(), 3);
    let attempted: Vec<_> = rows.iter().filter(|t| !t.persisted).collect();
    assert_eq!(attempted.len(), 1);
    assert_eq!(attempted[0].ordinal, 780);
    // The same participants and amount were then transferred in a persisted
    // frame; both facts are kept, distinguished by `persisted`.
    let replay = rows.iter().find(|t| t.persisted && t.ordinal == 804).unwrap();
    assert_eq!(
        (replay.from.clone(), replay.to.clone(), &*replay.amount),
        (attempted[0].from.clone(), attempted[0].to.clone(), &*attempted[0].amount)
    );
    assert_eq!(replay.amount, "80404901699414743");
    let lean = parse(r#"{"chain_id":56,"producer_versions":[5],"include_attempted":false}"#).unwrap();
    let events = project(&block, &lean).unwrap();
    assert!(events.transfers.iter().all(|t| t.persisted));
    assert_eq!(events.transfers.iter().filter(|t| t.token == wbnb).count(), 2);
}

#[test]
fn shapes_are_classified_by_encoding_not_by_contract() {
    let erc20 = transfer_log(padded(4), padded(5), vec![0; 31].into_iter().chain([7]).collect(), 1);
    assert_eq!(
        decode(&erc20),
        Decoded {
            shape: pb::LogShape::Erc20,
            first: vec![4; 20],
            second: vec![5; 20],
            quantity: "7".into()
        }
    );
    let mut erc721 = transfer_log(padded(4), padded(5), vec![], 2);
    erc721.topics.push({
        let mut id = vec![0u8; 32];
        id[31] = 9;
        id
    });
    assert_eq!(decode(&erc721).shape, pb::LogShape::Erc721);
    assert_eq!(decode(&erc721).quantity, "9");
    // Unpadded address topic, wrong data size, or an unindexed variant.
    let mut dirty = transfer_log(padded(4), padded(5), vec![0; 32], 3);
    dirty.topics[1][0] = 1;
    assert_eq!(decode(&dirty).shape, pb::LogShape::Nonstandard);
    let short = transfer_log(padded(4), padded(5), vec![1, 2, 3], 4);
    assert_eq!(decode(&short).shape, pb::LogShape::Nonstandard);
    let unindexed = eth::Log {
        topics: vec![TRANSFER_TOPIC.to_vec()],
        data: vec![0; 96],
        ..Default::default()
    };
    assert_eq!(decode(&unindexed).shape, pb::LogShape::Nonstandard);
    assert!(decode(&unindexed).first.is_empty() && decode(&unindexed).quantity.is_empty());
}

#[test]
fn zero_address_and_self_transfers_are_facts_not_labels() {
    let mut b = block();
    let mint = transfer_log(padded(0), padded(5), vec![0; 32], 1);
    let burn = transfer_log(padded(5), padded(0), vec![0; 32], 2);
    let selfie = transfer_log(padded(5), padded(5), vec![0; 32], 3);
    let mut unlimited = eth::Log {
        address: vec![0xaa; 20],
        topics: vec![APPROVAL_TOPIC.to_vec(), padded(5), padded(6)],
        data: vec![0xff; 32],
        ordinal: 4,
        ..Default::default()
    };
    let revoke = eth::Log {
        data: vec![0; 32],
        ordinal: 5,
        ..unlimited.clone()
    };
    unlimited.ordinal = 4;
    b.transaction_traces = vec![tx(vec![eth::Call {
        logs: vec![mint, burn, selfie, unlimited, revoke],
        ..Default::default()
    }])];
    let events = project(&b, &config()).unwrap();
    assert_eq!(events.transfers.len(), 3);
    assert!(events.transfers[0].from_zero && !events.transfers[0].to_zero);
    assert!(events.transfers[1].to_zero && !events.transfers[1].from_zero);
    assert!(events.transfers[2].self_transfer);
    assert_eq!(events.approvals.len(), 2);
    assert!(events.approvals[0].unlimited && !events.approvals[0].zero_value);
    assert!(events.approvals[1].zero_value && !events.approvals[1].unlimited);
}

#[test]
fn failed_transactions_system_calls_and_parameters_follow_the_contract() {
    let mut b = block();
    let mut failed = tx(vec![eth::Call {
        state_reverted: true,
        logs: vec![transfer_log(padded(4), padded(5), vec![0; 32], 1)],
        ..Default::default()
    }]);
    failed.status = eth::TransactionTraceStatus::Reverted as i32;
    b.transaction_traces = vec![failed];
    b.system_calls = vec![eth::Call {
        logs: vec![transfer_log(padded(6), padded(7), vec![0; 32], 9)],
        ..Default::default()
    }];
    let events = project(&b, &config()).unwrap();
    assert_eq!(events.transfers.len(), 2);
    let system = events.transfers.iter().find(|t| t.scope == pb::Scope::SystemCall as i32).unwrap();
    assert!(system.persisted && system.transaction_hash.is_empty());
    let attempted = events.transfers.iter().find(|t| t.scope == pb::Scope::Transaction as i32).unwrap();
    assert!(!attempted.persisted);
    assert!(parse(r#"{"chain_id":56,"producer_versions":[]}"#).is_err());
    assert!(parse(r#"{"chain_id":56,"producer_versions":[5],"x":1}"#).is_err());
    b.ver = 3;
    assert!(project(&b, &config()).unwrap_err().to_string().contains("producer version"));
    let empty = project(&block(), &config()).unwrap();
    assert!(empty.transfers.is_empty() && empty.clocks.len() == 1);
}

#[test]
fn topic_constants_match_the_canonical_signatures() {
    use tiny_keccak::{Hasher, Keccak};
    for (sig, expected) in [
        ("Transfer(address,address,uint256)", TRANSFER_TOPIC),
        ("Approval(address,address,uint256)", APPROVAL_TOPIC),
    ] {
        let mut out = [0u8; 32];
        let mut h = Keccak::v256();
        h.update(sig.as_bytes());
        h.finalize(&mut out);
        assert_eq!(out, expected);
    }
}
