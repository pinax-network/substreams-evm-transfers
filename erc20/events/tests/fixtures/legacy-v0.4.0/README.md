# ERC-20 legacy v0.4.0 differential corpus

This offline test executes the actual historical `erc20_transfers:map_events`
and `db_out` binaries, then calls the current `erc20_events::project` on the
same wire-encoded Firehose block. It compares a defined eight-column ERC-20
row projection and records known differences instead of treating them as
parity. It does not change either decoder, qualify a live source, or cover the
other tables of the historical package.

## Immutable oracle

The binaries were extracted without rebuilding from
[`spkg/evm-transfers-v0.4.0.spkg`](https://github.com/pinax-network/substreams-evm/blob/d0484a4e9fd5654f03e0b06053ed388fafbe5d3e/spkg/evm-transfers-v0.4.0.spkg)
at revision `d0484a4e9fd5654f03e0b06053ed388fafbe5d3e` (tag
`evm-transfers-v0.4.0`). These are historical test fixtures, not a newly packed
release. The test verifies each binary's SHA-256 before loading.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Original SPKG | 1888292 | `5ed10d526d71614d54067afda169b51ee36d0d513cfc84f50c6af2ca5c636d71` |
| `erc20_transfers.wasm`, binary index 1, export `map_events` | 358108 | `5d6a71d3d6eeb1b91cef2d599b61c54f145c936c6789a5456a69c281ca5f6126` |
| `db_out.wasm`, binary index 0, export `db_out` | 381979 | `111f1d391d6840c6581cca5ffe5554456a1dc60ff7ce478bc26b0700778d3091` |

This establishes the exact package/binary baseline, not a reproducible
source-to-WASM build proof. To verify extraction, pass a local copy of that
exact SPKG to `python3 extract_oracle.py /path/to/evm-transfers-v0.4.0.spkg`.
The extractor rejects a different package, module mapping or binary hash and
performs no download.

The native reference is owning-repository commit
`75cc91a606d2bf3d006bd2675e106d02936adf6e`. Its `erc20/events/src/lib.rs` has
SHA-256 `aebe5be87dcf5d285ebcdc6ce4293464e0e9a3ab00ba8273e676329d9e58e32c`,
identical to the same file at earlier public extraction revision
`3fdba9567fd75c11e58600f224c33b1722d1acbf` in
`pinax-network/substreams-evm-extended`. The corpus records the actual current
core hash, so a source update requires an explicit corpus review.

## Compared behavior

The historical map receives the exact block protobuf. Its actual output is
passed to historical `db_out` with `hex` encoding and a block-derived Clock.
Token-specific and native-transfer sibling outputs are empty: they write
different tables and are outside this ERC-20 comparison.

The eight compared string columns are `log_address`, `log_ordinal`, `tx_hash`,
`tx_from`, `tx_to`, `from`, `to`, and `amount`. Historical DBChanges creation
order is preserved, including transaction order before per-transaction log
ordinals. The native comparison selects persisted, transaction-scope,
ERC20-shaped transfers and projects only these columns. No ABI logic is
copied. Native evidence counts retain attempted, system-scope, ERC721 and
nonstandard observations separately. Both `include_attempted` settings are
checked to produce the same persisted evidence.

The 30 synthetic cases use chain ID 1 and producer versions 4/5; most blocks
use version 5, with explicit version 4 acceptance and version 3 rejection.

| Boundary | Historical result | Native result |
| --- | --- | --- |
| Canonical, max uint256, mint/burn, zero/self, creation recipient | Exact rows | Exact rows |
| Call/receipt duplicates or disagreement | Call logs win | Call logs win |
| Cross-transaction ordinals and unsorted per-transaction logs | Transaction, then log order | Same projected order |
| Failed/reverted transaction or reverted call | No persisted row | Attempt evidence; no persisted row |
| Call failure flags without `state_reverted` | Row retained | Row retained |
| Nonzero address padding or 33-byte address topic | First word's low 20 bytes retained | Nonstandard evidence; no compared row |
| Short address topic, wrong data length, extra indexed topic | No row | Nonstandard evidence; no compared row |
| ERC721 shape | No ERC-20 row | ERC721 evidence; no compared row |
| Missing successful receipt, even without matching logs | Registered package panic | Call evidence accepted |
| Missing failed receipt with calls | No row | Attempt evidence; no persisted row |
| Base receipt-only or Extended receipt-only block | Receipt rows | Explicit source rejection |
| Failed transaction without calls or unknown status | Transaction skipped | Explicit source rejection |
| Unreviewed producer version 3 | Rows | Explicit source rejection |
| System call plus transaction | Transaction row only | System evidence retained; transaction projection matches |

Public `erc20_events::project` is the native oracle. Consumers may impose
additional structure checks, for example rejecting malformed topic widths or
inconsistent transaction indices. Such rejection requires separate
qualification; a nonstandard observation does not mean the public API rejects
the whole block.

A separate test replays existing captured BSC block 122260950 with chain ID
56. The historical binary and native projection agree on all 110 persisted
ERC-20 rows; native evidence contains 121 transfers including 11 attempts.
This is one retained captured-block check, not general network qualification.

## Frozen consumer interface

[`corpus.json`](corpus.json) is deterministic. Each case contains:

- `name` and `difference`: stable inventory and explicit boundary classification.
- `block_hex`: exact `sf.ethereum.type.v2.Block` bytes, hex without a prefix.
- `legacy.outcome`: `rows` or `panic`; `legacy.rows` contains the eight columns.
- `legacy.db_out_hex`: exact historical `DatabaseChanges` bytes, or null on
  panic. Consumers can exercise their real database normalizer on these bytes.
- `native.outcome`: `rows` or `reject`; `native.rows`, `native.error` and
  `native.evidence` record the public API result and evidence counts.

Empty rows remain a successful result. Package panic, source rejection and
skipped evidence are distinct. Historical `db_out` can include other tables
such as `blocks`; consumers must select the table they are qualifying. Policy,
source and artifact hashes are part of the interface. Synthetic blocks contain
no credentials, endpoints or captured user input.

Consumers can pin/copy this JSON and test their decode-plus-projection path
without adding the interpreter or historical WASM to production dependencies.
Require an exact case inventory and classify additional rejection explicitly.
An expected difference is not permission to change production output.

## Running and updating

```sh
cargo test --locked -p erc20-events --test legacy_v04
```

The test-only Wasmi host allows only `logger.println`, `env.output`,
`env.register_panic`, and `env.skip_empty_output`. It has no WASI, stores, RPC,
network or filesystem imports. Each module gets a fresh instance, at most
64 MiB memory, one billion fuel units, 4 MiB per input/output, and one output.
Logger/panic text is not retained. Unexpected imports, traps, fuel exhaustion,
duplicate outputs and oversized outputs fail; only a registered historical
panic is classified as the expected package outcome.

The normal test never writes fixtures. After reviewing a deliberate change,
this explicit ignored export test first re-runs all handwritten assertions:

```sh
cargo test --locked -p erc20-events --test legacy_v04 export_reviewed_corpus -- --ignored
```

Review the resulting JSON and source/artifact provenance before committing.
No decoder semantics, ABI code, deployment or live inputs change in this work.
