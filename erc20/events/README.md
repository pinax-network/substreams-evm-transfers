# erc20/events

A single RPC-free `map_events` reads Firehose Extended blocks and emits
**`erc20.events.v1.Events`** ([schema](../../proto/v1/erc20_events.proto)):
one row per log signed `Transfer(address,address,uint256)` or
`Approval(address,address,uint256)`, classified by encoding shape and marked
persisted or attempted. It is evidence for consumers, not balance state and
not wallet labels: send versus receive, grouping and intent stay with the
consumer, and nothing here reconstructs a balance.

Issue [#19](https://github.com/pinax-network/substreams-evm-extended/issues/19)
preferred two packages; one package with one map and two tables is used
instead because the native sink already splits `transfers` and `approvals`
into separate tables and the two share every identity rule.

## Rows

| Table | Fields | Notes |
| --- | --- | --- |
| `Transfer` | scope, transaction hash/index, call index, producer log index, block index, ordinal, emitting `token`, `shape`, `from`, `to`, `amount`, topic count, data size, `from_zero`, `to_zero`, `self_transfer`, `persisted` | `amount` is the raw integer for the ERC-20 shape and the token id for the ERC-721 shape |
| `Approval` | same identity, `owner`, `spender`, `value`, `unlimited` (2²⁵⁶−1), `zero_value`, `persisted` | an emitted event only |
| `BlockClock` | one per block | clock and row counts |

**Shape, not standard.** ERC-20 and ERC-721 `Transfer`/`Approval` share
topic0. `LOG_SHAPE_ERC20` means three topics with canonically padded
addresses and exactly 32 bytes of data; `LOG_SHAPE_ERC721` means four topics
and no data; anything else is `LOG_SHAPE_NONSTANDARD` and keeps raw sizes
without decoded participants. No emitter is asserted to be a token, and a
matching signature is not proof of a balance change: fee-on-transfer,
rebasing and silent updates are outside event evidence, and a WETH9-style
`deposit()` changes a balance without any `Transfer`
([non-Transfer corpus](../balances/docs/non-transfer-mutations.md)).

**Persisted versus attempted.** `persisted` is true for logs of non-reverted
frames in succeeded transactions and for non-reverted system calls; those
are exactly the receipt logs. Logs of reverted frames are emitted as attempts
(`persisted = false`, block index 0) unless `include_attempted` is false.

**Approval is an event.** The approve or permit call intent lives in
`evm/executions` (call selectors); the allowance storage state is not
extracted; a Permit2 authorization or an unexecuted off-chain signature is
not an ERC-20 `Approval`. Permit2 and other permission systems need their own
adapters if requested.

## Parameters

```json
{"chain_id":56,"producer_versions":[5],"include_attempted":true}
```

`producer_versions` must be a nonempty subset of the reviewed Extended versions
4 and 5. Unreviewed versions are rejected even when explicitly listed.

## Evidence

Rust tests replay the complete captured BSC block 122260950: 121 `Transfer`
and 21 `Approval` rows (all ERC-20 shaped), 110 and 17 persisted, matching
the receipt logs with those signatures exactly, 8 zero-value approvals, 17
WBNB transfers of which 14 persisted. The captured WBNB deposit shows a
balance mutation with no transfer evidence; the captured reverted-child
transaction shows the same transfer attempted at ordinal 780 and persisted at
804. Synthetic cases cover ERC-721 and nonstandard shapes, zero-address and
self transfers, unlimited and revocation-shaped approvals, failed
transactions, system calls and parameter guards.

```sh
cargo test --locked -p erc20-events
make -C erc20/events build
```

Packaging and live qualification remain paused; producer semantics on other
chains are qualified under [#8](https://github.com/pinax-network/substreams-evm-extended/issues/8).
