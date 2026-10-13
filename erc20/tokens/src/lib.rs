//! Token-specific events of WETH, USDC, USDT, WBTC, SAI and stETH contracts
//! in a block's successful transactions, each with its transaction, as
//! `erc20.tokens.v1.Events`.
//!
//! Ported from substreams-evm `erc20-tokens` at tag `evm-transfers-v0.3.3`
//! (`d8a731d`), the source of the `erc20_tokens:map_events` module that the
//! production `evm-transfers` v0.3.3 runs; see the package README for what
//! changed in the port and the v0.3.3 behaviour it keeps on purpose.
//!
//! USDT `Issue` and `Redeem` carry the token's `owner()`, which v0.3.3 read
//! with one `eth_call` per log. [`project`] takes it through [`OwnerLookup`],
//! so a host program that decodes blocks itself can run the same decoding
//! without the Substreams runtime; the WASM-only `map_events` handler answers
//! it with Substreams RPC through [`EthCallOwner`].

use common::create::{CreateLog, CreateTransaction};
/// The `erc20.tokens.v1` messages [`project`] returns.
pub use proto::pb::erc20::tokens::v1 as pb;
use substreams::errors::Error;
use substreams_abis::tokens::erc20::sai::events as sai_events;
use substreams_abis::tokens::erc20::steth::events as steth_events;
use substreams_abis::tokens::erc20::usdc::fiattoken_v2_2::events as usdc_events;
use substreams_abis::tokens::erc20::usdt::swap_asset::events as usdt_swap_events;
use substreams_abis::tokens::erc20::usdt::tethertoken_v0_4_18::events as usdt_events;
use substreams_abis::tokens::erc20::usdt::tethertoken_v0_4_18::functions as usdt_functions;
use substreams_abis::tokens::erc20::usdt::tethertoken_v0_8_4::events as usdt_v084_events;
use substreams_abis::tokens::erc20::wbtc::events as wbtc_events;
use substreams_abis::tokens::erc20::weth::events as weth_events;
/// The `eth_call` request and response types [`EthCallOwner`] exchanges.
pub use substreams_ethereum::pb::eth::rpc;
/// The Firehose block types [`project`] takes.
pub use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::Event;

/// Answers `owner()` for the contract that emitted a USDT `Issue` or
/// `Redeem` (TetherToken v0.4.18).
///
/// v0.3.3 called `owner()` once per such log, in log order, and kept the log
/// only when the call returned an owner.
pub trait OwnerLookup {
    /// The 20-byte `owner()` of `contract` as of the block being decoded.
    ///
    /// `Ok(None)` when the call failed or its output does not decode: v0.3.3
    /// then dropped the log.
    ///
    /// # Errors
    ///
    /// When the lookup cannot answer at all, which fails the block (v0.3.3
    /// panicked when the RPC returned no response).
    fn owner(&mut self, contract: &[u8]) -> Result<Option<Vec<u8>>, Error>;
}

/// [`OwnerLookup`] over an `eth_call` transport, as v0.3.3 called it:
/// [`owner_call`] is the request, [`owner_from_response`] reads the first
/// response.
///
/// The WASM handler passes Substreams RPC (`substreams_ethereum::rpc::eth_call`);
/// a host program can pass any transport that answers `eth_call` at the
/// block being decoded.
pub struct EthCallOwner<F>(pub F);

impl<F: FnMut(&rpc::RpcCalls) -> rpc::RpcResponses> OwnerLookup for EthCallOwner<F> {
    fn owner(&mut self, contract: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        let responses = (self.0)(&owner_call(contract)).responses;
        let response = responses.first().ok_or_else(|| Error::msg("one response should have existed"))?;
        Ok(owner_from_response(response))
    }
}

/// The request v0.3.3 sent for a log of `contract`: one `eth_call` to
/// `contract` with the `owner()` selector, `0x8da5cb5b`, and no arguments.
pub fn owner_call(contract: &[u8]) -> rpc::RpcCalls {
    rpc::RpcCalls {
        calls: vec![rpc::RpcCall {
            to_addr: contract.to_vec(),
            data: usdt_functions::Owner {}.encode(),
        }],
    }
}

/// The owner in an `eth_call` response, read as v0.3.3 read it: `None` for a
/// failed call or an output shorter than one 32-byte word, otherwise the low
/// 20 bytes of the first word, whatever its padding.
pub fn owner_from_response(response: &rpc::RpcResponse) -> Option<Vec<u8>> {
    if response.failed {
        return None;
    }
    match usdt_functions::Owner::output(&response.raw) {
        Ok(owner) => Some(owner),
        Err(err) => {
            substreams::log::info!("Call output for function `owner` failed to decode with error: {}", err);
            None
        }
    }
}

/// Decodes the token events of `block`'s successful transactions.
///
/// Each successful transaction with at least one event becomes one
/// `Transaction`, in block order, holding one `Log` per decoded event in the
/// order [`common::logs_with_calls`] gives the logs (persisted call logs by
/// ordinal, or receipt logs for a transaction without call traces). Every
/// decoder runs on every log, so a log whose signature several tokens share
/// (`Mint(address,uint256)`, `Burn(address,uint256)`) yields one event per
/// token, in the order of the checks below.
///
/// # Errors
///
/// A successful transaction without a receipt (v0.3.3 panicked), or an error
/// from `owners`.
pub fn project<O: OwnerLookup + ?Sized>(block: &eth::Block, owners: &mut O) -> Result<pb::Events, Error> {
    common::check_receipts(block)?;
    let mut events = pb::Events::default();
    let mut decoded = Vec::new();

    for trx in block.transactions() {
        let mut transaction = pb::Transaction::create_transaction(trx);

        for (log, call) in common::logs_with_calls(trx) {
            decode_log(log, call, owners, &mut decoded)?;
            for event in decoded.drain(..) {
                transaction.logs.push(pb::Log::create_log_with_call(log, event, call));
            }
        }

        // Only include transactions with logs
        if !transaction.logs.is_empty() {
            events.transactions.push(transaction);
        }
    }
    Ok(events)
}

/// Every event v0.3.3 decoded from `log`, in v0.3.3's order.
fn decode_log<O: OwnerLookup + ?Sized>(log: &eth::Log, call: Option<&eth::Call>, owners: &mut O, out: &mut Vec<pb::log::Log>) -> Result<(), Error> {
    // ============================================
    // WETH Events
    // ============================================

    if let Some(event) = weth_events::Deposit::match_and_decode(log) {
        out.push(
            pb::WethDeposit {
                dst: event.dst,
                wad: event.wad.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = weth_events::Withdrawal::match_and_decode(log) {
        out.push(
            pb::WethWithdrawal {
                src: event.src,
                wad: event.wad.to_string(),
            }
            .into(),
        );
    }

    // ============================================
    // USDC Events
    // ============================================

    if let Some(event) = usdc_events::Mint::match_and_decode(log) {
        out.push(
            pb::UsdcMint {
                minter: event.minter,
                to: event.to,
                amount: event.amount.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = usdc_events::Burn::match_and_decode(log) {
        out.push(
            pb::UsdcBurn {
                burner: event.burner,
                amount: event.amount.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = usdc_events::Blacklisted::match_and_decode(log) {
        out.push(pb::UsdcBlacklisted { account: event.account }.into());
    }

    if let Some(event) = usdc_events::BlacklisterChanged::match_and_decode(log) {
        out.push(
            pb::UsdcBlacklisterChanged {
                new_blacklister: event.new_blacklister,
            }
            .into(),
        );
    }

    if let Some(event) = usdc_events::MasterMinterChanged::match_and_decode(log) {
        out.push(
            pb::UsdcMasterMinterChanged {
                new_master_minter: event.new_master_minter,
            }
            .into(),
        );
    }

    if let Some(event) = usdc_events::MinterConfigured::match_and_decode(log) {
        out.push(
            pb::UsdcMinterConfigured {
                minter: event.minter,
                minter_allowed_amount: event.minter_allowed_amount.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = usdc_events::MinterRemoved::match_and_decode(log) {
        out.push(pb::UsdcMinterRemoved { old_minter: event.old_minter }.into());
    }

    // OwnershipTransferred (shared: USDC, WBTC). FiatToken's parameters are
    // not indexed, so only its shape matches.
    if let Some(event) = usdc_events::OwnershipTransferred::match_and_decode(log) {
        out.push(
            pb::OwnershipTransferred {
                previous_owner: event.previous_owner,
                new_owner: event.new_owner,
            }
            .into(),
        );
    }

    // Pause (shared: USDC, USDT, WBTC)
    if usdc_events::Pause::match_and_decode(log).is_some() {
        out.push(pb::Pause {}.into());
    }

    if let Some(event) = usdc_events::PauserChanged::match_and_decode(log) {
        out.push(
            pb::UsdcPauserChanged {
                new_address: event.new_address,
            }
            .into(),
        );
    }

    if let Some(event) = usdc_events::RescuerChanged::match_and_decode(log) {
        out.push(
            pb::UsdcRescuerChanged {
                new_rescuer: event.new_rescuer,
            }
            .into(),
        );
    }

    if let Some(event) = usdc_events::UnBlacklisted::match_and_decode(log) {
        out.push(pb::UsdcUnBlacklisted { account: event.account }.into());
    }

    // Unpause (shared: USDC, USDT, WBTC)
    if usdc_events::Unpause::match_and_decode(log).is_some() {
        out.push(pb::Unpause {}.into());
    }

    if let Some(event) = usdc_events::AuthorizationCanceled::match_and_decode(log) {
        out.push(
            pb::UsdcAuthorizationCanceled {
                authorizer: event.authorizer,
                nonce: event.nonce.to_vec(),
            }
            .into(),
        );
    }

    if let Some(event) = usdc_events::AuthorizationUsed::match_and_decode(log) {
        out.push(
            pb::UsdcAuthorizationUsed {
                authorizer: event.authorizer,
                nonce: event.nonce.to_vec(),
            }
            .into(),
        );
    }

    // ============================================
    // USDT Events (tethertoken_v0_4_18)
    // ============================================

    // Issue and Redeem: kept only when the emitter answers `owner()`.
    if let Some(event) = usdt_events::Issue::match_and_decode(log) {
        if let Some(owner) = owners.owner(&log.address)? {
            out.push(
                pb::UsdtIssue {
                    amount: event.amount.to_string(),
                    owner,
                }
                .into(),
            );
        }
    }

    if let Some(event) = usdt_events::Redeem::match_and_decode(log) {
        if let Some(owner) = owners.owner(&log.address)? {
            out.push(
                pb::UsdtRedeem {
                    amount: event.amount.to_string(),
                    owner,
                }
                .into(),
            );
        }
    }

    if let Some(event) = usdt_events::Deprecate::match_and_decode(log) {
        out.push(
            pb::UsdtDeprecate {
                new_address: event.new_address,
            }
            .into(),
        );
    }

    if let Some(event) = usdt_events::Params::match_and_decode(log) {
        out.push(
            pb::UsdtParams {
                fee_basis_points: event.fee_basis_points.to_string(),
                max_fee: event.max_fee.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = usdt_events::DestroyedBlackFunds::match_and_decode(log) {
        out.push(
            pb::UsdtDestroyedBlackFunds {
                black_listed_user: event.black_listed_user,
                balance: event.balance.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = usdt_events::AddedBlackList::match_and_decode(log) {
        out.push(pb::UsdtAddedBlackList { user: event.user }.into());
    }

    if let Some(event) = usdt_events::RemovedBlackList::match_and_decode(log) {
        out.push(pb::UsdtRemovedBlackList { user: event.user }.into());
    }

    // ============================================
    // USDT Events (tethertoken_v0_8_4)
    // ============================================

    if let Some(event) = usdt_v084_events::BlockPlaced::match_and_decode(log) {
        out.push(pb::UsdtBlockPlaced { user: event.user }.into());
    }

    if let Some(event) = usdt_v084_events::BlockReleased::match_and_decode(log) {
        out.push(pb::UsdtBlockReleased { user: event.user }.into());
    }

    // Mint(address,uint256): also WBTC's and SAI's `Mint` below.
    if let Some(event) = usdt_v084_events::Mint::match_and_decode(log) {
        out.push(
            pb::UsdtMint {
                destination: event.destination,
                amount: event.amount.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = usdt_v084_events::DestroyedBlockedFunds::match_and_decode(log) {
        out.push(
            pb::UsdtDestroyedBlockedFunds {
                blocked_user: event.blocked_user,
                balance: event.balance.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = usdt_v084_events::NewPrivilegedContract::match_and_decode(log) {
        out.push(pb::UsdtNewPrivilegedContract { contract: event.contract }.into());
    }

    if let Some(event) = usdt_v084_events::RemovedPrivilegedContract::match_and_decode(log) {
        out.push(pb::UsdtRemovedPrivilegedContract { contract: event.contract }.into());
    }

    // ============================================
    // USDT Events (swap_asset)
    // ============================================

    if let Some(event) = usdt_swap_events::LogSwapin::match_and_decode(log) {
        out.push(
            pb::UsdtLogSwapin {
                txhash: event.txhash.to_vec(),
                account: event.account,
                amount: event.amount.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = usdt_swap_events::LogSwapout::match_and_decode(log) {
        out.push(
            pb::UsdtLogSwapout {
                account: event.account,
                bindaddr: event.bindaddr,
                amount: event.amount.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = usdt_swap_events::LogChangeDcrmOwner::match_and_decode(log) {
        out.push(
            pb::UsdtLogChangeDcrmOwner {
                old_owner: event.old_owner,
                new_owner: event.new_owner,
                effective_height: event.effective_height.to_string(),
            }
            .into(),
        );
    }

    // ============================================
    // WBTC Events
    // ============================================

    if let Some(event) = wbtc_events::Mint::match_and_decode(log) {
        out.push(
            pb::WbtcMint {
                to: event.to,
                amount: event.amount.to_string(),
            }
            .into(),
        );
    }

    // Burn(address,uint256): also USDC's `Burn` above and SAI's below.
    if let Some(event) = wbtc_events::Burn::match_and_decode(log) {
        out.push(
            pb::WbtcBurn {
                burner: event.burner,
                value: event.value.to_string(),
            }
            .into(),
        );
    }

    if wbtc_events::MintFinished::match_and_decode(log).is_some() {
        out.push(pb::WbtcMintFinished {}.into());
    }

    if let Some(event) = wbtc_events::OwnershipRenounced::match_and_decode(log) {
        out.push(
            pb::WbtcOwnershipRenounced {
                previous_owner: event.previous_owner,
            }
            .into(),
        );
    }

    // ============================================
    // SAI Events
    // ============================================

    if let Some(event) = sai_events::Mint::match_and_decode(log) {
        out.push(
            pb::SaiMint {
                guy: event.guy,
                wad: event.wad.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = sai_events::Burn::match_and_decode(log) {
        out.push(
            pb::SaiBurn {
                guy: event.guy,
                wad: event.wad.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = sai_events::LogSetAuthority::match_and_decode(log) {
        out.push(pb::SaiLogSetAuthority { authority: event.authority }.into());
    }

    if let Some(event) = sai_events::LogSetOwner::match_and_decode(log) {
        out.push(pb::SaiLogSetOwner { owner: event.owner }.into());
    }

    // ============================================
    // stETH Events
    // ============================================

    if let Some(event) = steth_events::Submitted::match_and_decode(log) {
        out.push(
            pb::StethSubmitted {
                sender: event.sender,
                amount: event.amount.to_string(),
                referral: event.referral,
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::Unbuffered::match_and_decode(log) {
        out.push(
            pb::StethUnbuffered {
                amount: event.amount.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::TokenRebased::match_and_decode(log) {
        out.push(
            pb::StethTokenRebased {
                report_timestamp: event.report_timestamp.to_string(),
                time_elapsed: event.time_elapsed.to_string(),
                pre_total_shares: event.pre_total_shares.to_string(),
                pre_total_ether: event.pre_total_ether.to_string(),
                post_total_shares: event.post_total_shares.to_string(),
                post_total_ether: event.post_total_ether.to_string(),
                shares_minted_as_fees: event.shares_minted_as_fees.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::TransferShares::match_and_decode(log) {
        out.push(
            pb::StethTransferShares {
                from: event.from,
                to: event.to,
                shares_value: event.shares_value.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::SharesBurnt::match_and_decode(log) {
        out.push(
            pb::StethSharesBurnt {
                account: event.account,
                pre_rebase_token_amount: event.pre_rebase_token_amount.to_string(),
                post_rebase_token_amount: event.post_rebase_token_amount.to_string(),
                shares_amount: event.shares_amount.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::ExternalSharesMinted::match_and_decode(log) {
        out.push(
            pb::StethExternalSharesMinted {
                recipient: event.receiver,
                amount_of_shares: event.amount_of_shares.to_string(),
            }
            .into(),
        );
    }

    // ExternalSharesBurnt names no owner: v0.3.3 takes the emitting call's
    // caller, so the event needs call metadata and is dropped from a
    // transaction without call traces.
    if let Some(call) = call {
        if let Some(event) = steth_events::ExternalSharesBurnt::match_and_decode(log) {
            out.push(
                pb::StethExternalSharesBurnt {
                    amount_of_shares: event.amount_of_shares.to_string(),
                    owner: call.caller.to_vec(),
                }
                .into(),
            );
        }
    }

    if let Some(event) = steth_events::ExternalEtherTransferredToBuffer::match_and_decode(log) {
        out.push(
            pb::StethExternalEtherTransferredToBuffer {
                amount: event.amount.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::ExternalBadDebtInternalized::match_and_decode(log) {
        out.push(
            pb::StethExternalBadDebtInternalized {
                amount_of_shares: event.amount_of_shares.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::MaxExternalRatioBpSet::match_and_decode(log) {
        out.push(
            pb::StethMaxExternalRatioBPSet {
                max_external_ratio_bp: event.max_external_ratio_bp.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::ClValidatorsUpdated::match_and_decode(log) {
        out.push(
            pb::StethCLValidatorsUpdated {
                report_timestamp: event.report_timestamp.to_string(),
                pre_cl_validators: event.pre_cl_validators.to_string(),
                post_cl_validators: event.post_cl_validators.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::DepositedValidatorsChanged::match_and_decode(log) {
        out.push(
            pb::StethDepositedValidatorsChanged {
                deposited_validators: event.deposited_validators.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::EthDistributed::match_and_decode(log) {
        out.push(
            pb::StethETHDistributed {
                report_timestamp: event.report_timestamp.to_string(),
                pre_cl_balance: event.pre_cl_balance.to_string(),
                post_cl_balance: event.post_cl_balance.to_string(),
                withdrawals_withdrawn: event.withdrawals_withdrawn.to_string(),
                execution_layer_rewards_withdrawn: event.execution_layer_rewards_withdrawn.to_string(),
                post_buffered_ether: event.post_buffered_ether.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::InternalShareRateUpdated::match_and_decode(log) {
        out.push(
            pb::StethInternalShareRateUpdated {
                report_timestamp: event.report_timestamp.to_string(),
                post_internal_shares: event.post_internal_shares.to_string(),
                post_internal_ether: event.post_internal_ether.to_string(),
                shares_minted_as_fees: event.shares_minted_as_fees.to_string(),
            }
            .into(),
        );
    }

    if steth_events::StakingPaused::match_and_decode(log).is_some() {
        out.push(pb::StethStakingPaused {}.into());
    }

    if steth_events::StakingResumed::match_and_decode(log).is_some() {
        out.push(pb::StethStakingResumed {}.into());
    }

    if let Some(event) = steth_events::StakingLimitSet::match_and_decode(log) {
        out.push(
            pb::StethStakingLimitSet {
                max_stake_limit: event.max_stake_limit.to_string(),
                stake_limit_increase_per_block: event.stake_limit_increase_per_block.to_string(),
            }
            .into(),
        );
    }

    if steth_events::StakingLimitRemoved::match_and_decode(log).is_some() {
        out.push(pb::StethStakingLimitRemoved {}.into());
    }

    if let Some(event) = steth_events::ElRewardsReceived::match_and_decode(log) {
        out.push(
            pb::StethELRewardsReceived {
                amount: event.amount.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::WithdrawalsReceived::match_and_decode(log) {
        out.push(
            pb::StethWithdrawalsReceived {
                amount: event.amount.to_string(),
            }
            .into(),
        );
    }

    if let Some(event) = steth_events::LidoLocatorSet::match_and_decode(log) {
        out.push(
            pb::StethLidoLocatorSet {
                lido_locator: event.lido_locator,
            }
            .into(),
        );
    }

    Ok(())
}

// The export exists only in the WASM build so host crates can link this
// crate next to other packages.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;

    #[substreams::handlers::map]
    fn map_events(block: eth::Block) -> Result<pb::Events, Error> {
        project(&block, &mut EthCallOwner(substreams_ethereum::rpc::eth_call))
    }
}

#[cfg(test)]
mod tests;
