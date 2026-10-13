//! `db_out` through the crate's public API: v0.3.3's rows for every table,
//! both address encodings, the errors that replace v0.3.3's panics, the three
//! maps feeding it, the manifest wiring and the embedded v0.3.3 schema.
use buffa_types::google::protobuf::Timestamp;
use evm_transfers::{changes, native_pb, tokens_pb, transfers_pb, Encoding};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use substreams::{pb::substreams::Clock, Hex};
use substreams_database_change::pb::sf::substreams::sink::database::v1::{
    field::UpdateOp,
    table_change::{Operation, PrimaryKey},
    TableChange,
};

const MANIFEST: &str = include_str!("../substreams.yaml");
const BLOCK_HASH: &str = "0xabababababababababababababababababababababababababababababababab";
/// `2^256 - 1`, the largest value of the `UInt256` columns.
const MAX_UINT256: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935";

fn clock() -> Clock {
    Clock {
        id: "ab".repeat(32),
        number: 23_000_000,
        timestamp: buffa::MessageField::some(Timestamp {
            seconds: 1_760_000_042,
            nanos: 7,
            ..Default::default()
        }),
    }
}

fn address(byte: u8) -> Vec<u8> {
    vec![byte; 20]
}

fn hex20(byte: u8) -> String {
    format!("0x{}", format!("{byte:02x}").repeat(20))
}

/// A change as the sink reads it: fields by column name, never by position.
/// Every field is a plain SET (`update_op` 1, the only addition of the 5.0.0
/// encoding over v0.3.3's 3.0.0).
fn columns(change: &TableChange) -> BTreeMap<&str, &str> {
    change
        .fields
        .iter()
        .map(|f| {
            assert_eq!(f.update_op, UpdateOp::UPDATE_OP_SET);
            (f.name.as_str(), f.value.as_str())
        })
        .collect()
}

/// [`columns`] with owned values.
fn owned(change: &TableChange) -> BTreeMap<&str, String> {
    columns(change).into_iter().map(|(k, v)| (k, v.to_string())).collect()
}

fn key(change: &TableChange) -> BTreeMap<&str, &str> {
    match &change.primary_key {
        Some(PrimaryKey::CompositePk(c)) => c.keys.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect(),
        other => panic!("not a composite key: {other:?}"),
    }
}

/// The clock part of every key but the `blocks` key.
fn clock_key(extra: &[(&'static str, &'static str)]) -> BTreeMap<&'static str, &'static str> {
    let mut key = BTreeMap::from([
        ("block_hash", BLOCK_HASH),
        ("block_num", "23000000"),
        ("minute", "29333334"),
        ("timestamp", "1760000042"),
    ]);
    key.extend(extra.iter().copied());
    key
}

/// The clock columns every row carries.
const CLOCK_COLUMNS: [(&str, &str); 4] = [
    ("block_hash", BLOCK_HASH),
    ("block_num", "23000000"),
    ("minute", "29333334"),
    ("timestamp", "1760000042"),
];

fn transfers_log(event: Option<transfers_pb::log::Log>) -> transfers_pb::Log {
    transfers_pb::Log {
        address: address(0x10),
        ordinal: 77,
        topics: vec![vec![0xdd; 32], vec![0x01; 32]],
        data: vec![0x01, 0x02],
        call: buffa::MessageField::some(transfers_pb::Call {
            caller: address(0xca),
            index: 3,
            depth: 1,
            call_type: transfers_pb::CallType::Call.into(),
        }),
        log: event,
    }
}

fn erc20_transfers() -> transfers_pb::Events {
    transfers_pb::Events {
        transactions: vec![
            transfers_pb::Transaction {
                hash: vec![0xaa; 32],
                from: address(0xf0),
                to: Some(address(0x70)),
                nonce: 9,
                gas_price: "100".into(),
                gas_limit: 90_000,
                gas_used: 1_000_000,
                value: "0".into(),
                logs: vec![
                    transfers_log(Some(
                        transfers_pb::Transfer {
                            from: address(1),
                            to: address(2),
                            amount: MAX_UINT256.into(),
                        }
                        .into(),
                    )),
                    // A log without an event writes no row but keeps its position.
                    transfers_log(None),
                    transfers_log(Some(
                        transfers_pb::Approval {
                            owner: address(3),
                            spender: address(4),
                            value: "5".into(),
                        }
                        .into(),
                    )),
                ],
            },
            transfers_pb::Transaction {
                hash: vec![0xbb; 32],
                from: address(0xf1),
                to: None,
                logs: vec![transfers_log(Some(
                    transfers_pb::Transfer {
                        from: address(5),
                        to: address(6),
                        amount: "0".into(),
                    }
                    .into(),
                ))],
                ..Default::default()
            },
        ],
    }
}

fn tokens_log(event: tokens_pb::log::Log) -> tokens_pb::Log {
    tokens_pb::Log {
        address: address(0x20),
        ordinal: 5,
        topics: vec![vec![0xee; 32]],
        data: vec![],
        log: Some(event),
        ..Default::default()
    }
}

fn tokens_events(events: Vec<tokens_pb::log::Log>) -> tokens_pb::Events {
    tokens_pb::Events {
        transactions: vec![tokens_pb::Transaction {
            hash: vec![0xcc; 32],
            from: address(0xf2),
            to: Some(address(0x20)),
            nonce: 1,
            gas_price: "7".into(),
            gas_limit: 50_000,
            gas_used: 40_000,
            value: "0".into(),
            logs: events.into_iter().map(tokens_log).collect(),
        }],
    }
}

fn native_transfers() -> native_pb::Events {
    native_pb::Events {
        block_rewards: vec![
            native_pb::BlockReward {
                miner: address(0xbb),
                value: "2000000000000000000".into(),
                reason: native_pb::Reason::RewardMineBlock.into(),
            },
            native_pb::BlockReward {
                miner: address(0xbc),
                value: "1".into(),
                reason: 99.into(),
            },
        ],
        transactions: vec![
            native_pb::Transaction {
                hash: vec![0xdd; 32],
                from: address(0xf3),
                to: Some(address(0x30)),
                nonce: 2,
                gas_price: "1000000000".into(),
                gas_limit: 21_000,
                gas_used: 21_000,
                value: "42".into(),
                calls: vec![
                    native_pb::Call {
                        index: 4,
                        begin_ordinal: 10,
                        end_ordinal: 20,
                        caller: address(0xc1),
                        address: address(0xd1),
                        value: "7".into(),
                        gas_consumed: 300,
                        gas_limit: 400,
                        depth: 2,
                        parent_index: 3,
                    },
                    native_pb::Call {
                        index: 6,
                        value: "8".into(),
                        caller: address(0xc2),
                        address: address(0xd2),
                        ..Default::default()
                    },
                ],
            },
            native_pb::Transaction {
                hash: vec![0xde; 32],
                from: address(0xf4),
                to: None,
                value: "1".into(),
                gas_price: "0".into(),
                ..Default::default()
            },
        ],
    }
}

/// A token event, its table and its event columns.
type TokenRow = (tokens_pb::log::Log, &'static str, Vec<(&'static str, String)>);

/// Every token event v0.3.3 stores, with its table and event columns.
fn token_rows() -> Vec<TokenRow> {
    use tokens_pb as pb;
    let a = hex20;
    vec![
        (
            pb::WethDeposit {
                dst: address(1),
                wad: "1".into(),
            }
            .into(),
            "weth_deposit",
            vec![("dst", a(1)), ("wad", "1".into())],
        ),
        (
            pb::WethWithdrawal {
                src: address(1),
                wad: "2".into(),
            }
            .into(),
            "weth_withdrawal",
            vec![("src", a(1)), ("wad", "2".into())],
        ),
        (
            pb::UsdcMint {
                minter: address(1),
                to: address(2),
                amount: "3".into(),
            }
            .into(),
            "usdc_mint",
            vec![("minter", a(1)), ("to", a(2)), ("amount", "3".into())],
        ),
        (
            pb::UsdcBurn {
                burner: address(1),
                amount: "4".into(),
            }
            .into(),
            "usdc_burn",
            vec![("burner", a(1)), ("amount", "4".into())],
        ),
        (
            pb::UsdtIssue {
                amount: "5".into(),
                owner: address(1),
            }
            .into(),
            "usdt_issue",
            vec![("owner", a(1)), ("amount", "5".into())],
        ),
        (
            pb::UsdtRedeem {
                amount: "6".into(),
                owner: address(1),
            }
            .into(),
            "usdt_redeem",
            vec![("owner", a(1)), ("amount", "6".into())],
        ),
        (
            pb::UsdtDestroyedBlackFunds {
                black_listed_user: address(1),
                balance: "7".into(),
            }
            .into(),
            "usdt_destroyed_black_funds",
            vec![("black_listed_user", a(1)), ("balance", "7".into())],
        ),
        (pb::UsdtBlockPlaced { user: address(1) }.into(), "usdt_block_placed", vec![("user", a(1))]),
        (pb::UsdtBlockReleased { user: address(1) }.into(), "usdt_block_released", vec![("user", a(1))]),
        (
            pb::UsdtMint {
                destination: address(1),
                amount: "8".into(),
            }
            .into(),
            "usdt_mint",
            vec![("destination", a(1)), ("amount", "8".into())],
        ),
        (
            pb::UsdtDestroyedBlockedFunds {
                blocked_user: address(1),
                balance: "9".into(),
            }
            .into(),
            "usdt_destroyed_blocked_funds",
            vec![("blocked_user", a(1)), ("balance", "9".into())],
        ),
        (
            pb::UsdtNewPrivilegedContract { contract: address(1) }.into(),
            "usdt_new_privileged_contract",
            vec![("contract", a(1))],
        ),
        (
            pb::UsdtRemovedPrivilegedContract { contract: address(1) }.into(),
            "usdt_removed_privileged_contract",
            vec![("contract", a(1))],
        ),
        (
            pb::UsdtLogSwapin {
                txhash: vec![0x5a; 32],
                account: address(1),
                amount: "10".into(),
            }
            .into(),
            "usdt_log_swapin",
            vec![("txhash", format!("0x{}", "5a".repeat(32))), ("account", a(1)), ("amount", "10".into())],
        ),
        (
            pb::UsdtLogSwapout {
                account: address(1),
                bindaddr: address(2),
                amount: "11".into(),
            }
            .into(),
            "usdt_log_swapout",
            vec![("account", a(1)), ("bindaddr", a(2)), ("amount", "11".into())],
        ),
        (
            pb::UsdtLogChangeDcrmOwner {
                old_owner: address(1),
                new_owner: address(2),
                effective_height: "12".into(),
            }
            .into(),
            "usdt_log_change_dcrm_owner",
            vec![("old_owner", a(1)), ("new_owner", a(2)), ("effective_height", "12".into())],
        ),
        (
            pb::WbtcMint {
                to: address(1),
                amount: "13".into(),
            }
            .into(),
            "wbtc_mint",
            vec![("to", a(1)), ("amount", "13".into())],
        ),
        (
            pb::WbtcBurn {
                burner: address(1),
                value: "14".into(),
            }
            .into(),
            "wbtc_burn",
            vec![("burner", a(1)), ("value", "14".into())],
        ),
        (
            pb::SaiMint {
                guy: address(1),
                wad: "15".into(),
            }
            .into(),
            "sai_mint",
            vec![("guy", a(1)), ("wad", "15".into())],
        ),
        (
            pb::SaiBurn {
                guy: address(1),
                wad: "16".into(),
            }
            .into(),
            "sai_burn",
            vec![("guy", a(1)), ("wad", "16".into())],
        ),
        (
            pb::StethSubmitted {
                sender: address(1),
                amount: "17".into(),
                referral: address(2),
            }
            .into(),
            "steth_submitted",
            vec![("sender", a(1)), ("amount", "17".into()), ("referral", a(2))],
        ),
        (
            pb::StethTokenRebased {
                report_timestamp: "1".into(),
                time_elapsed: "2".into(),
                pre_total_shares: "3".into(),
                pre_total_ether: "4".into(),
                post_total_shares: "5".into(),
                post_total_ether: "6".into(),
                shares_minted_as_fees: "7".into(),
            }
            .into(),
            "steth_token_rebased",
            vec![
                ("report_timestamp", "1".into()),
                ("time_elapsed", "2".into()),
                ("pre_total_shares", "3".into()),
                ("pre_total_ether", "4".into()),
                ("post_total_shares", "5".into()),
                ("post_total_ether", "6".into()),
                ("shares_minted_as_fees", "7".into()),
            ],
        ),
        (
            pb::StethTransferShares {
                from: address(1),
                to: address(2),
                shares_value: "18".into(),
            }
            .into(),
            "steth_transfer_shares",
            vec![("from", a(1)), ("to", a(2)), ("shares_value", "18".into())],
        ),
        (
            pb::StethSharesBurnt {
                account: address(1),
                pre_rebase_token_amount: "19".into(),
                post_rebase_token_amount: "20".into(),
                shares_amount: "21".into(),
            }
            .into(),
            "steth_shares_burnt",
            vec![
                ("account", a(1)),
                ("pre_rebase_token_amount", "19".into()),
                ("post_rebase_token_amount", "20".into()),
                ("shares_amount", "21".into()),
            ],
        ),
        (
            pb::StethExternalSharesMinted {
                recipient: address(1),
                amount_of_shares: "22".into(),
            }
            .into(),
            "steth_external_shares_minted",
            vec![("recipient", a(1)), ("amount_of_shares", "22".into())],
        ),
        (
            pb::StethExternalSharesBurnt {
                amount_of_shares: "23".into(),
                owner: address(1),
            }
            .into(),
            "steth_external_shares_burnt",
            vec![("owner", a(1)), ("amount_of_shares", "23".into())],
        ),
    ]
}

/// The token events v0.3.3 stores nowhere.
fn token_events_without_rows() -> Vec<tokens_pb::log::Log> {
    use tokens_pb as pb;
    vec![
        pb::Pause {}.into(),
        pb::Unpause {}.into(),
        pb::OwnershipTransferred::default().into(),
        pb::UsdcBlacklisted::default().into(),
        pb::UsdcBlacklisterChanged::default().into(),
        pb::UsdcMasterMinterChanged::default().into(),
        pb::UsdcMinterConfigured::default().into(),
        pb::UsdcMinterRemoved::default().into(),
        pb::UsdcPauserChanged::default().into(),
        pb::UsdcRescuerChanged::default().into(),
        pb::UsdcUnBlacklisted::default().into(),
        pb::UsdcAuthorizationCanceled::default().into(),
        pb::UsdcAuthorizationUsed::default().into(),
        pb::UsdtDeprecate::default().into(),
        pb::UsdtParams::default().into(),
        pb::UsdtAddedBlackList::default().into(),
        pb::UsdtRemovedBlackList::default().into(),
        pb::WbtcMintFinished {}.into(),
        pb::WbtcOwnershipRenounced::default().into(),
        pb::SaiLogSetAuthority::default().into(),
        pb::SaiLogSetOwner::default().into(),
        pb::StethUnbuffered::default().into(),
        pb::StethExternalEtherTransferredToBuffer::default().into(),
        pb::StethExternalBadDebtInternalized::default().into(),
        pb::StethMaxExternalRatioBPSet::default().into(),
        pb::StethCLValidatorsUpdated::default().into(),
        pb::StethDepositedValidatorsChanged::default().into(),
        pb::StethETHDistributed::default().into(),
        pb::StethInternalShareRateUpdated::default().into(),
        pb::StethStakingPaused {}.into(),
        pb::StethStakingResumed {}.into(),
        pb::StethStakingLimitSet::default().into(),
        pb::StethStakingLimitRemoved {}.into(),
        pb::StethELRewardsReceived::default().into(),
        pb::StethWithdrawalsReceived::default().into(),
        pb::StethLidoLocatorSet::default().into(),
    ]
}

#[test]
fn rows_follow_the_v0_3_3_format() {
    let tokens = tokens_events(vec![
        tokens_pb::WethDeposit {
            dst: address(1),
            wad: "1".into(),
        }
        .into(),
        tokens_pb::Pause {}.into(),
        tokens_pb::UsdtIssue {
            amount: "5".into(),
            owner: address(0xee),
        }
        .into(),
    ]);
    let changes = changes(&clock(), &erc20_transfers(), &tokens, &native_transfers(), Encoding::Hex)
        .unwrap()
        .table_changes;

    let tables: Vec<&str> = changes.iter().map(|c| c.table.as_str()).collect();
    assert_eq!(
        tables,
        [
            "erc20_transfers",
            "erc20_approvals",
            "erc20_transfers",
            "weth_deposit",
            "usdt_issue",
            "block_rewards",
            "block_rewards",
            "transactions",
            "calls",
            "calls",
            "transactions",
            "blocks",
        ]
    );
    for (i, change) in changes.iter().enumerate() {
        assert_eq!(change.ordinal, i as u64);
        assert_eq!(change.operation, Operation::OPERATION_CREATE);
        // Sorted by name, so the bytes do not depend on `HashMap` order.
        assert!(change.fields.iter().map(|f| &f.name).is_sorted());
        let cols = columns(change);
        for (name, value) in CLOCK_COLUMNS {
            assert_eq!(cols[name], value, "{} {name}", change.table);
        }
    }

    // ERC-20: positions in the map's output key the rows; a log without an
    // event keeps its position.
    let log_tx = BTreeMap::from([
        ("tx_index", "0"),
        ("tx_hash", "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ("tx_from", "0xf0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0"),
        ("tx_to", "0x7070707070707070707070707070707070707070"),
        ("tx_nonce", "9"),
        ("tx_gas_price", "100"),
        ("tx_gas_limit", "90000"),
        ("tx_gas_used", "1000000"),
        ("tx_value", "0"),
    ]);
    let log_columns = |log_index: &'static str, event: &[(&'static str, &str)]| -> BTreeMap<&'static str, String> {
        let mut out: BTreeMap<&str, String> = CLOCK_COLUMNS.iter().map(|(k, v)| (*k, v.to_string())).collect();
        out.extend(log_tx.iter().map(|(k, v)| (*k, v.to_string())));
        out.extend([
            ("log_index", log_index.to_string()),
            ("log_address", hex20(0x10)),
            ("log_ordinal", "77".into()),
            ("log_topics", format!("0x{},0x{}", "dd".repeat(32), "01".repeat(32))),
            ("log_data", "0x0102".into()),
        ]);
        out.extend(event.iter().map(|(k, v)| (*k, v.to_string())));
        out
    };

    assert_eq!(key(&changes[0]), clock_key(&[("tx_index", "0"), ("log_index", "0")]));
    assert_eq!(
        owned(&changes[0]),
        log_columns("0", &[("from", &hex20(1)), ("to", &hex20(2)), ("amount", MAX_UINT256)])
    );
    assert_eq!(key(&changes[1]), clock_key(&[("tx_index", "0"), ("log_index", "2")]));
    assert_eq!(
        owned(&changes[1]),
        log_columns("2", &[("owner", &hex20(3)), ("spender", &hex20(4)), ("value", "5")])
    );
    // A contract creation: empty `tx_to`; defaults are written as they are.
    assert_eq!(key(&changes[2]), clock_key(&[("tx_index", "1"), ("log_index", "0")]));
    let creation = columns(&changes[2]);
    assert_eq!((creation["tx_to"], creation["tx_gas_price"], creation["tx_value"]), ("", "", ""));
    assert_eq!((creation["amount"], creation["tx_nonce"]), ("0", "0"));

    // Tokens: `Pause` writes no row but keeps its position.
    assert_eq!(key(&changes[3]), clock_key(&[("tx_index", "0"), ("log_index", "0")]));
    assert_eq!(key(&changes[4]), clock_key(&[("tx_index", "0"), ("log_index", "2")]));
    let issue = columns(&changes[4]);
    assert_eq!((issue["owner"], issue["amount"], issue["log_index"]), (hex20(0xee).as_str(), "5", "2"));
    assert_eq!(
        (issue["tx_gas_used"], issue["tx_gas_price"], issue["log_topics"]),
        ("40000", "7", format!("0x{}", "ee".repeat(32)).as_str())
    );
    assert_eq!(issue["log_data"], "0x");

    // Native.
    assert_eq!(key(&changes[5]), clock_key(&[("index", "0")]));
    let reward = columns(&changes[5]);
    assert_eq!(
        (reward["index"], reward["miner"], reward["value"], reward["reason"]),
        ("0", hex20(0xbb).as_str(), "2000000000000000000", "REASON_REWARD_MINE_BLOCK")
    );
    assert_eq!(reward.len(), 8);
    // A reason outside the enum is written REASON_UNKNOWN.
    assert_eq!(columns(&changes[6])["reason"], "REASON_UNKNOWN");

    assert_eq!(key(&changes[7]), clock_key(&[("tx_index", "0")]));
    let tx = columns(&changes[7]);
    assert_eq!(tx.len(), 4 + 9);
    assert_eq!((tx["tx_value"], tx["tx_gas_used"], tx["tx_to"]), ("42", "21000", hex20(0x30).as_str()));

    assert_eq!(key(&changes[8]), clock_key(&[("tx_index", "0"), ("call_index", "0")]));
    let call: BTreeMap<&str, String> = owned(&changes[8]);
    let mut expected: BTreeMap<&str, String> = columns(&changes[7]).into_iter().map(|(k, v)| (k, v.to_string())).collect();
    expected.extend([
        ("call_index", "0".to_string()),
        ("call_begin_ordinal", "10".into()),
        ("call_end_ordinal", "20".into()),
        ("call_parent_index", "3".into()),
        ("call_caller", hex20(0xc1)),
        ("call_address", hex20(0xd1)),
        ("call_value", "7".into()),
        ("call_gas_consumed", "300".into()),
        ("call_gas_limit", "400".into()),
        ("call_depth", "2".into()),
        // Production's native map never sets `call_type`.
        ("call_type", "CALL_TYPE_UNSPECIFIED".into()),
    ]);
    assert_eq!(call, expected);
    // `call_index` is the position in `Transaction.calls`, not `Call.index`.
    assert_eq!(key(&changes[9]), clock_key(&[("tx_index", "0"), ("call_index", "1")]));
    assert_eq!(columns(&changes[9])["call_index"], "1");

    let creation = columns(&changes[10]);
    assert_eq!((creation["tx_index"], creation["tx_to"]), ("1", ""));

    let blocks = &changes[11];
    assert_eq!(key(blocks), BTreeMap::from([("block_num", "23000000")]));
    assert_eq!(columns(blocks), BTreeMap::from(CLOCK_COLUMNS));
}

#[test]
fn every_stored_token_event_writes_its_v0_3_3_table() {
    for (event, table, event_columns) in token_rows() {
        let changes = changes(&clock(), &Default::default(), &tokens_events(vec![event]), &Default::default(), Encoding::Hex)
            .unwrap()
            .table_changes;
        let tables: Vec<&str> = changes.iter().map(|c| c.table.as_str()).collect();
        assert_eq!(tables, [table, "blocks"]);
        let cols = columns(&changes[0]);
        // 4 clock, 9 transaction and 5 log columns, then the event's.
        assert_eq!(cols.len(), 18 + event_columns.len(), "{table}");
        for (name, value) in &event_columns {
            assert_eq!(cols[name], value.as_str(), "{table} {name}");
        }
        assert_eq!(key(&changes[0]), clock_key(&[("tx_index", "0"), ("log_index", "0")]));
    }
}

#[test]
fn token_events_v0_3_3_did_not_store_write_nothing() {
    let changes = changes(
        &clock(),
        &Default::default(),
        &tokens_events(token_events_without_rows()),
        &Default::default(),
        Encoding::Hex,
    )
    .unwrap();
    assert!(changes.table_changes.is_empty());
}

#[test]
fn tron_base58_encodes_addresses_only() {
    let tokens = tokens_events(vec![tokens_pb::UsdtLogSwapin {
        txhash: vec![0x5a; 32],
        account: address(1),
        amount: "1".into(),
    }
    .into()]);
    let changes = changes(&clock(), &erc20_transfers(), &tokens, &native_transfers(), Encoding::TronBase58)
        .unwrap()
        .table_changes;

    let transfer = columns(&changes[0]);
    // 0x0101.. and 0x0202.. as Tron addresses.
    let base58 = |byte: u8| {
        let mut data = vec![0x41];
        data.extend(address(byte));
        let checksum = Sha256::digest(Sha256::digest(&data));
        data.extend_from_slice(&checksum[..4]);
        bs58_encode(&data)
    };
    assert_eq!((transfer["from"], transfer["to"]), (base58(1).as_str(), base58(2).as_str()));
    assert_eq!(
        (transfer["tx_from"], transfer["tx_to"], transfer["log_address"]),
        (base58(0xf0).as_str(), base58(0x70).as_str(), base58(0x10).as_str())
    );
    // Hashes, topics and data stay hex.
    assert_eq!(transfer["tx_hash"], format!("0x{}", "aa".repeat(32)));
    assert!(transfer["log_topics"].starts_with("0xdddd"));
    assert_eq!(transfer["log_data"], "0x0102");

    // v0.3.3 encoded the swap-in `txhash` like an address: 32 bytes are no
    // Tron address, so it is empty.
    let swapin = changes.iter().find(|c| c.table == "usdt_log_swapin").unwrap();
    assert_eq!((columns(swapin)["txhash"], columns(swapin)["account"]), ("", base58(1).as_str()));

    let reward = changes.iter().find(|c| c.table == "block_rewards").unwrap();
    assert_eq!(columns(reward)["miner"], base58(0xbb));
    let call = changes.iter().find(|c| c.table == "calls").unwrap();
    assert_eq!(
        (columns(call)["call_caller"], columns(call)["call_address"]),
        (base58(0xc1).as_str(), base58(0xd1).as_str())
    );
}

/// Base58 (Bitcoin alphabet) of `data`, to check the encoding independently.
fn bs58_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut digits: Vec<u8> = Vec::new();
    for &byte in data {
        let mut carry = byte as u32;
        for digit in digits.iter_mut() {
            carry += (*digit as u32) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let zeros = data.iter().take_while(|b| **b == 0).count();
    std::iter::repeat_n('1', zeros)
        .chain(digits.iter().rev().map(|d| ALPHABET[*d as usize] as char))
        .collect()
}

#[test]
fn a_block_without_rows_writes_nothing() {
    let mut no_time = clock();
    no_time.timestamp = buffa::MessageField::none();
    for clock in [clock(), no_time] {
        let empty = changes(&clock, &Default::default(), &Default::default(), &Default::default(), Encoding::Hex).unwrap();
        assert!(empty.table_changes.is_empty());
        // Logs without a stored event write nothing either.
        let quiet = transfers_pb::Events {
            transactions: vec![transfers_pb::Transaction {
                logs: vec![transfers_log(None)],
                ..Default::default()
            }],
        };
        let changes = changes(
            &clock,
            &quiet,
            &tokens_events(vec![tokens_pb::Pause {}.into()]),
            &Default::default(),
            Encoding::Hex,
        )
        .unwrap();
        assert!(changes.table_changes.is_empty());
    }
}

#[test]
fn rows_without_a_clock_timestamp_fail_the_block() {
    let mut no_time = clock();
    no_time.timestamp = buffa::MessageField::none();
    let native_only = native_pb::Events {
        block_rewards: vec![native_pb::BlockReward::default()],
        ..Default::default()
    };
    for (erc20, native) in [(erc20_transfers(), Default::default()), (Default::default(), native_only)] {
        let err = changes(&no_time, &erc20, &Default::default(), &native, Encoding::Hex).unwrap_err();
        assert_eq!(err.to_string(), "clock.timestamp is required");
    }
}

#[test]
fn minute_is_seconds_over_60_truncated() {
    let mut clock = clock();
    for (seconds, minute) in [(59, "0"), (60, "1"), (-61, "-1"), (0, "0")] {
        clock.timestamp = buffa::MessageField::some(Timestamp { seconds, ..Default::default() });
        let changes = changes(&clock, &erc20_transfers(), &Default::default(), &Default::default(), Encoding::Hex).unwrap();
        let cols = columns(&changes.table_changes[0]);
        assert_eq!((cols["timestamp"], cols["minute"]), (seconds.to_string().as_str(), minute));
        assert_eq!(key(&changes.table_changes[0])["minute"], minute);
    }
}

/// A synthetic block through the three maps and `db_out`, as the manifest
/// wires them: `tx_index` is the position in each map's own output.
#[test]
fn the_three_maps_feed_db_out() {
    use buffa::MessageField;
    use substreams_ethereum::pb::eth::v2 as eth;

    let transfer_topic = Hex::decode("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef").unwrap();
    let word = |byte: u8| [vec![0u8; 12], address(byte)].concat();
    let trx = |hash: u8, value: Vec<u8>, logs: Vec<eth::Log>| eth::TransactionTrace {
        hash: vec![hash; 32],
        from: address(0xf0),
        to: address(0x70),
        value: MessageField::some(eth::BigInt { bytes: value }),
        status: eth::TransactionTraceStatus::Succeeded.into(),
        calls: vec![eth::Call {
            call_type: eth::CallType::Call.into(),
            logs,
            ..Default::default()
        }],
        receipt: MessageField::some(eth::TransactionReceipt {
            cumulative_gas_used: 777,
            ..Default::default()
        }),
        ..Default::default()
    };
    let transfer = eth::Log {
        address: address(0x10),
        topics: vec![transfer_topic, word(1), word(2)],
        data: [vec![0u8; 31], vec![9]].concat(),
        ordinal: 3,
        ..Default::default()
    };
    // A native-only transaction, then one with an ERC-20 transfer and no value.
    let block = eth::Block {
        transaction_traces: vec![trx(1, vec![5], vec![]), trx(2, vec![], vec![transfer])],
        ..Default::default()
    };

    let erc20 = erc20_transfers::project(&block).unwrap();
    let tokens = erc20_tokens::project(&block, &mut erc20_tokens::EthCallOwner(|_: &erc20_tokens::rpc::RpcCalls| unreachable!())).unwrap();
    let native = native_transfers::project(&block);
    let changes = changes(&clock(), &erc20, &tokens, &native, Encoding::Hex).unwrap().table_changes;

    let rows: Vec<(&str, &str, &str)> = changes
        .iter()
        .map(|c| {
            let cols = columns(c);
            (
                c.table.as_str(),
                cols.get("tx_index").copied().unwrap_or(""),
                cols.get("tx_hash").copied().unwrap_or(""),
            )
        })
        .collect();
    let hash = |byte: u8| format!("0x{}", format!("{byte:02x}").repeat(32));
    assert_eq!(
        rows,
        [
            // The second transaction is the first of the ERC-20 map's output.
            ("erc20_transfers", "0", hash(2).as_str()),
            ("transactions", "0", hash(1).as_str()),
            ("blocks", "", ""),
        ]
    );
    let transfer = columns(&changes[0]);
    assert_eq!((transfer["amount"], transfer["tx_gas_used"], transfer["log_ordinal"]), ("9", "777", "3"));
}

/// The handler takes `(params, Clock, erc20_transfers, erc20_tokens,
/// native_transfers)`: the manifest must feed the inputs in that order, with
/// the `hex` default, import the three local maps and embed the schema.
#[test]
fn manifest_wires_db_out_and_the_schema() {
    assert!(MANIFEST.contains(
        "    inputs:\n      - params: string\n      - source: sf.substreams.v1.Clock\n      - map: erc20_transfers:map_events\n      - map: erc20_tokens:map_events\n      - map: native_transfers:map_events\n    output:\n      type: proto:sf.substreams.sink.database.v1.DatabaseChanges\n"
    ));
    for import in [
        "\n  erc20_transfers: ../erc20/transfers/substreams.yaml\n",
        "\n  erc20_tokens: ../erc20/tokens/substreams.yaml\n",
        "\n  native_transfers: ../native/transfers/substreams.yaml\n",
    ] {
        assert!(MANIFEST.contains(import), "{import}");
    }
    assert!(MANIFEST.contains("\nparams:\n  db_out: \"hex\""));
    assert!(MANIFEST.contains("sink:\n  module: db_out\n  type: sf.substreams.sink.sql.v1.Service\n  config:\n    schema: \"./clickhouse/schema.sql\""));
    assert!(MANIFEST.contains("    engine: clickhouse\n"));
    assert!(MANIFEST.contains("\nnetwork: mainnet\n"));
    assert!(MANIFEST.contains("\n  name: evm_transfers\n  version: v0.8.0\n"));
}

/// The twelve SQL files are v0.3.3's, byte for byte, and `make schema` turns
/// them into the schema embedded in production `evm-clickhouse-transfers-v0.3.3`.
#[test]
fn schema_is_the_v0_3_3_schema() {
    const FILES: [(&str, &str); 12] = [
        ("schema.0.blocks.sql", "1156819b835563c8a87ec43f8d674092f04f4eb71e501189e0d72e89527f8af3"),
        ("schema.0.templates.sql", "f5fc2b4c9cfaff063c04d5e24519a52f28958f909ebf29a077aa758863a25e8c"),
        (
            "schema.1.table.erc20_transfers.sql",
            "323ecb8d732202ddb1a1edef5675f57935c1fe763c5cd4d6569a60a7e7daf4d8",
        ),
        (
            "schema.1.table.native_transfers.sql",
            "cf3309dd1dfb30c50233d1bc2e9d1d4b1558368e18a190c6a2fba5a36af9ad92",
        ),
        ("schema.1.table.sai.sql", "67e29a00c9bc2e028f14ad53460aa275f521359da914da8822c8e7afb44d41ec"),
        ("schema.1.table.steth.sql", "c93ada3cd3007d7cfb07e75a09348b14265693f4e26f2d5fe3253e7c18d488f8"),
        ("schema.1.table.usdc.sql", "7df7616245db008dca83c062c47ac748d33b9531cc99790563ca4b40e6704d52"),
        ("schema.1.table.usdt.sql", "ebde01f0a2591e16290bf113e56954848ceb9ab207a9bc6744543f55c8e205b7"),
        ("schema.1.table.wbtc.sql", "6c9e5411e8dada2294e282bd8f5cbefcbcc06dec7a842899dd082979b715cb08"),
        ("schema.1.table.weth.sql", "0dc6171e3b117372587025ea6a261f05873920048b6d418e5bde6dea2552ee55"),
        (
            "schema.2.mv.native_transfers.sql",
            "60657d1d57db29eaf3ac689da1326cf43fbd7007706629b0b2228899323fbb3a",
        ),
        ("schema.2.mv.transfers.sql", "440234f8bc9fe3a400ee9910105707c1b8567f0a2fd450817e8475bf274f030a"),
    ];
    let hex = |bytes: &[u8]| Hex::encode(Sha256::digest(bytes));
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("clickhouse");

    let mut parts: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("schema.") && n.ends_with(".sql") && n != "schema.sql")
        .collect();
    parts.sort();
    assert_eq!(parts, FILES.map(|(name, _)| name));

    let mut schema = b"-- This file is generated. Do not edit.\n\n".to_vec();
    for (name, sha256) in FILES {
        let sql = std::fs::read(dir.join(name)).unwrap();
        assert_eq!(hex(&sql), sha256, "{name}");
        schema.extend_from_slice(&sql);
        schema.push(b'\n');
    }
    assert_eq!(schema.len(), 32_418);
    assert_eq!(hex(&schema), "6ef74c5afc5401b7eda7a7d4cb979d79acc1ffa9c60db73a8e499fc8fe34c543");
}

/// The tables `db_out` writes are the ones the schema creates, and the
/// columns of a log row are those of `TEMPLATE_LOG` without the materialized
/// topics.
#[test]
fn rows_name_tables_and_columns_of_the_schema() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("clickhouse");
    let sql: String = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_str().unwrap() != "schema.sql")
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    let mut tables: Vec<&str> = token_rows().iter().map(|(_, table, _)| *table).collect();
    tables.extend(["erc20_transfers", "erc20_approvals", "block_rewards", "transactions", "calls", "blocks"]);
    for table in tables {
        assert!(sql.contains(&format!("CREATE TABLE IF NOT EXISTS {table} ")), "{table}");
    }
    let changes = changes(&clock(), &erc20_transfers(), &Default::default(), &native_transfers(), Encoding::Hex)
        .unwrap()
        .table_changes;
    for change in &changes {
        for name in columns(change).keys() {
            let column = format!(" {name} ");
            let quoted = format!("`{name}`");
            assert!(sql.contains(&column) || sql.contains(&quoted), "{} {name}", change.table);
        }
    }
}

#[test]
fn database_change_descriptor_is_the_5_0_0_source() {
    let proto = include_bytes!("../proto/sf/substreams/sink/database/v1/database.proto");
    assert_eq!(
        Hex::encode(Sha256::digest(proto)),
        "122db4ce295c1e4e24d66a55d5796cdb950599ffeeb820684d61f630721094f7"
    );
    assert!(MANIFEST.contains("protobuf:\n  files:\n    - sf/substreams/sink/database/v1/database.proto\n  importPaths:\n    - ./proto\n"));
}
