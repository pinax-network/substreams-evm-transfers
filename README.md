# substreams-evm-transfers

EVM transfer [Substreams](https://substreams.dev): token and native value
movements from Firehose blocks, as native protobuf and as the Token API's
ClickHouse tables.

| Package | Status | Output |
| --- | --- | --- |
| [`erc20/events`](erc20/events/README.md) | imported from substreams-evm-balances | `erc20.events.v1.Events`: standard ERC-20 `Transfer` and `Approval` log evidence from Extended blocks, with persistence scope |
| [`erc20/transfers`](erc20/transfers/README.md) | ported from substreams-evm v0.3.3 | `erc20.transfers.v1.Events`: ERC-20 `Transfer` and `Approval` logs of successful transactions, with their transaction |
| [`erc20/tokens`](erc20/tokens/README.md) | ported from substreams-evm v0.3.3 | `erc20.tokens.v1.Events`: WETH deposits and withdrawals, USDC, USDT, WBTC and SAI mints, burns and admin events, stETH events |
| [`native/transfers`](native/transfers/README.md) | ported from the v0.2.0 build that substreams-evm v0.3.3 imports | `native.transfers.v1.Events`: native value of transactions and calls, block rewards |
| `db_out` | to port | the Token API transfers tables, with the ClickHouse SQL |

## Use from Rust

Every package is also a library: call its `project` function on a decoded
`sf.ethereum.type.v2.Block` without the Substreams runtime, for example
`erc20_events::project(&block, &config)`, `erc20_transfers::project(&block)`,
`erc20_tokens::project(&block, &mut owners)` (USDT `owner()` through an
`OwnerLookup`) or `native_transfers::project(&block)`.

## Roadmap

1. Workspace, toolchain and CI; `erc20/events` with its fixtures.
2. Port substreams-evm's transfer packages to substreams 0.8 and buffa, with
   offline parity against the production v0.3.3 builds (done; captured-block
   fixtures to follow).
3. Port the transfers `db_out` and its ClickHouse SQL; check the Token API
   queries offline, then parity against the production tables.
4. Release and deploy per chain.

See [AGENTS.md](AGENTS.md) for scope, provenance and the rules of this
repository.

## License

[Apache-2.0](LICENSE)
