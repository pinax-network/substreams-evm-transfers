# Repository guidance

EVM transfer Substreams: ERC-20 transfers and approvals, native value
transfers, wrapped-token deposits and withdrawals, stETH and similar
token-movement events from Firehose blocks. Created on 2026-10-13 at the
owner's direction, next to `pinax-network/substreams-evm-balances` and
`pinax-network/substreams-evm-dex`, taking the transfer code of
`pinax-network/substreams-evm` and `erc20/events` from substreams-evm-balances.

## Scope and formats

- Every package emits **native protobuf** (`map_*` modules), so native
  Substreams ClickHouse/Postgres sinks can consume it directly later.
- For the Token API today, a `db_out` writes the ClickHouse tables the Token
  API reads, with the SQL embedded in its manifest's `sink:` section.
- Every package exposes plain-Rust decoding (`project(...)`) that a host
  program such as `firehose-parquet` calls on a decoded `Block` without the
  Substreams runtime. Handlers compile only for wasm32
  (`#[cfg(target_arch = "wasm32")] mod handler`).
- `erc20_events::project(&Block, &Config)` and the `erc20.events.v1`
  protobuf are a public contract: other Pinax services pin them by commit.
  Tell them before changing a signature, a protobuf name or field, or output
  semantics (persisted and system-call scope, failed or reverted
  transactions, log ordinals, nonstandard Transfer shapes).
- Contract ABIs and generated decoders come from `substreams-abis` (tag
  `v2.0.0`); do not vendor ABI code here.

## Provenance

- `erc20/events`, its protos and its three captured BSC fixtures were
  imported with their history from substreams-evm-balances (formerly
  substreams-evm-extended, `main` at `ca00aba`); commit messages name that
  repository's pull requests explicitly. The fixtures moved from
  `erc20/balances/tests/fixtures` to `erc20/events/tests/fixtures`.
- Transfer packages ported from substreams-evm record their source commit in
  their package README.

## Toolchain and validation

`rust-toolchain.toml` pins Rust 1.99.0 exactly (a point release: the WASM
embeds `/rustc/<commit>`). The workspace is on substreams 0.8.0,
substreams-ethereum 0.12.0 and buffa 0.9.2. `proto/src/pb` is generated
from `proto/v1` by `buf generate` in `proto/`; only `pb/mod.rs` is written by
hand. Package Makefiles build WASM with `--locked` and the `RUSTFLAGS` of
`build.mk`, which remap the checkout, `CARGO_HOME` and the Rust sources.

Offline validation, all with warnings denied: formatting, workspace tests,
Clippy for all targets on the host and wasm32, a wasm32 check and rustdoc.
Keep regression tests in Rust inside the package crates, with captured-block
fixtures; verify new work ad hoc and report the method and results in the
pull request.

## Releases and deployment

Releases are plain `v<version>` tags whose versions do not collide with the
Token API's existing `<chain>:evm-transfers@<version>` databases. The Token
API live sink takes the release's spkg URL as `SPKG_URL`. Committed spkgs
come from CI's Linux build, never a local pack.

## Live use

Every live Substreams, Firehose, RPC or sink step needs the owner's explicit
OK for that step. Keep credentials in environment variables only, and never
write internal hostnames, endpoints or secrets into this public repository.
