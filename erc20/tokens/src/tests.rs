//! Synthetic blocks through [`project`]: topic 0 is the Keccak-256 of each
//! event's canonical signature, so every case also checks its decoder's
//! signature. The `owner()` lookup is injected.
use super::*;
use buffa::{Message, MessageField};
use eth::{Block, Call, CallType, Log, TransactionReceipt, TransactionTrace, TransactionTraceStatus};
use tiny_keccak::{Hasher, Keccak};

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

/// A 32-byte word: an address left-padded, or a number.
fn word_address(byte: u8) -> Vec<u8> {
    let mut out = vec![0; 12];
    out.extend(address(byte));
    out
}

fn word(value: u128) -> Vec<u8> {
    let mut out = vec![0; 16];
    out.extend(value.to_be_bytes());
    out
}

fn event_log(signature: &str, indexed: Vec<Vec<u8>>, data: Vec<Vec<u8>>) -> Log {
    let mut topics = vec![keccak(signature)];
    topics.extend(indexed);
    Log {
        address: address(0x70),
        topics,
        data: data.concat(),
        ordinal: 1,
        ..Default::default()
    }
}

fn call(index: u32, logs: Vec<Log>) -> Call {
    Call {
        index,
        depth: 1,
        caller: address(0xca),
        call_type: CallType::Call.into(),
        logs,
        ..Default::default()
    }
}

fn trx(calls: Vec<Call>, receipt_logs: Vec<Log>) -> TransactionTrace {
    TransactionTrace {
        hash: vec![0xaa; 32],
        from: address(0xf0),
        to: address(0x70),
        status: TransactionTraceStatus::Succeeded.into(),
        calls,
        receipt: MessageField::some(TransactionReceipt {
            cumulative_gas_used: 777,
            logs: receipt_logs,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn block(transaction_traces: Vec<TransactionTrace>) -> Block {
    Block {
        transaction_traces,
        ..Default::default()
    }
}

/// An `owner()` lookup that answers from a list and records its questions.
#[derive(Default)]
struct Owners {
    answers: Vec<Result<Option<Vec<u8>>, String>>,
    asked: Vec<Vec<u8>>,
}

impl OwnerLookup for Owners {
    fn owner(&mut self, contract: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        self.asked.push(contract.to_vec());
        match self.answers.remove(0) {
            Ok(answer) => Ok(answer),
            Err(message) => Err(Error::msg(message)),
        }
    }
}

/// The owner lookup is not asked for anything.
struct NoOwners;

impl OwnerLookup for NoOwners {
    fn owner(&mut self, contract: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        panic!("owner() asked for {contract:?}")
    }
}

/// The events of one log, emitted in an Extended transaction's call.
fn events_of(log: Log) -> Vec<pb::log::Log> {
    let events = project(&block(vec![trx(vec![call(0, vec![log])], vec![])]), &mut NoOwners).unwrap();
    events
        .transactions
        .iter()
        .flat_map(|tx| &tx.logs)
        .map(|l| l.log.clone().expect("every log carries an event"))
        .collect()
}

/// A signature, its indexed topics, its data words and the event it decodes to.
type Case = (&'static str, Vec<Vec<u8>>, Vec<Vec<u8>>, pb::log::Log);

#[test]
fn every_event_decodes_from_its_canonical_signature() {
    let max = vec![0xff; 32];
    let max_decimal = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    let cases: Vec<Case> = vec![
        // WETH
        (
            "Deposit(address,uint256)",
            vec![word_address(1)],
            vec![word(5)],
            pb::WethDeposit {
                dst: address(1),
                wad: "5".into(),
            }
            .into(),
        ),
        (
            "Withdrawal(address,uint256)",
            vec![word_address(1)],
            vec![max.clone()],
            pb::WethWithdrawal {
                src: address(1),
                wad: max_decimal.into(),
            }
            .into(),
        ),
        // USDC
        (
            "Mint(address,address,uint256)",
            vec![word_address(1), word_address(2)],
            vec![word(3)],
            pb::UsdcMint {
                minter: address(1),
                to: address(2),
                amount: "3".into(),
            }
            .into(),
        ),
        (
            "Blacklisted(address)",
            vec![word_address(1)],
            vec![],
            pb::UsdcBlacklisted { account: address(1) }.into(),
        ),
        (
            "BlacklisterChanged(address)",
            vec![word_address(1)],
            vec![],
            pb::UsdcBlacklisterChanged { new_blacklister: address(1) }.into(),
        ),
        (
            "MasterMinterChanged(address)",
            vec![word_address(1)],
            vec![],
            pb::UsdcMasterMinterChanged { new_master_minter: address(1) }.into(),
        ),
        (
            "MinterConfigured(address,uint256)",
            vec![word_address(1)],
            vec![word(9)],
            pb::UsdcMinterConfigured {
                minter: address(1),
                minter_allowed_amount: "9".into(),
            }
            .into(),
        ),
        (
            "MinterRemoved(address)",
            vec![word_address(1)],
            vec![],
            pb::UsdcMinterRemoved { old_minter: address(1) }.into(),
        ),
        (
            "OwnershipTransferred(address,address)",
            vec![],
            vec![word_address(1), word_address(2)],
            pb::OwnershipTransferred {
                previous_owner: address(1),
                new_owner: address(2),
            }
            .into(),
        ),
        ("Pause()", vec![], vec![], pb::Pause {}.into()),
        (
            "PauserChanged(address)",
            vec![word_address(1)],
            vec![],
            pb::UsdcPauserChanged { new_address: address(1) }.into(),
        ),
        (
            "RescuerChanged(address)",
            vec![word_address(1)],
            vec![],
            pb::UsdcRescuerChanged { new_rescuer: address(1) }.into(),
        ),
        (
            "UnBlacklisted(address)",
            vec![word_address(1)],
            vec![],
            pb::UsdcUnBlacklisted { account: address(1) }.into(),
        ),
        ("Unpause()", vec![], vec![], pb::Unpause {}.into()),
        (
            "AuthorizationCanceled(address,bytes32)",
            vec![word_address(1), vec![0xab; 32]],
            vec![],
            pb::UsdcAuthorizationCanceled {
                authorizer: address(1),
                nonce: vec![0xab; 32],
            }
            .into(),
        ),
        (
            "AuthorizationUsed(address,bytes32)",
            vec![word_address(1), vec![0xcd; 32]],
            vec![],
            pb::UsdcAuthorizationUsed {
                authorizer: address(1),
                nonce: vec![0xcd; 32],
            }
            .into(),
        ),
        // USDT (TetherToken v0.4.18); Issue and Redeem need the owner lookup.
        (
            "Deprecate(address)",
            vec![],
            vec![word_address(1)],
            pb::UsdtDeprecate { new_address: address(1) }.into(),
        ),
        (
            "Params(uint256,uint256)",
            vec![],
            vec![word(10), word(20)],
            pb::UsdtParams {
                fee_basis_points: "10".into(),
                max_fee: "20".into(),
            }
            .into(),
        ),
        (
            "DestroyedBlackFunds(address,uint256)",
            vec![],
            vec![word_address(1), word(7)],
            pb::UsdtDestroyedBlackFunds {
                black_listed_user: address(1),
                balance: "7".into(),
            }
            .into(),
        ),
        (
            "AddedBlackList(address)",
            vec![],
            vec![word_address(1)],
            pb::UsdtAddedBlackList { user: address(1) }.into(),
        ),
        (
            "RemovedBlackList(address)",
            vec![],
            vec![word_address(1)],
            pb::UsdtRemovedBlackList { user: address(1) }.into(),
        ),
        // USDT (TetherToken v0.8.4)
        (
            "BlockPlaced(address)",
            vec![word_address(1)],
            vec![],
            pb::UsdtBlockPlaced { user: address(1) }.into(),
        ),
        (
            "BlockReleased(address)",
            vec![word_address(1)],
            vec![],
            pb::UsdtBlockReleased { user: address(1) }.into(),
        ),
        (
            "DestroyedBlockedFunds(address,uint256)",
            vec![word_address(1)],
            vec![word(8)],
            pb::UsdtDestroyedBlockedFunds {
                blocked_user: address(1),
                balance: "8".into(),
            }
            .into(),
        ),
        (
            "NewPrivilegedContract(address)",
            vec![word_address(1)],
            vec![],
            pb::UsdtNewPrivilegedContract { contract: address(1) }.into(),
        ),
        (
            "RemovedPrivilegedContract(address)",
            vec![word_address(1)],
            vec![],
            pb::UsdtRemovedPrivilegedContract { contract: address(1) }.into(),
        ),
        // USDT (swap asset)
        (
            "LogSwapin(bytes32,address,uint256)",
            vec![vec![0x5a; 32], word_address(1)],
            vec![word(4)],
            pb::UsdtLogSwapin {
                txhash: vec![0x5a; 32],
                account: address(1),
                amount: "4".into(),
            }
            .into(),
        ),
        (
            "LogSwapout(address,address,uint256)",
            vec![word_address(1), word_address(2)],
            vec![word(4)],
            pb::UsdtLogSwapout {
                account: address(1),
                bindaddr: address(2),
                amount: "4".into(),
            }
            .into(),
        ),
        (
            "LogChangeDCRMOwner(address,address,uint256)",
            vec![word_address(1), word_address(2), word(100)],
            vec![],
            pb::UsdtLogChangeDcrmOwner {
                old_owner: address(1),
                new_owner: address(2),
                effective_height: "100".into(),
            }
            .into(),
        ),
        // WBTC
        ("MintFinished()", vec![], vec![], pb::WbtcMintFinished {}.into()),
        (
            "OwnershipRenounced(address)",
            vec![word_address(1)],
            vec![],
            pb::WbtcOwnershipRenounced { previous_owner: address(1) }.into(),
        ),
        // SAI
        (
            "LogSetAuthority(address)",
            vec![word_address(1)],
            vec![],
            pb::SaiLogSetAuthority { authority: address(1) }.into(),
        ),
        (
            "LogSetOwner(address)",
            vec![word_address(1)],
            vec![],
            pb::SaiLogSetOwner { owner: address(1) }.into(),
        ),
        // stETH
        (
            "Submitted(address,uint256,address)",
            vec![word_address(1)],
            vec![word(5), word_address(2)],
            pb::StethSubmitted {
                sender: address(1),
                amount: "5".into(),
                referral: address(2),
            }
            .into(),
        ),
        ("Unbuffered(uint256)", vec![], vec![word(5)], pb::StethUnbuffered { amount: "5".into() }.into()),
        (
            "TokenRebased(uint256,uint256,uint256,uint256,uint256,uint256,uint256)",
            vec![word(1)],
            vec![word(2), word(3), word(4), word(5), word(6), word(7)],
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
        ),
        (
            "TransferShares(address,address,uint256)",
            vec![word_address(1), word_address(2)],
            vec![word(3)],
            pb::StethTransferShares {
                from: address(1),
                to: address(2),
                shares_value: "3".into(),
            }
            .into(),
        ),
        (
            "SharesBurnt(address,uint256,uint256,uint256)",
            vec![word_address(1)],
            vec![word(2), word(3), word(4)],
            pb::StethSharesBurnt {
                account: address(1),
                pre_rebase_token_amount: "2".into(),
                post_rebase_token_amount: "3".into(),
                shares_amount: "4".into(),
            }
            .into(),
        ),
        (
            "ExternalSharesMinted(address,uint256)",
            vec![word_address(1)],
            vec![word(2)],
            pb::StethExternalSharesMinted {
                recipient: address(1),
                amount_of_shares: "2".into(),
            }
            .into(),
        ),
        (
            // The owner is the emitting call's caller.
            "ExternalSharesBurnt(uint256)",
            vec![],
            vec![word(2)],
            pb::StethExternalSharesBurnt {
                amount_of_shares: "2".into(),
                owner: address(0xca),
            }
            .into(),
        ),
        (
            "ExternalEtherTransferredToBuffer(uint256)",
            vec![],
            vec![word(2)],
            pb::StethExternalEtherTransferredToBuffer { amount: "2".into() }.into(),
        ),
        (
            "ExternalBadDebtInternalized(uint256)",
            vec![],
            vec![word(2)],
            pb::StethExternalBadDebtInternalized { amount_of_shares: "2".into() }.into(),
        ),
        (
            "MaxExternalRatioBPSet(uint256)",
            vec![],
            vec![word(2)],
            pb::StethMaxExternalRatioBPSet {
                max_external_ratio_bp: "2".into(),
            }
            .into(),
        ),
        (
            "CLValidatorsUpdated(uint256,uint256,uint256)",
            vec![word(1)],
            vec![word(2), word(3)],
            pb::StethCLValidatorsUpdated {
                report_timestamp: "1".into(),
                pre_cl_validators: "2".into(),
                post_cl_validators: "3".into(),
            }
            .into(),
        ),
        (
            "DepositedValidatorsChanged(uint256)",
            vec![],
            vec![word(2)],
            pb::StethDepositedValidatorsChanged {
                deposited_validators: "2".into(),
            }
            .into(),
        ),
        (
            "ETHDistributed(uint256,uint256,uint256,uint256,uint256,uint256)",
            vec![word(1)],
            vec![word(2), word(3), word(4), word(5), word(6)],
            pb::StethETHDistributed {
                report_timestamp: "1".into(),
                pre_cl_balance: "2".into(),
                post_cl_balance: "3".into(),
                withdrawals_withdrawn: "4".into(),
                execution_layer_rewards_withdrawn: "5".into(),
                post_buffered_ether: "6".into(),
            }
            .into(),
        ),
        (
            "InternalShareRateUpdated(uint256,uint256,uint256,uint256)",
            vec![word(1)],
            vec![word(2), word(3), word(4)],
            pb::StethInternalShareRateUpdated {
                report_timestamp: "1".into(),
                post_internal_shares: "2".into(),
                post_internal_ether: "3".into(),
                shares_minted_as_fees: "4".into(),
            }
            .into(),
        ),
        ("StakingPaused()", vec![], vec![], pb::StethStakingPaused {}.into()),
        ("StakingResumed()", vec![], vec![], pb::StethStakingResumed {}.into()),
        (
            "StakingLimitSet(uint256,uint256)",
            vec![],
            vec![word(2), word(3)],
            pb::StethStakingLimitSet {
                max_stake_limit: "2".into(),
                stake_limit_increase_per_block: "3".into(),
            }
            .into(),
        ),
        ("StakingLimitRemoved()", vec![], vec![], pb::StethStakingLimitRemoved {}.into()),
        (
            "ELRewardsReceived(uint256)",
            vec![],
            vec![word(2)],
            pb::StethELRewardsReceived { amount: "2".into() }.into(),
        ),
        (
            "WithdrawalsReceived(uint256)",
            vec![],
            vec![word(2)],
            pb::StethWithdrawalsReceived { amount: "2".into() }.into(),
        ),
        (
            "LidoLocatorSet(address)",
            vec![],
            vec![word_address(1)],
            pb::StethLidoLocatorSet { lido_locator: address(1) }.into(),
        ),
    ];

    for (signature, indexed, data, expected) in cases {
        assert_eq!(events_of(event_log(signature, indexed, data)), [expected], "{signature}");
    }
}

#[test]
fn shared_mint_and_burn_signatures_yield_one_event_per_token() {
    // `Mint(address,uint256)`: USDT v0.8.4, WBTC and SAI decode the same log.
    let mint = events_of(event_log("Mint(address,uint256)", vec![word_address(1)], vec![word(6)]));
    let expected: Vec<pb::log::Log> = vec![
        pb::UsdtMint {
            destination: address(1),
            amount: "6".into(),
        }
        .into(),
        pb::WbtcMint {
            to: address(1),
            amount: "6".into(),
        }
        .into(),
        pb::SaiMint {
            guy: address(1),
            wad: "6".into(),
        }
        .into(),
    ];
    assert_eq!(mint, expected);

    // `Burn(address,uint256)`: USDC, WBTC and SAI.
    let burn = events_of(event_log("Burn(address,uint256)", vec![word_address(1)], vec![word(6)]));
    let expected: Vec<pb::log::Log> = vec![
        pb::UsdcBurn {
            burner: address(1),
            amount: "6".into(),
        }
        .into(),
        pb::WbtcBurn {
            burner: address(1),
            value: "6".into(),
        }
        .into(),
        pb::SaiBurn {
            guy: address(1),
            wad: "6".into(),
        }
        .into(),
    ];
    assert_eq!(burn, expected);
}

#[test]
fn shared_events_become_consecutive_logs_with_the_same_log_fields() {
    let mint = event_log("Mint(address,uint256)", vec![word_address(1)], vec![word(6)]);
    let events = project(&block(vec![trx(vec![call(3, vec![mint.clone()])], vec![])]), &mut NoOwners).unwrap();

    let logs = &events.transactions[0].logs;
    assert_eq!(logs.len(), 3);
    for log in logs {
        assert_eq!((log.address.clone(), log.ordinal), (mint.address.clone(), mint.ordinal));
        assert_eq!((log.topics.clone(), log.data.clone()), (mint.topics.clone(), mint.data.clone()));
        assert_eq!(log.call.as_option().unwrap().index, 3);
    }
}

#[test]
fn events_match_on_topic_count_and_data_length_only() {
    // OwnershipTransferred with indexed parameters (WBTC's own) is not
    // FiatToken's shape, so nothing decodes it.
    assert!(events_of(event_log(
        "OwnershipTransferred(address,address)",
        vec![word_address(1), word_address(2)],
        vec![]
    ))
    .is_empty());
    // A Deposit with a second data word.
    assert!(events_of(event_log("Deposit(address,uint256)", vec![word_address(1)], vec![word(1), word(2)])).is_empty());
    // An Issue without data never asks for the owner.
    assert!(events_of(event_log("Issue(uint256)", vec![], vec![])).is_empty());
    // Dirty address padding is ignored.
    let mut dirty = word_address(1);
    dirty[0] = 0xff;
    assert_eq!(
        events_of(event_log("Deposit(address,uint256)", vec![dirty], vec![word(1)])),
        [pb::log::Log::from(pb::WethDeposit {
            dst: address(1),
            wad: "1".into()
        })]
    );
}

#[test]
fn usdt_issue_and_redeem_carry_the_owner_the_lookup_returns() {
    let issue = event_log("Issue(uint256)", vec![], vec![word(10)]);
    let mut redeem = event_log("Redeem(uint256)", vec![], vec![word(20)]);
    redeem.address = address(0x71);
    redeem.ordinal = 2;
    let mut owners = Owners {
        answers: vec![Ok(Some(address(0xee))), Ok(Some(address(0xef)))],
        ..Default::default()
    };

    let events = project(&block(vec![trx(vec![call(0, vec![issue, redeem])], vec![])]), &mut owners).unwrap();

    // One lookup per log, of the emitting contract, in log order.
    assert_eq!(owners.asked, [address(0x70), address(0x71)]);
    let logs: Vec<pb::log::Log> = events.transactions[0].logs.iter().map(|l| l.log.clone().unwrap()).collect();
    let expected: Vec<pb::log::Log> = vec![
        pb::UsdtIssue {
            amount: "10".into(),
            owner: address(0xee),
        }
        .into(),
        pb::UsdtRedeem {
            amount: "20".into(),
            owner: address(0xef),
        }
        .into(),
    ];
    assert_eq!(logs, expected);
}

#[test]
fn usdt_issue_and_redeem_are_dropped_when_owner_fails() {
    let issue = event_log("Issue(uint256)", vec![], vec![word(10)]);
    let redeem = event_log("Redeem(uint256)", vec![], vec![word(20)]);
    let mut owners = Owners {
        answers: vec![Ok(None), Ok(None)],
        ..Default::default()
    };

    let events = project(&block(vec![trx(vec![call(0, vec![issue, redeem])], vec![])]), &mut owners).unwrap();

    assert_eq!(owners.asked.len(), 2);
    // Without any other event the transaction is left out.
    assert!(events.transactions.is_empty());
}

#[test]
fn a_lookup_that_cannot_answer_fails_the_block() {
    let issue = event_log("Issue(uint256)", vec![], vec![word(10)]);
    let mut owners = Owners {
        answers: vec![Err("no response".into())],
        ..Default::default()
    };

    let err = project(&block(vec![trx(vec![call(0, vec![issue])], vec![])]), &mut owners).unwrap_err();

    assert_eq!(err.to_string(), "no response");
}

#[test]
fn eth_call_owner_sends_v0_3_3_request_and_reads_the_first_word() {
    let mut requests = Vec::new();
    let mut respond = |raw: Vec<u8>, failed: bool| {
        let mut lookup = EthCallOwner(|calls: &rpc::RpcCalls| {
            requests.push(calls.encode_to_vec());
            rpc::RpcResponses {
                responses: vec![rpc::RpcResponse { raw: raw.clone(), failed }],
            }
        });
        lookup.owner(&address(0x70)).unwrap()
    };

    // An address word, with dirty padding, and a longer output.
    let mut dirty = word_address(0xee);
    dirty[0] = 0x01;
    assert_eq!(respond(word_address(0xee), false), Some(address(0xee)));
    assert_eq!(respond(dirty, false), Some(address(0xee)));
    assert_eq!(respond([word_address(0xee), word(1)].concat(), false), Some(address(0xee)));
    // A failed call, and outputs shorter than a word.
    assert_eq!(respond(word_address(0xee), true), None);
    assert_eq!(respond(vec![], false), None);
    assert_eq!(respond(word_address(0xee)[..31].to_vec(), false), None);

    // RpcCalls { calls: [RpcCall { to_addr: 0x70.., data: 0x8da5cb5b }] },
    // field by field, as prost encoded it for v0.3.3.
    let mut expected = vec![0x0a, 28, 0x0a, 20];
    expected.extend(address(0x70));
    expected.extend([0x12, 4, 0x8d, 0xa5, 0xcb, 0x5b]);
    assert_eq!(requests.len(), 6);
    assert!(requests.iter().all(|r| *r == expected));
    assert_eq!(owner_call(&address(0x70)).encode_to_vec(), expected);
}

#[test]
fn eth_call_owner_without_a_response_fails() {
    let mut lookup = EthCallOwner(|_: &rpc::RpcCalls| rpc::RpcResponses::default());
    assert!(lookup.owner(&address(0x70)).is_err());
}

#[test]
fn external_shares_burnt_needs_call_metadata() {
    let burnt = || event_log("ExternalSharesBurnt(uint256)", vec![], vec![word(2)]);
    let deposit = || event_log("Deposit(address,uint256)", vec![word_address(1)], vec![word(1)]);

    // A transaction without call traces (detail level BASE) reads its
    // receipt logs: the Deposit decodes, ExternalSharesBurnt has no caller.
    let events = project(&block(vec![trx(vec![], vec![burnt(), deposit()])]), &mut NoOwners).unwrap();
    let logs = &events.transactions[0].logs;
    assert_eq!(logs.len(), 1);
    assert!(logs[0].call.is_unset());
    assert!(matches!(logs[0].log, Some(pb::log::Log::WethDeposit(_))));

    // In a call, its owner is the call's caller.
    let events = project(&block(vec![trx(vec![call(0, vec![burnt()])], vec![])]), &mut NoOwners).unwrap();
    assert_eq!(
        events.transactions[0].logs[0].log,
        Some(
            pb::StethExternalSharesBurnt {
                amount_of_shares: "2".into(),
                owner: address(0xca),
            }
            .into()
        )
    );
}

#[test]
fn transactions_carry_v0_3_3_metadata_and_skip_reverted_frames() {
    let deposit = |ordinal| {
        let mut log = event_log("Deposit(address,uint256)", vec![word_address(1)], vec![word(1)]);
        log.ordinal = ordinal;
        log
    };
    let mut reverted = call(1, vec![deposit(5)]);
    reverted.state_reverted = true;
    let mut failed = trx(vec![call(0, vec![deposit(1)])], vec![]);
    failed.status = TransactionTraceStatus::Failed.into();
    failed.hash = vec![0xbb; 32];

    let events = project(
        &block(vec![failed, trx(vec![call(0, vec![deposit(9)]), reverted, call(2, vec![deposit(3)])], vec![])]),
        &mut NoOwners,
    )
    .unwrap();

    assert_eq!(events.transactions.len(), 1);
    let tx = &events.transactions[0];
    assert_eq!(tx.hash, vec![0xaa; 32]);
    assert_eq!(tx.gas_used, 777);
    assert_eq!(tx.logs.iter().map(|l| l.ordinal).collect::<Vec<_>>(), [3, 9]);
}

#[test]
fn a_successful_transaction_without_receipt_fails_the_block() {
    let mut broken = trx(vec![], vec![]);
    broken.receipt = MessageField::none();
    assert!(project(&block(vec![broken]), &mut NoOwners).is_err());
    assert_eq!(project(&Block::default(), &mut NoOwners).unwrap(), pb::Events::default());
}
