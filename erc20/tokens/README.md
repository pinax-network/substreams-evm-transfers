# ERC-20 Tokens

Token-specific events of WETH, USDC, USDT, WBTC, SAI and stETH contracts in a
block's successful transactions, each with its transaction, as
**`erc20.tokens.v1.Events`** ([schema](../../proto/v1/erc20-tokens.proto)). A
port of substreams-evm `erc20-tokens` at tag `evm-transfers-v0.3.3`
(`d8a731d`), the `erc20_tokens:map_events` module that the Token API's
production `evm-transfers` v0.3.3 runs, to substreams 0.8.0, buffa and
substreams-abis v2.0.0. Its output is unchanged, including the v0.3.3
behaviour listed under [Kept from v0.3.3](#kept-from-v033).

## Events

Transactions and logs are shaped as in [`erc20/transfers`](../transfers/README.md#output):
one `Transaction` per successful transaction with at least one event, one
`Log` per decoded event with the log's raw fields and call metadata.

| Contract (ABI in substreams-abis v2.0.0) | Events |
| --- | --- |
| WETH9 (`tokens::erc20::weth`) | `Deposit`, `Withdrawal` |
| USDC FiatToken v2.2 (`tokens::erc20::usdc::fiattoken_v2_2`) | `Mint`, `Burn`, `Blacklisted`, `UnBlacklisted`, `BlacklisterChanged`, `MasterMinterChanged`, `MinterConfigured`, `MinterRemoved`, `PauserChanged`, `RescuerChanged`, `AuthorizationCanceled`, `AuthorizationUsed`; shared `OwnershipTransferred`, `Pause`, `Unpause` |
| USDT TetherToken v0.4.18 (`tokens::erc20::usdt::tethertoken_v0_4_18`) | `Issue`, `Redeem` (with `owner()`), `Deprecate`, `Params`, `DestroyedBlackFunds`, `AddedBlackList`, `RemovedBlackList` |
| USDT TetherToken v0.8.4 (`tokens::erc20::usdt::tethertoken_v0_8_4`) | `BlockPlaced`, `BlockReleased`, `Mint`, `DestroyedBlockedFunds`, `NewPrivilegedContract`, `RemovedPrivilegedContract` |
| USDT swap asset (`tokens::erc20::usdt::swap_asset`) | `LogSwapin`, `LogSwapout`, `LogChangeDCRMOwner` |
| WBTC (`tokens::erc20::wbtc`) | `Mint`, `Burn`, `MintFinished`, `OwnershipRenounced` |
| SAI (`tokens::erc20::sai`) | `Mint`, `Burn`, `LogSetAuthority`, `LogSetOwner` |
| stETH (`tokens::erc20::steth`) | `Submitted`, `Unbuffered`, `TokenRebased`, `TransferShares`, `SharesBurnt`, `ExternalSharesMinted`, `ExternalSharesBurnt`, `ExternalEtherTransferredToBuffer`, `ExternalBadDebtInternalized`, `MaxExternalRatioBPSet`, `CLValidatorsUpdated`, `DepositedValidatorsChanged`, `ETHDistributed`, `InternalShareRateUpdated`, `StakingPaused`, `StakingResumed`, `StakingLimitSet`, `StakingLimitRemoved`, `ELRewardsReceived`, `WithdrawalsReceived`, `LidoLocatorSet` |

No emitter is checked: any contract's log with a matching signature, topic
count and exact data length decodes. Amounts are exact decimal strings.

## USDT `Issue` and `Redeem`: the `owner()` call

v0.3.3 put the token's `owner()` into `UsdtIssue` and `UsdtRedeem`. For each
such log it made one `eth_call` to the emitting contract at the block, with
`owner()` (selector `0x8da5cb5b`, no arguments), and kept the log only when
the call returned an owner: a failed call, or an output shorter than one word,
drops the log silently; the owner is the low 20 bytes of the first word. The
port keeps that call, byte for byte:

- `project` asks an `OwnerLookup` once per `Issue` or `Redeem` log, in log
  order, for the emitter's owner. `Ok(None)` drops the log; an error fails
  the block.
- `EthCallOwner(transport)` is the v0.3.3 call over any `eth_call` transport:
  `owner_call(contract)` builds the same `RpcCalls` request (one call, the
  emitter as `to_addr`, `0x8da5cb5b` as data, the same protobuf bytes) and
  `owner_from_response` reads the response as v0.3.3 did. With no response at
  all it fails the block, where v0.3.3 panicked.
- The WASM `map_events` handler passes Substreams RPC
  (`substreams_ethereum::rpc::eth_call`), as v0.3.3 did, so the package needs
  an endpoint with RPC enabled, like v0.3.3.

The owner reaches only the `usdt_issue` and `usdt_redeem` tables of
[`evm-transfers`](../../README.md); the `transfers` rows of these events leave
`from` and `to` empty. The call still decides whether the event exists.

## Host API

`erc20_tokens::project(&eth::Block, &mut impl OwnerLookup) -> Result<pb::Events, Error>`
is the whole module without the Substreams runtime. A host program implements
`OwnerLookup` itself, or wraps its own `eth_call` client in `EthCallOwner`.
`eth`, `pb` and `rpc` re-export the block, output and `eth_call` types. It
fails where v0.3.3 panicked: on a successful transaction without a receipt,
and when the lookup cannot answer.

## Kept from v0.3.3

1. **Shared signatures give one event per token.** Every decoder runs on
   every log. `Mint(address indexed,uint256)` is USDT v0.8.4's, WBTC's and
   SAI's `Mint`, so one such log yields `UsdtMint`, `WbtcMint` and `SaiMint`
   (in that order, as consecutive logs); `Burn(address indexed,uint256)`
   yields `UsdcBurn`, `WbtcBurn` and `SaiBurn`. In `evm-transfers` each
   becomes a row of its table, so such a `Mint` gives two `transfers` rows
   (WBTC's and SAI's; `usdt_mint` feeds none) and such a `Burn` three.
2. **`OwnershipTransferred`** is decoded with FiatToken's ABI (parameters not
   indexed): the indexed form WBTC and most Ownable contracts emit does not
   match.
3. **`ExternalSharesBurnt`** takes the emitting call's caller as `owner`, so it
   is dropped from a transaction without call traces.
4. **Receipt-log fallback**, **cumulative `gas_used`** and **successful
   transactions only**, as in [`erc20/transfers`](../transfers/README.md#kept-from-v033).
5. **The `owner()` call**, above, including dropping the log when it fails.

## Port from v0.3.3

| File | Source at `d8a731d` | Changes |
| --- | --- | --- |
| `src/lib.rs` | `erc20-tokens/src/lib.rs` | `map_events` in a wasm32-only module over `project`; the decoders in one function, in v0.3.3's order; `owner()` through `OwnerLookup`; errors instead of panics; the per-block counter log lines are not ported |
| `../../common/src/` | `common/src/create.rs` | the `erc20.tokens.v1` templates and the shared log selection |
| `../../proto/v1/erc20-tokens.proto` | `proto/v1/erc20-tokens.proto` | none (byte-identical; the packed descriptor equals v0.3.3's) |
| `substreams.yaml` | `erc20-tokens/substreams.yaml` | `erc20_tokens` v0.8.0; binary type `wasm/rust-v1` (v0.3.3 declared `+wasm-bindgen-shims` but imports no shim) |

Deviations from v0.3.3, none of which changes an output:
- **Errors instead of panics** on a successful transaction without a receipt
  (v0.3.3 failed the block at that transaction, after the `owner()` calls of
  earlier transactions; the port fails before any call), and on an `eth_call`
  without a response.
- **substreams-abis v0.8.0 → v2.0.0.** All 62 events used have the same topic
  0, `match_log` rules (topic count, exact data length) and fields in both
  versions (compared with a script over both checkouts); `Owner` encodes the
  same request and reads the first word as before. v2.0.0's readers take an
  address's low 20 bytes without checking its padding, and accept outputs
  longer than needed, as v0.8.0's `ethabi` 17 did.

The production build is the committed `spkg/erc20-tokens-v0.3.3.spkg` of
`d8a731d`, module `fe4d1f2d…`; Rust 1.88.0 rebuilds its WASM byte for byte
from `d8a731d` (sha256 `933d1c85…`).

## Build

```sh
cargo test --locked -p erc20-tokens
make -C erc20/tokens pack   # out/erc20-tokens-v0.8.0.spkg
```

Committed spkgs come from CI's Linux build, never a local pack.
