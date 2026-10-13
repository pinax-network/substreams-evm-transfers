# substreams-evm-transfers

EVM transfer [Substreams](https://substreams.dev). It covers:
- ERC-20 transfers and approvals;
- wrapped and stable token events: WETH/WBNB deposits and withdrawals, USDC,
  USDT, WBTC and SAI mints and burns, and stETH events;
- native value transfers and block rewards.

Every package emits native protobuf, `evm-transfers` writes the Token API's
ClickHouse transfers tables, and every package is also a plain Rust library
that a host program, such as firehose-parquet, can call on a decoded block
without the Substreams runtime.

Sibling repositories:
[substreams-evm-balances](https://github.com/pinax-network/substreams-evm-balances)
(native and ERC-20 balances) and
[substreams-evm-dex](https://github.com/pinax-network/substreams-evm-dex)
(swaps and pool state). This repository takes over the transfer code of
[substreams-evm](https://github.com/pinax-network/substreams-evm).

## Packages

| Package | Module | Output |
| --- | --- | --- |
| [`erc20/transfers`](erc20/transfers/README.md) (`erc20_transfers` v0.8.0) | `map_events` | `erc20.transfers.v1.Events`: ERC-20 `Transfer` and `Approval` logs of successful transactions, with their transaction |
| [`erc20/tokens`](erc20/tokens/README.md) (`erc20_tokens` v0.8.0) | `map_events` | `erc20.tokens.v1.Events`: WETH/WBNB `Deposit`/`Withdrawal`; USDC, USDT, WBTC and SAI mints, burns and admin events; stETH `Submitted`, `TransferShares`, `SharesBurnt`, `TokenRebased` and external shares |
| [`native/transfers`](native/transfers/README.md) (`evm_native_transfers` v0.8.0) | `map_events` | `native.transfers.v1.Events`: native value of transactions and calls, and block rewards |
| [`evm-transfers`](evm-transfers/README.md) (`evm_transfers` v0.8.0) | `db_out` (imports the three maps) | `DatabaseChanges` into the Token API transfers tables; the ClickHouse SQL is embedded in the package |
| [`erc20/events`](erc20/events/README.md) (`erc20_events` v0.1.0) | `map_events` | `erc20.events.v1.Events`: standard ERC-20 `Transfer` and `Approval` evidence from Extended blocks, with persistence scope (persisted or attempted, system calls) |

```
sf.ethereum.type.v2.Block ─┬─▶ erc20_transfers:map_events ─┐
                           ├─▶ erc20_tokens:map_events ────┼─▶ evm_transfers:db_out ──▶ ClickHouse (Token API)
                           └─▶ native_transfers:map_events ┘
```

`erc20/events` is separate evidence for consumers that need persistence
scope. Its API and protobuf are a public contract, pinned by other Pinax
services (see [AGENTS.md](AGENTS.md)).

## ClickHouse tables (`evm-transfers`)

The SQL is substreams-evm `evm-transfers-v0.3.3`'s, byte for byte: 12 files,
concatenated sha256 `6ef74c5a…`, the schema embedded in the production
package. `db_out` writes the same rows.
- **Base tables:** one per event family. Examples: `erc20_transfers`,
  `erc20_approvals`, `weth_deposit`, `usdt_issue`, `steth_transfer_shares`,
  `transactions`, `calls`, `block_rewards`.
- **Views for the Token API:** materialized views combine the base tables
  into `transfers` (one row per token movement, typed by `transfer_type`)
  and `native_transfers`.
- **`blocks`:** every block that wrote a row.

The Token API and its scraper read `transfers`, `native_transfers` and
`blocks`. [`evm-transfers`](evm-transfers/README.md#rows) lists every table,
key and column. The `db_out` param is the address encoding: `hex`, or
`tron_base58` on Tron.

## Block detail and RPC

The maps keep v0.3.3's behaviour, at the owner's direction (2026-10-13):
- **BASE blocks:** on blocks without call traces, they fall back to receipt
  logs, so chains that only serve BASE blocks, such as Avalanche, work.
- **One RPC call:** USDT `Issue` and `Redeem` still call `owner()` at the
  block, as v0.3.3 does, and a log without an owner is dropped. It is the
  only RPC call in this repository.

## Use from Rust

Each crate re-exports the block type it takes (`eth`) and its output types
(`pb`). For a host without Substreams RPC, `erc20_tokens` takes the `owner()`
lookup as a trait, `OwnerLookup`. `EthCallOwner` wraps any `eth_call`
transport.

```rust
let erc20 = erc20_transfers::project(&block)?;                    // erc20.transfers.v1.Events
let mut owners = erc20_tokens::EthCallOwner(|calls| eth_call_at(&block, calls));
let tokens = erc20_tokens::project(&block, &mut owners)?;         // erc20.tokens.v1.Events
let native = native_transfers::project(&block);                   // native.transfers.v1.Events
let rows = evm_transfers::changes(&clock, &erc20, &tokens, &native, evm_transfers::Encoding::Hex)?;

let config = erc20_events::parse(params)?;                        // erc20.events.v1 evidence
let evidence = erc20_events::project(&block, &config)?;
```

## Build and test

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.0). The workspace
is on substreams 0.8.0, substreams-ethereum 0.12.0, buffa 0.9.2 and
substreams-abis `v2.0.0`.

```sh
cargo test --locked --workspace
make -C evm-transfers build schema pack   # the three maps and evm-transfers WASM; the spkg
```

CI runs formatting, tests, Clippy on the host and wasm32, a wasm32 check and
rustdoc, all with warnings denied. Release spkgs will come from CI's Linux
build.

## Deploy

Releases are plain `v<version>` tags starting at `v0.8.0`. Each chain gets
its own database, `<chain>:evm-transfers@<version>`; those names never collide
with substreams-evm's `evm-transfers` v0.2–v0.7. The Token API live sink
takes the release spkg as `SPKG_URL`. Production runs v0.3.3 today. Its
tables are byte-compatible with this schema, so a new database can take its
history by partition copy. Before the Token API switches, run a noop over the
range and check parity against production; each of those is a live step that
needs the owner's OK.

## Provenance and status

- **Ported from v0.3.3, output unchanged.** `erc20/transfers`,
  `erc20/tokens` and `evm-transfers` come from substreams-evm
  `evm-transfers-v0.3.3` (`d8a731d`). `native/transfers` comes from the build
  that v0.3.3 imports (`f576af1`, source unchanged since `aaae9dd`). Rust 1.88
  rebuilds all four production WASMs byte for byte from those sources.
- **Checked offline only.** A differential run executed the production
  WASMs next to the port on 240,000 random blocks: 0 mismatches in map
  outputs, `owner()` requests and `db_out` rows. The schema loads in
  clickhouse-local. Nothing is live-qualified yet.
- **Questionable v0.3.3 behaviour kept for the owner to decide**, listed in
  [`evm-transfers`](evm-transfers/README.md#kept-from-v033) and the map
  READMEs. Examples: `tx_index`, `log_index` and `call_index` are positions
  in each map's output, `tx_gas_used` is cumulative, and some tokens share
  `Mint`/`Burn` signatures.
- **`erc20/events`** came with its history and fixtures from
  substreams-evm-balances (formerly substreams-evm-extended).

Next: a CI job that checks committed package hashes, a release workflow,
captured-block fixtures and the `v0.8.0` release.

## License

[Apache-2.0](LICENSE)
