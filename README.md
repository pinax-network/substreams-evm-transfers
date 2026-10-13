# substreams-evm-transfers

EVM transfer [Substreams](https://substreams.dev): token and native value
movements from Firehose blocks, as native protobuf and as the Token API's
ClickHouse tables.

| Package | Status | Output |
| --- | --- | --- |
| [`erc20/events`](erc20/events/README.md) | imported from substreams-evm-balances | `erc20.events.v1.Events`: standard ERC-20 `Transfer` and `Approval` log evidence from Extended blocks, with persistence scope |
| transfers from substreams-evm | to port | ERC-20 and native transfers, wrapped-token deposits and withdrawals, stETH and related events |
| `db_out` | to port | the Token API transfers tables, with the ClickHouse SQL |

## Use from Rust

Every package is also a library: call its `project` function on a decoded
`sf.ethereum.type.v2.Block` without the Substreams runtime, for example
`erc20_events::project(&block, &config)`.

## Roadmap

1. Workspace, toolchain and CI; `erc20/events` with its fixtures.
2. Port substreams-evm's transfer packages to substreams 0.8 and buffa, with
   captured-block fixtures and offline parity against the original.
3. Port the transfers `db_out` and its ClickHouse SQL; check the Token API
   queries offline, then parity against the production tables.
4. Release and deploy per chain.

See [AGENTS.md](AGENTS.md) for scope, provenance and the rules of this
repository.

## License

[Apache-2.0](LICENSE)
