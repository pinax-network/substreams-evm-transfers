#![cfg(not(target_arch = "wasm32"))]
//! Differential boundary corpus: execute the old package, call the real native
//! projector, and retain both exact outcomes. Expected differences are not parity.
#[path = "support/legacy_wasm.rs"]
mod legacy_wasm;

use buffa::Message;
use erc20_events::{parse, project, TRANSFER_TOPIC};
use legacy_wasm::Oracle;
use proto::pb::erc20::events::v1::{LogShape, Scope};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use substreams_database_change::pb::database::DatabaseChanges;
use substreams_ethereum::pb::eth::v2 as eth;

type Row = BTreeMap<String, String>;
const CORPUS: &str = "fixtures/legacy-v0.4.0/corpus.json";
const COLUMNS: [&str; 8] = ["log_address", "log_ordinal", "tx_hash", "tx_from", "tx_to", "from", "to", "amount"];

fn hex(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}
fn padded(byte: u8) -> Vec<u8> {
    let mut bytes = vec![0; 12];
    bytes.extend([byte; 20]);
    bytes
}
fn log(ordinal: u64, index: u32) -> eth::Log {
    let mut data = vec![0; 32];
    data[31] = 17;
    eth::Log {
        address: vec![0xaa; 20],
        topics: vec![TRANSFER_TOPIC.to_vec(), padded(0x11), padded(0x22)],
        data,
        ordinal,
        index,
        block_index: index,
    }
}
fn block() -> eth::Block {
    let logs = vec![log(42, 0)];
    eth::Block {
        ver: 5,
        number: 100,
        hash: vec![0xab; 32],
        detail_level: eth::block::DetailLevel::DetaillevelExtended.into(),
        header: buffa::MessageField::some(eth::BlockHeader {
            number: 100,
            parent_hash: vec![0xcd; 32],
            state_root: vec![0xef; 32],
            timestamp: buffa::MessageField::some(buffa_types::google::protobuf::Timestamp {
                seconds: 1_790_000_000,
                ..Default::default()
            }),
            ..Default::default()
        }),
        transaction_traces: vec![eth::TransactionTrace {
            index: 0,
            hash: vec![0x77; 32],
            from: vec![0x33; 20],
            to: vec![0x44; 20],
            status: eth::TransactionTraceStatus::Succeeded.into(),
            calls: vec![eth::Call {
                logs: logs.clone(),
                ..Default::default()
            }],
            receipt: buffa::MessageField::some(eth::TransactionReceipt { logs, ..Default::default() }),
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn expected_row(ordinal: u64, hash_byte: u8) -> Row {
    [
        ("log_address", hex(&[0xaa; 20])),
        ("log_ordinal", ordinal.to_string()),
        ("tx_hash", hex(&[hash_byte; 32])),
        ("tx_from", hex(&[0x33; 20])),
        ("tx_to", hex(&[0x44; 20])),
        ("from", hex(&[0x11; 20])),
        ("to", hex(&[0x22; 20])),
        ("amount", "17".to_string()),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_string(), value))
    .collect()
}

struct Case {
    name: &'static str,
    difference: &'static str,
    block: eth::Block,
    legacy: Option<Vec<Row>>,
    native: Result<Vec<Row>, &'static str>,
    // All signature evidence before selecting persisted transaction ERC20 rows:
    // ERC20, ERC721, nonstandard, system scope, attempted.
    evidence: [usize; 5],
}
fn equal(name: &'static str, block: eth::Block, rows: Vec<Row>) -> Case {
    let count = rows.len();
    Case {
        name,
        difference: "none",
        block,
        legacy: Some(rows.clone()),
        native: Ok(rows),
        evidence: [count, 0, 0, 0, 0],
    }
}
fn cases() -> Vec<Case> {
    let row = expected_row(42, 0x77);
    let mut out = vec![equal("canonical", block(), vec![row.clone()])];
    let mut b = block();
    b.ver = 4;
    out.push(equal("extended_v4", b, vec![row.clone()]));
    let mut b = block();
    b.transaction_traces.clear();
    out.push(equal("empty", b, vec![]));
    let mut b = block();
    b.transaction_traces[0].calls[0].logs[0].data.fill(255);
    let mut max = row.clone();
    max.insert(
        "amount".into(),
        "115792089237316195423570985008687907853269984665640564039457584007913129639935".into(),
    );
    out.push(equal("uint256_max_calls_override_receipt", b, vec![max]));
    let mut b = block();
    b.transaction_traces[0].to.clear();
    let mut creation = row.clone();
    creation.insert("tx_to".into(), String::new());
    out.push(equal("contract_creation", b, vec![creation]));
    for (name, from, to, amount) in [("zero_self_transfer", 0x11, 0x11, 0), ("mint", 0, 0x22, 17), ("burn", 0x11, 0, 17)] {
        let mut b = block();
        let log = &mut b.transaction_traces[0].calls[0].logs[0];
        log.topics[1] = padded(from);
        log.topics[2] = padded(to);
        log.data[31] = amount;
        let mut expected = row.clone();
        expected.insert("from".into(), hex(&[from; 20]));
        expected.insert("to".into(), hex(&[to; 20]));
        expected.insert("amount".into(), amount.to_string());
        out.push(equal(name, b, vec![expected]));
    }
    for name in [
        "dirty_address_padding",
        "oversized_address_topic",
        "short_address_topic",
        "erc721",
        "extra_indexed_topic",
        "data_31",
        "data_33",
        "data_64",
    ] {
        let mut b = block();
        let log = &mut b.transaction_traces[0].calls[0].logs[0];
        match name {
            "dirty_address_padding" => log.topics[1][0] = 1,
            "oversized_address_topic" => log.topics[1].push(1),
            "short_address_topic" => {
                log.topics[1].pop();
            }
            "erc721" => {
                log.topics.push(log.data.clone());
                log.data.clear();
            }
            "extra_indexed_topic" => log.topics.push(vec![0; 32]),
            "data_31" => {
                log.data.pop();
            }
            "data_33" => log.data.push(0),
            "data_64" => log.data.extend([0; 32]),
            _ => unreachable!(),
        }
        let legacy_rows = if matches!(name, "dirty_address_padding" | "oversized_address_topic") {
            vec![row.clone()]
        } else {
            vec![]
        };
        out.push(Case {
            name,
            difference: if legacy_rows.is_empty() {
                "evidence_classification"
            } else {
                "abi_acceptance"
            },
            block: b,
            legacy: Some(legacy_rows),
            native: Ok(vec![]),
            evidence: if name == "erc721" { [0, 1, 0, 0, 0] } else { [0, 0, 1, 0, 0] },
        });
    }
    for (name, status) in [
        ("failed_transaction", eth::TransactionTraceStatus::Failed),
        ("reverted_transaction", eth::TransactionTraceStatus::Reverted),
    ] {
        let mut b = block();
        b.transaction_traces[0].status = status.into();
        let mut case = equal(name, b, vec![]);
        case.evidence = [1, 0, 0, 0, 1];
        out.push(case);
    }
    let mut b = block();
    b.transaction_traces[0].calls[0].state_reverted = true;
    let mut case = equal("reverted_call", b, vec![]);
    case.evidence = [1, 0, 0, 0, 1];
    out.push(case);
    let mut b = block();
    b.transaction_traces[0].calls[0].status_failed = true;
    b.transaction_traces[0].calls[0].status_reverted = true;
    out.push(equal("call_status_flags_without_state_reverted", b, vec![row.clone()]));
    for (name, matches) in [("missing_successful_receipt", true), ("missing_receipt_no_matching_logs", false)] {
        let mut b = block();
        b.transaction_traces[0].receipt = Default::default();
        if !matches {
            b.transaction_traces[0].calls[0].logs.clear();
        }
        out.push(Case {
            name,
            difference: "receipt_requirement",
            block: b,
            legacy: None,
            native: Ok(if matches { vec![row.clone()] } else { vec![] }),
            evidence: [usize::from(matches), 0, 0, 0, 0],
        });
    }
    let mut b = block();
    b.transaction_traces[0].status = eth::TransactionTraceStatus::Failed.into();
    b.transaction_traces[0].receipt = Default::default();
    let mut case = equal("missing_failed_receipt", b, vec![]);
    case.evidence = [1, 0, 0, 0, 1];
    out.push(case);
    for name in [
        "base_receipt_only",
        "extended_receipt_only",
        "failed_transaction_without_calls",
        "unknown_transaction_status",
        "unreviewed_producer_v3",
    ] {
        let mut b = block();
        let (legacy, error) = match name {
            "base_receipt_only" => {
                b.detail_level = eth::block::DetailLevel::DetaillevelBase.into();
                b.transaction_traces[0].calls.clear();
                (vec![row.clone()], "Extended blocks required")
            }
            "extended_receipt_only" => {
                b.transaction_traces[0].calls.clear();
                (vec![row.clone()], "incomplete transaction persistence data")
            }
            "failed_transaction_without_calls" => {
                b.transaction_traces[0].status = eth::TransactionTraceStatus::Failed.into();
                b.transaction_traces[0].calls.clear();
                (vec![], "incomplete transaction persistence data")
            }
            "unknown_transaction_status" => {
                b.transaction_traces[0].status = 0.into();
                (vec![], "incomplete transaction persistence data")
            }
            "unreviewed_producer_v3" => {
                b.ver = 3;
                (vec![row.clone()], "Extended producer version not qualified")
            }
            _ => unreachable!(),
        };
        out.push(Case {
            name,
            difference: "source_admission",
            block: b,
            legacy: Some(legacy),
            native: Err(error),
            evidence: [0; 5],
        });
    }
    let mut b = block();
    let mut later = b.transaction_traces[0].clone();
    later.index = 1;
    later.hash = vec![0x88; 32];
    later.calls[0].logs = vec![log(5, 0)];
    later.calls[0].logs[0].block_index = 2;
    b.transaction_traces[0].calls[0].logs = vec![log(42, 0), log(20, 1)];
    b.transaction_traces.push(later);
    out.push(equal(
        "transaction_then_log_order",
        b,
        vec![expected_row(20, 0x77), row.clone(), expected_row(5, 0x88)],
    ));
    let mut b = block();
    b.system_calls = vec![eth::Call {
        logs: vec![log(50, 1)],
        ..Default::default()
    }];
    let mut case = equal("system_scope_with_transaction", b, vec![row]);
    case.difference = "system_evidence";
    case.evidence = [2, 0, 0, 1, 0];
    out.push(case);
    out
}

fn legacy(oracle: &Oracle, block: &eth::Block) -> Option<(Vec<Row>, Vec<u8>)> {
    let events = oracle.map(&block.encode_to_vec())?;
    // Decode the actual map's output with the shared wire-compatible schema.
    proto::pb::erc20::transfers::v1::Events::decode_from_slice(&events).unwrap();
    let clock = legacy_substreams::pb::substreams::Clock {
        id: hex::encode(&block.hash),
        number: block.number,
        timestamp: Some(Default::default()),
    };
    let mut clock = clock;
    clock.timestamp.as_mut().unwrap().seconds = block.header.as_option().unwrap().timestamp.as_option().unwrap().seconds;
    let bytes = oracle.database(&prost::Message::encode_to_vec(&clock), &events);
    let changes = <DatabaseChanges as prost::Message>::decode(bytes.as_slice()).unwrap();
    let rows = changes
        .table_changes
        .into_iter()
        .filter(|change| change.table == "erc20_transfers")
        .map(|change| {
            let row: Row = change
                .fields
                .into_iter()
                .filter(|field| COLUMNS.contains(&field.name.as_str()))
                .map(|field| (field.name, field.new_value))
                .collect();
            assert_eq!(row.len(), COLUMNS.len(), "all eight string columns must be present");
            row
        })
        .collect();
    Some((rows, bytes))
}

fn native(block: &eth::Block, chain_id: u64) -> (Result<Vec<Row>, String>, [usize; 5]) {
    let config = parse(&format!(r#"{{"chain_id":{chain_id},"producer_versions":[4,5],"include_attempted":true}}"#)).unwrap();
    let events = match project(block, &config) {
        Ok(events) => events,
        Err(error) => return (Err(error.to_string()), [0; 5]),
    };
    let evidence = [
        events.transfers.iter().filter(|row| row.shape == LogShape::Erc20).count(),
        events.transfers.iter().filter(|row| row.shape == LogShape::Erc721).count(),
        events.transfers.iter().filter(|row| row.shape == LogShape::Nonstandard).count(),
        events.transfers.iter().filter(|row| row.scope == Scope::SystemCall).count(),
        events.transfers.iter().filter(|row| !row.persisted).count(),
    ];
    let without_attempted = project(
        block,
        &parse(&format!(r#"{{"chain_id":{chain_id},"producer_versions":[4,5],"include_attempted":false}}"#)).unwrap(),
    )
    .unwrap();
    assert!(without_attempted.transfers.iter().all(|row| row.persisted));
    assert_eq!(
        without_attempted.transfers,
        events.transfers.iter().filter(|row| row.persisted).cloned().collect::<Vec<_>>()
    );
    let rows = events
        .transfers
        .into_iter()
        .filter(|row| row.persisted && row.shape == LogShape::Erc20 && row.scope == Scope::Transaction)
        .map(|row| {
            let tx = &block.transaction_traces[usize::try_from(row.transaction_index).unwrap()];
            [
                ("log_address", hex(&row.token)),
                ("log_ordinal", row.ordinal.to_string()),
                ("tx_hash", hex(&row.transaction_hash)),
                ("tx_from", hex(&tx.from)),
                ("tx_to", if tx.to.is_empty() { String::new() } else { hex(&tx.to) }),
                ("from", hex(&row.from)),
                ("to", hex(&row.to)),
                ("amount", row.amount),
            ]
            .into_iter()
            .map(|(name, value)| (name.to_string(), value))
            .collect()
        })
        .collect();
    (Ok(rows), evidence)
}

fn corpus() -> Value {
    let oracle = Oracle::new();
    let cases: Vec<_> = cases().into_iter().map(|case| {
        let legacy = legacy(&oracle, &case.block);
        let (native, evidence) = native(&case.block, 1);
        assert_eq!(legacy.as_ref().map(|(rows, _)| rows), case.legacy.as_ref(), "legacy case {}", case.name);
        assert_eq!(native.as_ref().map_err(String::as_str), case.native.as_ref().map_err(|error| *error), "native case {}", case.name);
        assert_eq!(evidence, case.evidence, "evidence case {}", case.name);
        json!({"name":case.name,"difference":case.difference,"block_hex":hex::encode(case.block.encode_to_vec()),
            "legacy":{"outcome":if legacy.is_some(){"rows"}else{"panic"},"rows":legacy.as_ref().map(|(rows,_)|rows).cloned().unwrap_or_default(),"db_out_hex":legacy.as_ref().map(|(_,bytes)|hex::encode(bytes))},
            "native":{"outcome":if native.is_ok(){"rows"}else{"reject"},"rows":native.as_ref().ok().cloned().unwrap_or_default(),"error":native.err(),
                "evidence":{"erc20":evidence[0],"erc721":evidence[1],"nonstandard":evidence[2],"system":evidence[3],"attempted":evidence[4]}}})
    }).collect();
    json!({"schema_version":1,"policy":{"chain_id":1,"producer_versions":[4,5],"include_attempted":true,"row_selection":"persisted transaction-scope ERC20-shaped transfers"},
        "baseline":{"source_revision":"d0484a4e9fd5654f03e0b06053ed388fafbe5d3e","package_sha256":"5ed10d526d71614d54067afda169b51ee36d0d513cfc84f50c6af2ca5c636d71","erc20_map_sha256":legacy_wasm::MAP_SHA256,"db_out_sha256":legacy_wasm::DB_SHA256,
            "native_reference_revision":"75cc91a606d2bf3d006bd2675e106d02936adf6e","native_core_sha256":hex::encode(Sha256::digest(include_bytes!("../src/lib.rs")))},
        "cases":cases})
}

#[test]
fn historical_package_and_native_projector_match_the_reviewed_corpus() {
    let actual = corpus();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(CORPUS);
    let frozen: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(actual, frozen, "review any oracle or native difference before explicitly regenerating fixtures");
}

#[test]
fn captured_bsc_block_matches_historical_package_rows() {
    let block = eth::Block::decode_from_slice(include_bytes!("fixtures/bsc-122260950.pb")).unwrap();
    let (legacy, _) = legacy(&Oracle::new(), &block).unwrap();
    let (native, evidence) = native(&block, 56);
    assert_eq!(legacy.len(), 110);
    assert_eq!(evidence, [121, 0, 0, 0, 11]);
    assert_eq!(native.unwrap(), legacy);
}

#[test]
#[ignore = "explicitly export only after all hand-written outcome assertions pass"]
fn export_reviewed_corpus() {
    let actual = corpus();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(CORPUS);
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(&actual).unwrap())).unwrap();
}
