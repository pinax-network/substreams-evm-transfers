# EVM Native Transfers

Native value transfers of a block's successful transactions and their calls,
and its block rewards, as **`native.transfers.v1.Events`**
([schema](../../proto/v1/native-transfers.proto)). A port of substreams-evm
`native-transfers` as of `f576af1` (unchanged since `aaae9dd`), the build of
`evm-native-transfers-v0.2.0.spkg` that the Token API's production
`evm-transfers` v0.3.3 imports, to substreams 0.8.0 and buffa. Its output is
unchanged.

That build predates the withdrawal, selfdestruct, genesis-balance and DAO
messages and the `Call.call_type` field that substreams-evm added on
2026-02-01 (`887f0b0`, `5e2f109`): production v0.3.3 writes none of those rows,
and neither does this port.

## Output

- **`block_rewards`**: one per block-level balance change whose reason is
  `REASON_REWARD_MINE_BLOCK`, `REASON_REWARD_TRANSACTION_FEE`,
  `REASON_REWARD_BLOB_FEE`, `REASON_REWARD_MINE_UNCLE` or
  `REASON_INCREASE_MINT` (Optimism) and whose balance grew: the account, the
  increase and the reason.
- **`transactions`**: each successful transaction, in block order, that sends
  value or has a call that does: hash, from, `to` (unset for a contract
  creation), nonce, gas price (zero when unset or wider than 20 bytes), gas
  limit, `gas_used` (the transaction's own) and value; and its `calls` that
  move value: not reverted or failed, value above zero, depth above zero (the
  root call repeats the transaction's value) and type `CALL` (a
  `DELEGATECALL` moves no value), with index, ordinals, caller, address,
  value, gas and depth.

Amounts are exact decimal strings. Balance changes and calls come from
Extended blocks; a BASE block gives transactions with their value only.

## Host API

`native_transfers::project(&eth::Block) -> pb::Events` is the whole module
without the Substreams runtime; `eth` and `pb` re-export the input and output
types. It reads no receipt and cannot fail.

## Port from v0.2.0

| File | Source at `f576af1` | Changes |
| --- | --- | --- |
| `src/lib.rs` | `native-transfers/src/lib.rs` and `src/utils.rs` | `map_events` in a wasm32-only module over `project`; the helpers inlined; the unused `_is_failed_transaction` dropped |
| `../../proto/v1/native-transfers.proto` | `proto/v1/native-transfers.proto` (`aaae9dd`) | none (byte-identical; the packed descriptor equals production's) |
| `substreams.yaml` | `native-transfers/substreams.yaml` | `evm_native_transfers` v0.8.0 |

Deviations, none of which changes an output:
- **substreams 0.7.0 → 0.8.0, prost → buffa.** The transaction value comes
  from the unsigned integer directly; v0.2.0 divided it by 1 as a
  `BigDecimal` at scale 0, which prints the same digits and is zero exactly
  when the integer is. An enum value outside `CallType` or
  `BalanceChange.Reason` reads as no `CALL` and no reward, as prost's getters
  read it as the default.

The production build is `spkg/evm-native-transfers-v0.2.0.spkg`, committed in
`f576af1` (2026-01-13) and moved by `ce22b43`, module `f2a74d0f…`. Rust 1.88.0
on macOS, where it was built, rebuilds its WASM byte for byte from `f576af1`
(sha256 `0f93aede…`), and its packed `v1/native-transfers.proto` is
`aaae9dd`'s.

## Build

```sh
cargo test --locked -p native-transfers
make -C native/transfers pack   # out/evm-native-transfers-v0.8.0.spkg
```

Committed spkgs come from CI's Linux build, never a local pack.
