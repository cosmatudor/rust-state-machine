# Rust State Machine

A Substrate-inspired blockchain node built from scratch in Rust — starting from the
[Dot Code School state machine course](https://dotcodeschool.com/courses/in-browser-rust-state-machine)
and extended into a fully functional, multi-node P2P blockchain.

---

## What this is

The project began as an educational exercise following Dot Code School's Rust state machine
curriculum, which teaches the core concepts behind Substrate's runtime model: pallets, typed
storage, a dispatch system, and block execution. From that foundation, every layer of a real
blockchain node was added incrementally — without reaching for Substrate's existing machinery.
The goal was to understand exactly what Substrate solves and *why* it solves it the way it does.

**What was built on top of the course material:**

| Layer | What was added |
|---|---|
| Cryptography | Ed25519 signing/verification via `ed25519-dalek` |
| Encoding | Full SCALE encoding of all wire types (extrinsics, blocks) |
| Persistence | RocksDB-backed `KeyValueStore` trait; state survives restarts |
| Mempool | `(signer, nonce)`-keyed pending pool with capacity and block-limit modes |
| Networking | libp2p swarm, Noise/Yamux transport, gossipsub for blocks and extrinsics |
| Consensus | Wall-clock-aligned 20s slots, round-robin authorship (mirrors Aura) |
| RPC | Axum HTTP server: `POST /submit`, `GET /nonce/:account`, `GET /state` |
| CLI | `clap`-driven interface for starting nodes and submitting transactions |
| Parallel sig-verify | `rayon`-backed batch verification mirrors a production block pipeline |
| Proc macros | `#[macros::runtime]` and `#[macros::call]` mirror `construct_runtime!` / `#[pallet::call]` |
| Testing | 67 tests: thread-local MemStore for unit tests, tempfile RocksDB for integration tests |

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      CLI  (clap)                        │
│   start │ submit-transfer │ submit-claim │ state │ reset │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                   HTTP RPC  (axum)                      │
│    POST /submit     GET /nonce/:account    GET /state    │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                Node  (tokio async)                      │
│                                                         │
│   ┌──────────────┐  ┌────────────┐  ┌───────────────┐   │
│   │   Mempool    │  │   Ticker   │  │  P2P Network  │   │
│   │              │  │  20s slot  │  │  (libp2p      │   │
│   │ pending txs  │  │ wall-clock │  │   gossipsub)  │   │
│   │ keyed by     │  │  aligned   │  │               │   │
│   │ (signer,     │  │            │  │  topic:blocks │   │
│   │  nonce)      │  │            │  │  topic:exts   │   │
│   └──────┬───────┘  └─────┬──────┘  └───────┬───────┘   │
│          └────────────────┴──────────────────┘           │
│                           │                              │
│            Round-robin slot authorship                   │
│            sorted(peer_ids)[slot % n] → author          │
└────────────────────────┬────────────────────────────────┘
                         │ execute_block
┌────────────────────────▼────────────────────────────────┐
│                     Runtime                             │
│                                                         │
│   Pass 1: verify_batch (rayon — parallel sig checks)    │
│   Pass 2: nonce-check + dispatch (sequential)           │
│                                                         │
│   ┌──────────┐   ┌──────────┐   ┌────────────────────┐  │
│   │  System  │   │ Balances │   │  Proof-of-         │  │
│   │  pallet  │   │  pallet  │   │  Existence pallet  │  │
│   │          │   │          │   │                    │  │
│   │ nonce    │   │ transfer │   │  create / revoke   │  │
│   │ block #  │   │ balances │   │  content claims    │  │
│   └──────────┘   └──────────┘   └────────────────────┘  │
│                                                         │
│   Macro-generated: RuntimeCall enum + Dispatch impl     │
└────────────────────────┬────────────────────────────────┘
                         │ KeyValueStore trait
┌────────────────────────▼────────────────────────────────┐
│               Persistence  (RocksDB)                    │
│    Prefixed keys:  system:block_number                  │
│                    system:nonce:<account>               │
│                    balances:balance:<account>           │
│                    poe:claim:<content>                  │
└─────────────────────────────────────────────────────────┘
```

---

## Extrinsic lifecycle

This is the most Substrate-faithful part of the design. Every extrinsic goes through an
identical pipeline to what `sc-block-builder` and the executive pallet do:

```
Client                   Node (RPC)                   Runtime
  │                         │                            │
  │──POST /submit (SCALE)──►│                            │
  │                         │ SCALE::decode(bytes)       │
  │                         │ mempool.submit(ext)        │
  │                         │ gossip to peers            │
  │                         │                            │
  │                   [slot tick]                        │
  │                         │                            │
  │                         │ drain_for_block(limit)     │
  │                         │ sort by (signer, nonce)    │
  │                         │ drop nonce gaps            │
  │                         │──execute_block(block)─────►│
  │                         │                            │ inc_block_number()
  │                         │                            │ check header.block_number
  │                         │                            │
  │                         │  Pass 1 — parallel         │
  │                         │  verify_batch (rayon):     │
  │                         │    for each extrinsic:     │
  │                         │      SCALE(signer ‖ nonce ‖ call)
  │                         │      ed25519::verify(sig)  │
  │                         │                            │
  │                         │  Pass 2 — sequential:      │
  │                         │    for each ext:           │
  │                         │      if sig_err → skip     │
  │                         │      if nonce_mismatch → skip
  │                         │      inc_nonce(signer)     │
  │                         │      dispatch(caller, call)│
  │                         │      persist to RocksDB    │
```

The two-pass structure mirrors a production block author's pipeline: signature checks are
embarrassingly parallel (CPU-bound, no shared state), so they run on Rayon's thread pool.
The state transition must be sequential because each call can read state written by a
previous call in the same block.

---

## Signed payload

The bytes signed and verified for every extrinsic are:

```
SCALE( signer_pubkey_bytes [32] ‖ nonce [u32] ‖ encoded_call )
```

This ensures replay protection (nonce), binding to a specific account (pubkey), and
call integrity (the full dispatch path is covered). Changing any field after signing
causes `verify()` to return `Err("invalid signature")`.

---

## Proc macro system

Two attribute macros provide the glue that Substrate's `construct_runtime!` and
`#[pallet::call]` provide:

### `#[macros::call]`

Applied to an `impl Pallet<T>` block. Collects each `pub fn` as a call variant and
generates a `Call<T>` enum that derives `Encode + Decode`. The variant names match the
function names exactly (snake_case, matching Substrate convention).

```rust
#[macros::call]
impl<T: Config> Pallet<T> {
    pub fn transfer(caller: T::AccountId, to: T::AccountId, amount: T::Balance)
        -> DispatchResult { ... }
}
// ↓ generates
pub enum Call<T: Config> { transfer { to: T::AccountId, amount: T::Balance } }
```

### `#[macros::runtime]`

Applied to the `Runtime` struct. Inspects the pallet fields and generates:

1. `RuntimeCall` — a top-level enum with one variant per pallet,
   each wrapping that pallet's `Call<Runtime>`. Derives `Encode + Decode`.
2. `impl Dispatch for Runtime` — routes `RuntimeCall::pallet_name(call)` to
   `self.pallet_name.dispatch(caller, call)`.
3. `impl Runtime { pub fn new() }` — constructs each pallet from persistent storage.
4. `pub fn execute_block(block)` — the two-pass signature + dispatch loop.

```rust
#[macros::runtime]
pub struct Runtime {
    pub system: system::Pallet<Self>,
    pub balances: balances::Pallet<Self>,
    pub proof_of_existence: proof_of_existence::Pallet<Self>,
}
// ↓ generates RuntimeCall, Dispatch impl, ::new(), ::execute_block()
```

---

## Substrate parallels

| This project | Substrate equivalent | Notes |
|---|---|---|
| `AccountId32` — 32-byte Ed25519 pubkey | `sp_core::crypto::AccountId32` | Same type, same SCALE encoding |
| `UncheckedExtrinsic<Call>` | `sp_runtime::generic::UncheckedExtrinsic` | Same structure; Ed25519 sig over SCALE payload |
| `SCALE(signer ‖ nonce ‖ call)` signed payload | `SignedPayload` in `sp_runtime` | Same binding |
| `verify_batch` (Rayon) | `sc_block_builder` parallel sig checks | Same pipeline concept |
| `#[macros::runtime]` → `RuntimeCall` + `Dispatch` | `construct_runtime!` | Minimal reimplementation of the same idea |
| `#[macros::call]` → `Call<T>` enum | `#[pallet::call]` | Same pattern |
| `KeyValueStore` trait + `RocksDbStore` | `sp_database::Database` / `sc_client_db` | Same role; same storage engine |
| Prefixed key layout (`pallet:kind:account`) | `StorageMap` key hashing | Simpler but same idea |
| `Mempool::retain` evicts included txs | `sc_transaction_pool` pruning | Same eviction logic |
| Wall-clock 20s slots, round-robin authorship | Aura (Authority Round) | Same algorithm |
| Dev keyring (Alice/Bob/Charlie from name seeds) | `sp_keyring::AccountKeyring` | Same derivation strategy |
| Genesis: fund dev accounts, seal block #1 | `GenesisConfig` / `GenesisBuild` | Same role |
| libp2p gossipsub for blocks + extrinsics | `sc_network` (also libp2p) | Same library, same two-topic pattern |
| `POST /submit`, `GET /nonce/:account` | `author_submitExtrinsic`, `system_accountNextIndex` | Same semantics |

---

## Pallets

### System
Tracks the chain's block number and per-account nonce. Persisted to RocksDB under prefixed
keys so state survives restarts. The macro-generated `execute_block` calls `inc_block_number()`
first and validates header continuity before any dispatch happens.

### Balances
`u128` token balances per account. `transfer { to, amount }` checks for underflow (insufficient
funds) and overflow (recipient's balance wrapping). A failed dispatch is logged but does not
roll back the block — the nonce was already incremented, preventing replay of the failed tx.

### Proof of Existence
On-chain content ownership. `create_claim { claim: String }` associates a document fingerprint
(any string; in production this would be a hash) with the caller's identity. Only the original
claimer can `revoke_claim`. Attempting to claim an already-claimed document is rejected at
dispatch without affecting the claimer's nonce — the block still commits.

---

## Consensus: wall-clock round-robin

Every 20 seconds, all nodes fire a slot tick. The ticker is aligned to the slot boundary at
startup so all nodes tick in near-unison regardless of when they started:

```rust
let secs_until_next = SLOT_SECS - (now_secs % SLOT_SECS);
ticker = interval_at(now + Duration::from_secs(secs_until_next), slot_duration);
```

Both nodes independently compute the same author:

```
slot   = unix_timestamp_secs / 20
author = sorted_peer_ids[slot % num_peers]
```

Because the peer list is kept sorted identically on every node (sorted on `ConnectionEstablished`),
no coordination message is needed. Only the designated author seals and gossips a block. A node
that receives a peer block immediately evicts the included extrinsics from its mempool so it does
not produce a duplicate in the next slot.

**Fork prevention:** a node will not produce blocks until it has at least one connected peer.
A solo node advancing the chain would create an incompatible fork that peers reject on joining.

---

## Mempool and nonce handling

Pending extrinsics are queued in a `VecDeque`. At seal time, candidates are:

1. Grouped by signer
2. Sorted by nonce within each group
3. Included as a consecutive run starting from `runtime.system.nonce(signer)` — any gap in
   the sequence breaks the run (matching `txpool` semantics: a tx at nonce 5 cannot land before
   nonce 4, even if its signature is valid)

The `/nonce/:account` RPC endpoint returns `runtime_nonce + pending_count` for that account —
the same "pending nonce" semantics as `eth_getTransactionCount(account, "pending")`. This lets
a client submit several transactions in rapid succession without waiting for a block confirmation.

---

## Storage abstraction

All state reads and writes go through the `KeyValueStore` trait:

```rust
pub trait KeyValueStore {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), String>;
    fn delete(&self, key: &[u8]) -> Result<(), String>;
    fn scan_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)>;
}
```

`kv_store()` is conditionally compiled:

```rust
#[cfg(not(test))]  pub fn kv_store() -> RocksDbStore { RocksDbStore }
#[cfg(test)]       pub fn kv_store() -> MemStore      { MemStore     }
```

In tests, `MemStore` is backed by a `thread_local! { BTreeMap }`. The Rust test harness spawns
each test in its own thread, so every unit test gets a completely isolated, zero-initialised
store with no RocksDB involvement and no state leaking between tests.

---

## Testing

```
67 tests total
├── unit tests (inline, #[cfg(test)] mod tests)
│   ├── support.rs     — 16 tests (Mempool API, UncheckedExtrinsic sign/verify, verify_batch)
│   ├── system.rs      —  7 tests (block number, nonce tracking)
│   ├── balances.rs    —  7 tests (set/get, transfer, overflow, underflow)
│   └── proof_of_existence.rs — 9 tests (create, duplicate, revoke, wrong owner, reclaim)
│
└── integration tests (tests/)
    ├── encoding.rs    — 14 tests (SCALE roundtrip, sig validity, tampering, verify_batch)
    └── runtime.rs     — 14 tests (execute_block, transfers, nonce tracking, PoE, genesis)
```

**Unit test isolation** — unit tests never touch RocksDB. The `#[cfg(test)]` override of
`kv_store()` returns a thread-local `MemStore`. Each test thread gets a clean slate.

**Integration test isolation** — each test binary gets one `TempDir` via `OnceLock<TempDir>`.
`init()` calls `support::init_db_path(temp_path)` before any storage operation, so no test
binary writes to `state.db`. Tests use relative assertions (e.g. `block_number() + 1` for
the expected next height) so they remain correct regardless of the DB's starting state.

---

## Running a two-node network

**Reset (if needed)**
```bash
cargo run -- reset --db-path /tmp/node-a
cargo run -- reset --db-path /tmp/node-b
```

**Terminal 1 — Node A**
```bash
cargo run -- start \
  --port 4001 \
  --rpc-port 8000 \
  --db-path /tmp/node-a
```

**Terminal 2 — Node B** (dials Node A)
```bash
cargo run -- start \
  --port 4002 \
  --peer /ip4/127.0.0.1/tcp/4001 \
  --rpc-port 8001 \
  --db-path /tmp/node-b
```

**Terminal 3 — submit transactions**
```bash
# Single transfer (via RPC to a running node)
cargo run -- submit-transfer alice bob 500 --node http://127.0.0.1:8000

# Submit multiple txs within a slot window — they land in the same block
# because /nonce returns the pending nonce (runtime + mempool count)
cargo run -- submit-transfer alice bob 100 --node http://127.0.0.1:8000
cargo run -- submit-transfer alice bob 100 --node http://127.0.0.1:8000
cargo run -- submit-transfer alice bob 100 --node http://127.0.0.1:8000

# Proof-of-existence claim
cargo run -- submit-claim alice "hello world" --node http://127.0.0.1:8000

# Inspect chain state from the database
cargo run -- state --db-path /tmp/node-a
```

---

## CLI reference

| Command | Flags | Description |
|---|---|---|
| `start` | `--port`, `--peer`, `--rpc-port`, `--db-path` | Start a P2P node |
| `submit-transfer <from> <to> <amount>` | `--node <url>` | Transfer tokens. Without `--node`, runs a local one-shot runtime |
| `submit-claim <who> <content>` | `--node <url>` | Create a proof-of-existence claim |
| `state` | `--db-path` | Print the current runtime state from the database |
| `reset` | `--db-path` | Delete the database directory |

---

## Tech stack

- **Rust 2024 edition**
- [`tokio`](https://tokio.rs) — async runtime
- [`libp2p`](https://libp2p.io) — P2P transport (TCP + Noise + Yamux), gossipsub
- [`parity-scale-codec`](https://github.com/paritytech/parity-scale-codec) — SCALE encoding/decoding
- [`ed25519-dalek`](https://github.com/dalek-cryptography/curve25519-dalek) — Ed25519 signing and verification
- [`rocksdb`](https://rocksdb.org) — persistent key-value storage
- [`axum`](https://github.com/tokio-rs/axum) — HTTP RPC server
- [`rayon`](https://github.com/rayon-rs/rayon) — parallel signature verification
- [`clap`](https://clap.rs) — CLI argument parsing
- [`proc-macro2`](https://github.com/dtolnay/proc-macro2) / [`quote`](https://github.com/dtolnay/quote) / [`syn`](https://github.com/dtolnay/syn) — proc macro implementation

---

## Origin

The runtime core — pallets, dispatch macros, block execution — is based on the
[Dot Code School Rust State Machine course](https://dotcodeschool.com/courses/in-browser-rust-state-machine),
an excellent curriculum that teaches the internals of Substrate by building them from scratch.
Everything above the runtime (networking, consensus, persistence, RPC, CLI, testing) was added
on top to turn the teaching exercise into a running node.
