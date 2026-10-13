# ERC-20 Transfers

ERC-20 `Transfer` and `Approval` logs of a block's successful transactions,
each with its transaction, as **`erc20.transfers.v1.Events`**
([schema](../../proto/v1/erc20-transfers.proto)). A port of substreams-evm
`erc20-transfers` at tag `evm-transfers-v0.3.3` (`d8a731d`), the
`erc20_transfers:map_events` module that the Token API's production
`evm-transfers` v0.3.3 runs, to substreams 0.8.0, buffa and substreams-abis
v2.0.0. Its output is unchanged, including the v0.3.3 behaviour listed under
[Kept from v0.3.3](#kept-from-v033).

## Output

One `Transaction` per successful transaction with at least one matching log,
in block order: hash, from, `to` (unset for a contract creation), nonce, gas
price, gas limit, `gas_used` and value. Its `logs` hold, in log order, each
matching log's emitter, ordinal, raw topics and data, the emitting call's
caller, index, depth and type when there is one, and either

| Event | Signature | Fields |
| --- | --- | --- |
| `Transfer` | `Transfer(address indexed,address indexed,uint256)` | `from`, `to`, `amount` |
| `Approval` | `Approval(address indexed,address indexed,uint256)` | `owner`, `spender`, `value` |

A log matches when its topic 0 is the signature hash, it has exactly three
topics and exactly 32 bytes of data. Addresses are the low 20 bytes of their
topic, whatever the high 12 bytes hold; amounts are exact decimal strings.

## Host API

`erc20_transfers::project(&eth::Block) -> Result<pb::Events, Error>` is the
whole module without the Substreams runtime, for a host program that decodes
blocks itself; `eth` and `pb` re-export the input and output types. It fails
where v0.3.3 panicked: on a successful transaction without a receipt.

## Kept from v0.3.3

1. **Receipt-log fallback.** A transaction with call traces gives the logs of
   its non-reverted calls, sorted by ordinal, with call metadata. A
   transaction without call traces, as in blocks of detail level BASE (all of
   Avalanche), gives its receipt logs in receipt order, without call metadata.
2. **`Transaction.gas_used`** is the receipt's cumulative gas used in the
   block, not the transaction's own gas.
3. **Shape, not standard.** Any emitter qualifies; ERC-721-shaped logs (four
   topics) and logs with other data lengths are skipped; non-canonically
   padded address topics decode to their low 20 bytes.
4. **Successful transactions only**, and only the persisted (non-reverted)
   frames of their calls.

## Port from v0.3.3

| File | Source at `d8a731d` | Changes |
| --- | --- | --- |
| `src/lib.rs` | `erc20-transfers/src/lib.rs` | `map_events` in a wasm32-only module over `project`; errors instead of panics; the per-block counter log lines are not ported |
| `../../common/src/` | `common/src/create.rs` | the `erc20.transfers.v1` templates and the shared log selection; see [`common`](../../common/src/lib.rs) |
| `../../proto/v1/erc20-transfers.proto` | `proto/v1/erc20-transfers.proto` | none (byte-identical; the packed descriptor equals v0.3.3's) |
| `substreams.yaml` | `erc20-transfers/substreams.yaml` | `erc20_transfers` v0.8.0; binary type `wasm/rust-v1` (v0.3.3 declared `+wasm-bindgen-shims` but imports no shim) |

Deviations from v0.3.3, none of which changes an output:
- **Errors instead of panics** on a successful transaction without a receipt.
- **substreams-abis v0.8.0 → v2.0.0.** `Transfer` and `Approval` have the same
  topic 0, `match_log` rules (topic count, exact data length) and fields in
  both versions; v2.0.0's readers take an address's low 20 bytes without
  checking its padding, as v0.8.0's `ethabi` 17 did.
- **substreams 0.7.6 → 0.8.0, prost → buffa.** The decimal strings of the
  transaction's gas price and value come from the unsigned integer directly;
  v0.3.3 divided it by 1 as a `BigDecimal` at scale 0, which prints the same
  digits.

The production build is the committed `spkg/erc20-transfers-v0.3.3.spkg` of
`d8a731d`, module `7d6f427f…`. Rust 1.88.0 on macOS, where it was built,
rebuilds its WASM byte for byte from `d8a731d` (sha256 `1f1ad2ff…`).

## Build

```sh
cargo test --locked -p erc20-transfers
make -C erc20/transfers pack   # out/erc20-transfers-v0.8.0.spkg
```

Committed spkgs come from CI's Linux build, never a local pack.
