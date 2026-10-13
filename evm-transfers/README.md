# evm-transfers

One `db_out` writes the events of [`erc20/transfers`](../erc20/transfers/README.md),
[`erc20/tokens`](../erc20/tokens/README.md) and
[`native/transfers`](../native/transfers/README.md) into the Token API's
`evm-transfers` ClickHouse tables. A port of substreams-evm `db-evm-transfers`
at tag `evm-transfers-v0.3.3` (`d8a731d`), the `db_out` that production runs
(`<chain>:evm-transfers@v0.3.3`), to substreams 0.8.0, buffa and
`substreams-database-change` 5.0.0 (owner, 2026-10-13). The rows are v0.3.3's,
including the behaviour listed under [Kept from v0.3.3](#kept-from-v033), and
the SQL is v0.3.3's, byte for byte, so a new database can take the production
history by partition copy and the Token API queries keep working. The package
embeds the SQL, so `substreams-sink-sql setup` needs only the package.

```
params (hex | tron_base58) ─────┐
Clock ──────────────────────────┤
erc20_transfers:map_events ─────┼─▶ db_out ─▶ DatabaseChanges ─▶ substreams-sink-sql
erc20_tokens:map_events ────────┤
native_transfers:map_events ────┘

erc20_transfers, weth_*, usdc_*, usdt_issue, usdt_redeem,
usdt_destroyed_black_funds, wbtc_*, sai_*, steth_* ──mv_transfers_*──▶ transfers
transactions, calls, block_rewards ──mv_native_transfers_*──▶ native_transfers
erc20_approvals, other usdt_* tables, blocks
```

The Token API reads `transfers`, `native_transfers` and `blocks`. The tables
`db_out` writes, but for `blocks` and `block_rewards`, are created `AS`
templates with `TTL timestamp + INTERVAL 1 DAY`: they keep one day; the
materialized views keep the history.

## Rows

| Table | Key | Columns | One row per |
| --- | --- | --- | --- |
| `erc20_transfers` | `(minute, timestamp, block_num, tx_index, log_index, block_hash)` | the 18 log columns, `from`, `to`, `amount` | `Transfer` of `erc20_transfers:map_events` |
| `erc20_approvals` | same | the 18 log columns, `owner`, `spender`, `value` | `Approval` |
| 26 token tables | same | the 18 log columns, then the event's (below) | stored event of `erc20_tokens:map_events` |
| `block_rewards` | `(minute, timestamp, block_num, index, block_hash)` | the 4 clock columns, `index`, `miner`, `value`, `reason` | `BlockReward` of `native_transfers:map_events` |
| `transactions` | `(minute, timestamp, block_num, tx_index, block_hash)` | the 4 clock and 9 transaction columns | native `Transaction` |
| `calls` | `(minute, timestamp, block_num, tx_index, call_index, block_hash)` | the 4 clock and 9 transaction columns, `call_index`, `call_begin_ordinal`, `call_end_ordinal`, `call_parent_index`, `call_caller`, `call_address`, `call_value`, `call_gas_consumed`, `call_gas_limit`, `call_depth`, `call_type` | native `Call` |
| `blocks` | `block_num` | the 4 clock columns | block that wrote any other row |

The 18 log columns are those of the SQL `TEMPLATE_LOG`, without the
`MATERIALIZED` topics:

| Columns | Value |
| --- | --- |
| `block_num`, `block_hash`, `timestamp`, `minute` | the clock: number, `0x` + id, seconds, seconds / 60 (truncated) |
| `tx_index` | position of the transaction in the map's `Events.transactions` |
| `tx_hash` | hex |
| `tx_from`, `tx_to` | addresses; `tx_to` is empty for a contract creation |
| `tx_nonce`, `tx_gas_price`, `tx_gas_limit`, `tx_gas_used`, `tx_value` | verbatim |
| `log_index` | position of the log in the map's `Transaction.logs` |
| `log_address` | address |
| `log_ordinal` | verbatim |
| `log_topics`, `log_data` | hex; the topics comma-separated |

The token tables and their event columns (addresses follow the encoding,
amounts are verbatim):

| Event | Table | Columns |
| --- | --- | --- |
| WETH `Deposit`, `Withdrawal` | `weth_deposit`, `weth_withdrawal` | `dst`/`src`, `wad` |
| USDC `Mint`, `Burn` | `usdc_mint`, `usdc_burn` | `minter`, `to`, `amount`; `burner`, `amount` |
| USDT `Issue`, `Redeem` | `usdt_issue`, `usdt_redeem` | `owner`, `amount` |
| USDT `DestroyedBlackFunds` | `usdt_destroyed_black_funds` | `black_listed_user`, `balance` |
| USDT `BlockPlaced`, `BlockReleased` | `usdt_block_placed`, `usdt_block_released` | `user` |
| USDT `Mint`, `DestroyedBlockedFunds` | `usdt_mint`, `usdt_destroyed_blocked_funds` | `destination`, `amount`; `blocked_user`, `balance` |
| USDT `NewPrivilegedContract`, `RemovedPrivilegedContract` | `usdt_new_privileged_contract`, `usdt_removed_privileged_contract` | `contract` |
| USDT `LogSwapin`, `LogSwapout`, `LogChangeDCRMOwner` | `usdt_log_swapin`, `usdt_log_swapout`, `usdt_log_change_dcrm_owner` | `txhash`, `account`, `amount`; `account`, `bindaddr`, `amount`; `old_owner`, `new_owner`, `effective_height` |
| WBTC `Mint`, `Burn` | `wbtc_mint`, `wbtc_burn` | `to`, `amount`; `burner`, `value` |
| SAI `Mint`, `Burn` | `sai_mint`, `sai_burn` | `guy`, `wad` |
| stETH `Submitted` | `steth_submitted` | `sender`, `amount`, `referral` |
| stETH `TokenRebased` | `steth_token_rebased` | `report_timestamp`, `time_elapsed`, `pre_total_shares`, `pre_total_ether`, `post_total_shares`, `post_total_ether`, `shares_minted_as_fees` |
| stETH `TransferShares` | `steth_transfer_shares` | `from`, `to`, `shares_value` |
| stETH `SharesBurnt` | `steth_shares_burnt` | `account`, `pre_rebase_token_amount`, `post_rebase_token_amount`, `shares_amount` |
| stETH `ExternalSharesMinted`, `ExternalSharesBurnt` | `steth_external_shares_minted`, `steth_external_shares_burnt` | `recipient`/`owner`, `amount_of_shares` |

The other token events (USDC and USDT admin events, `Pause`, `Unpause`,
`OwnershipTransferred`, WBTC `MintFinished` and `OwnershipRenounced`, SAI
`LogSetAuthority` and `LogSetOwner`, the other stETH events) are stored
nowhere, as in v0.3.3.

- **Addresses.** The `db_out` param selects the encoding, `hex` by default:
  `0x` + lowercase hex, or with `tron_base58` Tron Base58Check (`T...`), where
  a 20-byte value gets the `0x41` version byte, a 21-byte one must start with
  it, and any other length is written as an empty string. Any other param
  fails the block. Hashes, topics and data are hex in both encodings.
- **Operation and order.** Every row is a CREATE, which ClickHouse inserts:
  ERC-20 rows, token rows, block rewards, then each native transaction
  followed by its calls, then the `blocks` row. A block without rows writes
  nothing.
- **Fields.** Sorted by name: `substreams-database-change` keeps a row in a
  `HashMap`, in 5.0.0 as in 3.0.0, and the sink matches columns by name. The
  composite keys stay in map order.
- **No chain id.** No table stores one: use one database per network.

## Host API

`evm_transfers::changes(&Clock, &transfers_pb::Events, &tokens_pb::Events, &native_pb::Events, Encoding) -> Result<DatabaseChanges, Error>`
is the `db_out` body without the Substreams runtime, for a host program that
runs the three maps' `project` itself. `Encoding::from_param(&str)` parses the
param; `transfers_pb`, `tokens_pb` and `native_pb` re-export the input
messages. It fails, where v0.3.3 panicked, on a clock without a timestamp in
a block with rows.

## Kept from v0.3.3

The port writes v0.3.3's rows exactly, including behaviour that looks wrong.
Changing any of it is a separate change the owner approves.

1. **Positions, not chain indexes.** `tx_index`, `log_index` and
   `call_index` are positions in each map's output, and they key the rows.
   The same transaction has one `tx_index` in the ERC-20 tables and another
   in the token tables and in `transactions`; a token log that two tokens
   decode (`Mint(address,uint256)`, `Burn(address,uint256)`) gives rows with
   consecutive `log_index`.
2. **`tx_gas_used`** is the receipt's cumulative gas in the ERC-20 and token
   tables, and the transaction's own gas in `transactions` and `calls` (the
   maps' values).
3. **`calls.call_type`** is always `CALL_TYPE_UNSPECIFIED`: v0.3.3's `db_out`
   reads `Call.call_type`, which the native map it imports (`aaae9dd`) never
   sets.
4. **`withdrawals`, `selfdestructs`, `genesis_balances` and `dao_transfers`**
   stay empty: the schema creates them, and the `db_out` of v0.3.3 had loops
   for them, but its native map has no such messages.
5. **`usdt_log_swapin.txhash`** is written with the address encoding: hex
   normally, empty with `tron_base58` (32 bytes are no Tron address).
6. **`block_rewards.reason`** is the proto name; a value outside the enum is
   `REASON_UNKNOWN` (the native map writes reward reasons only).
7. **Rows the views drop.** Zero amounts and self transfers are stored in the
   base tables; the views apply v0.3.3's filters (`amount > 0`, `from != to`
   where v0.3.3 had it).
8. **Maps.** The receipt-log fallback on BASE blocks, the USDT `owner()` call
   and the shared signatures belong to the maps; see their READMEs.

## Port from v0.3.3

Every file comes from substreams-evm `d8a731d` (tag `evm-transfers-v0.3.3`)
unless noted:

| File | Source | Changes |
| --- | --- | --- |
| `src/lib.rs` | `db-evm-transfers/src/lib.rs` | `db_out` in a wasm32-only module over `changes`; errors instead of panics; fields sorted; `substreams-database-change` 5.0.0 |
| `src/columns.rs` | `db-evm-transfers/src/lib.rs` (`set_clock`, `log_key`, `tx_key`, `call_key`, `reward_key`), `logs.rs`, `transactions.rs` | one `set_transaction` for the three identical `set_template_*_tx` |
| `src/erc20_transfers.rs`, `src/erc20_tokens.rs`, `src/native_transfers.rs` | the same files in `db-evm-transfers/src/` | the token rows as one table of events; the native `withdrawals`, `selfdestructs`, `genesis_balances` and `dao_transfers` loops are not ported (item 4 above); `call_type` is the constant production writes (item 3) |
| `src/encoding.rs` | `common/src/lib.rs` | `Encoding`, `handle_encoding_param` (as `Encoding::from_param`, an error instead of a panic), `bytes_to_hex`, `bytes_to_string`, `tron_base58_from_bytes` |
| `clickhouse/schema.*.sql` (12 files) | `db-evm-transfers-clickhouse/` | none (byte-identical; sha256 values in [`tests/db_out.rs`](tests/db_out.rs)) |
| `clickhouse/schema.sql` | the schema embedded in production `evm-clickhouse-transfers-v0.3.3.spkg` | generated by `make schema`, not committed: 32,418 bytes, sha256 `6ef74c5a…`, MD5 `af97e869…`, the same bytes |
| `substreams.yaml` | `db-evm-transfers/substreams.yaml` and `db-evm-transfers-clickhouse/substreams.yaml` | one manifest, `evm_transfers` v0.8.0 with the `sink:` section; imports the three local manifests instead of v0.3.3's committed spkgs; same module, inputs and `hex` default |
| `Makefile` | `db-evm-transfers/Makefile` and `db-evm-transfers-clickhouse/Makefile` | `build`, `schema` (the same recipe, in byte order) and `pack` only |
| `proto/sf/substreams/sink/database/v1/database.proto` | `substreams-database-change` 5.0.0 ([`dae3f279`](https://github.com/streamingfast/substreams-sink-database-changes/blob/dae3f279f3d93518ac0f81e1ef71b82b4f7ec1b7/proto/sf/substreams/sink/database/v1/database.proto)) | none (byte-identical, sha256 `122db4ce…`) |

The Base58 test vectors come from `common/tests/address_tests.rs`.

Deviations from v0.3.3, none of which changes a stored row:
- **Errors instead of panics** on a param other than `hex`, `tron_base58` or
  empty, and on a clock without a timestamp in a block with rows. The block
  fails either way.
- **`substreams-database-change` 5.0.0** (buffa, substreams 0.8.0) in place of
  3.0.0. On the wire, field 2 of `Field` keeps its number (`new_value` in
  3.0.0, `value` in 5.0.0), field 3 (`old_value`) was always empty, and every
  field now also carries `update_op = UPDATE_OP_SET` (field 4). The SQL sink
  uses only the name and value of a CREATE's fields. Fields are sorted by name.
- **Descriptors.** The manifest compiles `DatabaseChanges` from the 5.0.0
  source above, so the package describes `value` and `update_op`; the CLI adds
  `sf.substreams.sink.sql.v1.Service` from its built-in
  `sf/substreams/sink/sql/v1/deprecated.proto` (it refuses that path in
  `protobuf.files`). v0.3.3 imported `substreams-database-change-v2.0.0.spkg`
  and `substreams-sink-sql-protodefs-v1.0.7.spkg` instead.

Not ported: the PostgreSQL variant (`db-evm-transfers-postgres`),
`db-evm-transfers-clickhouse/examples/` (they query tables and databases the
v0.3.3 schema does not define) and `migrate/v0.2.1-patch.sql`, the wrapper
manifest, the image and the Makefiles' live targets.

The production build is the committed `spkg/evm-transfers-v0.3.3.spkg` of
`d8a731d`, module `81ed3876…`; Rust 1.88.0 rebuilds its WASM byte for byte
from `d8a731d` (sha256 `ecd489c9…`, with the original checkout path
remapped, since its `proto` crate embeds absolute paths).

## Validation

`cargo test -p evm-transfers` pins the 12 SQL files, the generated schema and
the vendored proto by sha256, checks the columns of every table in both
encodings, the errors, the three maps feeding `db_out`, the manifest wiring and
that every table and column written exists in the schema. The pull request
that added this package reports the offline checks: a differential run against
the production v0.3.3 `db_out` binary, and the rows loaded into the schema in
`clickhouse-local` with the Token API queries. No live run backs this package.

## Build

From the repository root, with the pinned toolchain:

```sh
make -C evm-transfers build    # the three maps' and evm-transfers' WASM, one cargo build each
make -C evm-transfers schema   # clickhouse/schema.sql
make -C evm-transfers pack     # out/evm-transfers-v0.8.0.spkg
```

`substreams pack` embeds the imported packages' WASM without building it, and
it reads `clickhouse/schema.sql`, so `pack` runs both. The four WASMs are
built one package at a time: built together, `substreams-database-change`
turns on buffa's `json` feature for the imported maps too, and their WASM and
module hashes would differ from the map packages'. Committed spkgs come from
CI's Linux build, never a local pack.

The Token API runs one database per chain, `<chain>:evm-transfers@<version>`,
from the release spkg; v0.8.0 is a new database next to
`<chain>:evm-transfers@v0.3.3`. The `erc20_tokens` map calls `owner()` through
Substreams RPC, like v0.3.3, so the endpoint needs RPC enabled. Each live step
(a run, a sink, a partition copy) needs the owner's OK.
