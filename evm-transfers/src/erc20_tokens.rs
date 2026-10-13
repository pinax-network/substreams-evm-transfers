//! The token tables (`weth_deposit`, `usdc_mint`, `usdt_issue`, ...). Ported
//! from substreams-evm `db-evm-transfers/src/erc20_tokens.rs` (`d8a731d`).

use substreams::errors::Error;
use substreams_database_change::tables::Tables;

use crate::columns::{set_log, set_transaction, BlockColumns};
use crate::{tokens_pb as pb, Encoding};

pub(crate) fn process_events(encoding: Encoding, tables: &mut Tables, block: &BlockColumns, events: &pb::Events) -> Result<(), Error> {
    for (tx_index, tx) in events.transactions.iter().enumerate() {
        for (log_index, log) in tx.logs.iter().enumerate() {
            let Some((table, columns)) = log.log.as_ref().and_then(|event| event_row(encoding, event)) else {
                continue;
            };
            let row = tables.create_row(table, block.log_key(tx_index, log_index)?);
            block.set(row);
            set_log(encoding, log, log_index, row);
            set_transaction(encoding, tx, tx_index, row);
            for (name, value) in columns {
                row.set(name, value);
            }
        }
    }
    Ok(())
}

/// The table and the event columns v0.3.3 wrote for `event`; `None` for the
/// events it stored nowhere (USDC admin events, `Pause`, `Unpause`,
/// `OwnershipTransferred`, the other USDT, WBTC and SAI events and the other
/// stETH events). Addresses follow `encoding`, as does `usdt_log_swapin.txhash`
/// (v0.3.3 wrote it with `bytes_to_string`); amounts are verbatim.
fn event_row(encoding: Encoding, event: &pb::log::Log) -> Option<(&'static str, Vec<(&'static str, String)>)> {
    use pb::log::Log as E;
    let address = |bytes: &[u8]| encoding.address(bytes);
    Some(match event {
        E::WethDeposit(e) => ("weth_deposit", vec![("dst", address(&e.dst)), ("wad", e.wad.clone())]),
        E::WethWithdrawal(e) => ("weth_withdrawal", vec![("src", address(&e.src)), ("wad", e.wad.clone())]),
        E::UsdcMint(e) => (
            "usdc_mint",
            vec![("minter", address(&e.minter)), ("to", address(&e.to)), ("amount", e.amount.clone())],
        ),
        E::UsdcBurn(e) => ("usdc_burn", vec![("burner", address(&e.burner)), ("amount", e.amount.clone())]),
        E::UsdtIssue(e) => ("usdt_issue", vec![("owner", address(&e.owner)), ("amount", e.amount.clone())]),
        E::UsdtRedeem(e) => ("usdt_redeem", vec![("owner", address(&e.owner)), ("amount", e.amount.clone())]),
        E::UsdtDestroyedBlackFunds(e) => (
            "usdt_destroyed_black_funds",
            vec![("black_listed_user", address(&e.black_listed_user)), ("balance", e.balance.clone())],
        ),
        E::UsdtBlockPlaced(e) => ("usdt_block_placed", vec![("user", address(&e.user))]),
        E::UsdtBlockReleased(e) => ("usdt_block_released", vec![("user", address(&e.user))]),
        E::UsdtMint(e) => ("usdt_mint", vec![("destination", address(&e.destination)), ("amount", e.amount.clone())]),
        E::UsdtDestroyedBlockedFunds(e) => (
            "usdt_destroyed_blocked_funds",
            vec![("blocked_user", address(&e.blocked_user)), ("balance", e.balance.clone())],
        ),
        E::UsdtNewPrivilegedContract(e) => ("usdt_new_privileged_contract", vec![("contract", address(&e.contract))]),
        E::UsdtRemovedPrivilegedContract(e) => ("usdt_removed_privileged_contract", vec![("contract", address(&e.contract))]),
        E::UsdtLogSwapin(e) => (
            "usdt_log_swapin",
            vec![("txhash", address(&e.txhash)), ("account", address(&e.account)), ("amount", e.amount.clone())],
        ),
        E::UsdtLogSwapout(e) => (
            "usdt_log_swapout",
            vec![
                ("account", address(&e.account)),
                ("bindaddr", address(&e.bindaddr)),
                ("amount", e.amount.clone()),
            ],
        ),
        E::UsdtLogChangeDcrmOwner(e) => (
            "usdt_log_change_dcrm_owner",
            vec![
                ("old_owner", address(&e.old_owner)),
                ("new_owner", address(&e.new_owner)),
                ("effective_height", e.effective_height.clone()),
            ],
        ),
        E::WbtcMint(e) => ("wbtc_mint", vec![("to", address(&e.to)), ("amount", e.amount.clone())]),
        E::WbtcBurn(e) => ("wbtc_burn", vec![("burner", address(&e.burner)), ("value", e.value.clone())]),
        E::SaiMint(e) => ("sai_mint", vec![("guy", address(&e.guy)), ("wad", e.wad.clone())]),
        E::SaiBurn(e) => ("sai_burn", vec![("guy", address(&e.guy)), ("wad", e.wad.clone())]),
        E::StethSubmitted(e) => (
            "steth_submitted",
            vec![("sender", address(&e.sender)), ("amount", e.amount.clone()), ("referral", address(&e.referral))],
        ),
        E::StethTokenRebased(e) => (
            "steth_token_rebased",
            vec![
                ("report_timestamp", e.report_timestamp.clone()),
                ("time_elapsed", e.time_elapsed.clone()),
                ("pre_total_shares", e.pre_total_shares.clone()),
                ("pre_total_ether", e.pre_total_ether.clone()),
                ("post_total_shares", e.post_total_shares.clone()),
                ("post_total_ether", e.post_total_ether.clone()),
                ("shares_minted_as_fees", e.shares_minted_as_fees.clone()),
            ],
        ),
        E::StethTransferShares(e) => (
            "steth_transfer_shares",
            vec![("from", address(&e.from)), ("to", address(&e.to)), ("shares_value", e.shares_value.clone())],
        ),
        E::StethSharesBurnt(e) => (
            "steth_shares_burnt",
            vec![
                ("account", address(&e.account)),
                ("pre_rebase_token_amount", e.pre_rebase_token_amount.clone()),
                ("post_rebase_token_amount", e.post_rebase_token_amount.clone()),
                ("shares_amount", e.shares_amount.clone()),
            ],
        ),
        E::StethExternalSharesMinted(e) => (
            "steth_external_shares_minted",
            vec![("recipient", address(&e.recipient)), ("amount_of_shares", e.amount_of_shares.clone())],
        ),
        E::StethExternalSharesBurnt(e) => (
            "steth_external_shares_burnt",
            vec![("owner", address(&e.owner)), ("amount_of_shares", e.amount_of_shares.clone())],
        ),
        E::Pause(_)
        | E::Unpause(_)
        | E::OwnershipTransferred(_)
        | E::UsdcBlacklisted(_)
        | E::UsdcBlacklisterChanged(_)
        | E::UsdcMasterMinterChanged(_)
        | E::UsdcMinterConfigured(_)
        | E::UsdcMinterRemoved(_)
        | E::UsdcPauserChanged(_)
        | E::UsdcRescuerChanged(_)
        | E::UsdcUnBlacklisted(_)
        | E::UsdcAuthorizationCanceled(_)
        | E::UsdcAuthorizationUsed(_)
        | E::UsdtDeprecate(_)
        | E::UsdtParams(_)
        | E::UsdtAddedBlackList(_)
        | E::UsdtRemovedBlackList(_)
        | E::WbtcMintFinished(_)
        | E::WbtcOwnershipRenounced(_)
        | E::SaiLogSetAuthority(_)
        | E::SaiLogSetOwner(_)
        | E::StethUnbuffered(_)
        | E::StethExternalEtherTransferredToBuffer(_)
        | E::StethExternalBadDebtInternalized(_)
        | E::StethMaxExternalRatioBpSet(_)
        | E::StethClValidatorsUpdated(_)
        | E::StethDepositedValidatorsChanged(_)
        | E::StethEthDistributed(_)
        | E::StethInternalShareRateUpdated(_)
        | E::StethStakingPaused(_)
        | E::StethStakingResumed(_)
        | E::StethStakingLimitSet(_)
        | E::StethStakingLimitRemoved(_)
        | E::StethElRewardsReceived(_)
        | E::StethWithdrawalsReceived(_)
        | E::StethLidoLocatorSet(_) => return None,
    })
}
