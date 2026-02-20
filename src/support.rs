use core::fmt;
use once_cell::sync::Lazy;
use parity_scale_codec::{Decode, Encode};
use rocksdb::{DB, IteratorMode, Options};
use std::sync::OnceLock;

/// Override the RocksDB path before any storage operation is performed.
/// Defaults to `"state.db"` in the current working directory.
/// Panics if called after the database has already been opened.
pub fn init_db_path(path: &str) {
	DB_PATH
		.set(path.to_string())
		.expect("DB path already initialised — call init_db_path before any storage operation");
}

pub fn db_path() -> &'static str {
	DB_PATH.get().map(|s| s.as_str()).unwrap_or("state.db")
}

static DB_PATH: OnceLock<String> = OnceLock::new();

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Encode, Decode)]
pub struct AccountId32(pub [u8; 32]);

impl AccountId32 {
	pub fn as_bytes(&self) -> &[u8; 32] {
		&self.0
	}
}

impl From<[u8; 32]> for AccountId32 {
	fn from(bytes: [u8; 32]) -> Self {
		Self(bytes)
	}
}

impl fmt::Debug for AccountId32 {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let hex: String = self.0.iter().map(|b| format!("{b:02x}")).collect();
		write!(f, "0x{}…{}", &hex[..8], &hex[56..])
	}
}

#[derive(Clone, Encode, Decode)]
pub struct Block<Header, Extrinsic> {
	pub header: Header,
	pub extrinsics: Vec<Extrinsic>,
}

#[derive(Clone, Encode, Decode)]
pub struct Header<BlockNumber> {
	pub block_number: BlockNumber,
}

#[derive(Encode, Decode)]
pub struct UncheckedExtrinsic<Call> {
	/// Ed25519 public key of the sender
	pub signer: AccountId32,
	/// Ed25519 signature over SCALE(`signer.0 ‖ nonce ‖ call`).
	pub signature: [u8; 64],
	pub nonce: u32,
	pub call: Call,
}

impl<Call: Encode> UncheckedExtrinsic<Call> {
		pub fn new_signed(sk: &ed25519_dalek::SigningKey, nonce: u32, call: Call) -> Self {
		use ed25519_dalek::Signer;
		let signer = AccountId32(*sk.verifying_key().as_bytes());
		let payload = (signer.as_bytes(), nonce, &call).encode();
		let signature = sk.sign(&payload).to_bytes();
		Self { signer, signature, nonce, call }
	}

	pub fn verify(&self) -> DispatchResult {
		use ed25519_dalek::Verifier;
		let vk = ed25519_dalek::VerifyingKey::from_bytes(self.signer.as_bytes())
			.map_err(|_| "invalid public key")?;
		let sig = ed25519_dalek::Signature::from_bytes(&self.signature);
		let payload = (self.signer.as_bytes(), self.nonce, &self.call).encode();
		vk.verify(&payload, &sig).map_err(|_| "invalid signature")
	}
}

impl<Call: fmt::Debug> fmt::Debug for UncheckedExtrinsic<Call> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let sig_hex: String = self.signature.iter().map(|b| format!("{b:02x}")).collect();
		f.debug_struct("UncheckedExtrinsic")
			.field("signer", &self.signer)
			.field("signature", &format!("0x{}…{}", &sig_hex[..8], &sig_hex[120..]))
			.field("nonce", &self.nonce)
			.field("call", &self.call)
			.finish()
	}
}

/// Verify all extrinsics in parallel using Rayon. Returns one `DispatchResult` per extrinsic
/// in the same order. This mirrors a block-author's ability to pipeline signature checks
/// across CPU cores before sequential state-transition.
pub fn verify_batch<Call>(exts: &[UncheckedExtrinsic<Call>]) -> Vec<DispatchResult>
where
	Call: Encode + Sync,
{
	use rayon::prelude::*;
	exts.par_iter().map(|e| e.verify()).collect()
}

pub type DispatchResult = Result<(), &'static str>;

pub trait KeyValueStore {
	fn get(&self, key: &[u8]) -> Option<Vec<u8>>;
	fn put(&self, key: &[u8], value: &[u8]) -> Result<(), String>;
	fn delete(&self, key: &[u8]) -> Result<(), String>;
	fn scan_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)>;
}

static ROCKS_DB: Lazy<DB> = Lazy::new(|| {
	let mut opts = Options::default();
	opts.create_if_missing(true);
	DB::open(&opts, db_path())
		.unwrap_or_else(|e| panic!("failed to open RocksDB at '{}': {e}", db_path()))
});

pub struct RocksDbStore;

impl KeyValueStore for RocksDbStore {
	fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
		ROCKS_DB.get(key).ok().flatten().map(|v| v.to_vec())
	}

	fn put(&self, key: &[u8], value: &[u8]) -> Result<(), String> {
		ROCKS_DB.put(key, value).map_err(|e| e.to_string())
	}

	fn delete(&self, key: &[u8]) -> Result<(), String> {
		ROCKS_DB.delete(key).map_err(|e| e.to_string())
	}

	fn scan_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
		let mode = IteratorMode::Start;
		ROCKS_DB
			.iterator(mode)
			.filter_map(|res| res.ok())
			.filter_map(
				|(k, v)| {
					if k.starts_with(prefix) { Some((k.to_vec(), v.to_vec())) } else { None }
				},
			)
			.collect()
	}
}

#[cfg(not(test))]
pub fn kv_store() -> RocksDbStore {
	RocksDbStore
}

/// In-memory store used by all unit tests.
///
/// Each test runs in its own thread (Rust test harness uses `thread::spawn` per test),
/// so the thread-local gives every test a completely isolated, zero-initialised store —
/// no RocksDB, no leftover chain state bleeding between tests.
#[cfg(test)]
pub fn kv_store() -> test_store::MemStore {
	test_store::MemStore
}

#[cfg(test)]
pub mod test_store {
	use super::KeyValueStore;
	use std::{cell::RefCell, collections::BTreeMap};

	thread_local! {
		static MEM: RefCell<BTreeMap<Vec<u8>, Vec<u8>>> = RefCell::new(BTreeMap::new());
	}

	pub struct MemStore;

	impl KeyValueStore for MemStore {
		fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
			MEM.with(|m| m.borrow().get(key).cloned())
		}

		fn put(&self, key: &[u8], value: &[u8]) -> Result<(), String> {
			MEM.with(|m| m.borrow_mut().insert(key.to_vec(), value.to_vec()));
			Ok(())
		}

		fn delete(&self, key: &[u8]) -> Result<(), String> {
			MEM.with(|m| m.borrow_mut().remove(key));
			Ok(())
		}

		fn scan_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
			MEM.with(|m| {
				m.borrow()
					.iter()
					.filter(|(k, _)| k.starts_with(prefix))
					.map(|(k, v)| (k.clone(), v.clone()))
					.collect()
			})
		}
	}
}

/// Separates "received" txs from "applied" ones; only drained at seal time.
#[derive(Debug, Default)]
pub struct Mempool<Extrinsic> {
	pending: std::collections::VecDeque<Extrinsic>,
	max_capacity: Option<usize>,
	/// How many extrinsics constitute a full block. When `pending.len() >= block_limit`
	/// the node should seal and execute a new block automatically.
	block_limit: Option<usize>,
}

#[derive(Debug)]
pub struct MempoolFull;

impl<Extrinsic> Mempool<Extrinsic> {
		pub fn new() -> Self {
		Self { pending: std::collections::VecDeque::new(), max_capacity: None, block_limit: None }
	}

	/// New mempool that rejects new extrinsics when `max_len` is reached.
	pub fn with_capacity(max_len: usize) -> Self {
		Self {
			pending: std::collections::VecDeque::new(),
			max_capacity: Some(max_len),
			block_limit: None,
		}
	}

	/// New mempool that auto-signals block-seal when `block_limit` extrinsics are pending.
	pub fn with_block_limit(block_limit: usize) -> Self {
		Self {
			pending: std::collections::VecDeque::new(),
			max_capacity: None,
			block_limit: Some(block_limit),
		}
	}

	/// Returns `true` when enough extrinsics have accumulated to fill a block.
	pub fn is_block_ready(&self) -> bool {
		self.block_limit.is_some_and(|limit| self.pending.len() >= limit)
	}

	pub fn block_limit(&self) -> Option<usize> {
		self.block_limit
	}

	/// Used by the RPC nonce handler to count pending txs per account.
	pub fn pending_extrinsics(&self) -> impl Iterator<Item = &Extrinsic> {
		self.pending.iter()
	}

	/// Add an extrinsic. Returns `Err(MempoolFull)` if at capacity.
	pub fn submit(&mut self, ext: Extrinsic) -> Result<(), MempoolFull> {
		if let Some(max) = self.max_capacity {
			if self.pending.len() >= max {
				return Err(MempoolFull);
			}
		}
		self.pending.push_back(ext);
		Ok(())
	}

	/// Take up to `n` extrinsics from the front for block inclusion.
	pub fn drain_for_block(&mut self, n: usize) -> Vec<Extrinsic> {
		let mut exts = Vec::new();
		for _ in 0..n {
			match self.pending.pop_front() {
				Some(ext) => exts.push(ext),
				None => break,
			}
		}
		exts
	}

	/// Remove the extrinsic at index (0-based). Use when a tx is invalid.
	pub fn remove(&mut self, index: usize) -> Option<Extrinsic> {
		if index < self.pending.len() { self.pending.remove(index) } else { None }
	}

	pub fn len(&self) -> usize {
		self.pending.len()
	}

	pub fn is_empty(&self) -> bool {
		self.pending.is_empty()
	}

	/// Keep only extrinsics for which `f` returns `true`. Used to evict txs that
	/// were already included in a peer block so we don't produce a duplicate.
	pub fn retain<F>(&mut self, f: F)
	where
		F: FnMut(&Extrinsic) -> bool,
	{
		self.pending.retain(f);
	}
}

impl<Extrinsic> Clone for Mempool<Extrinsic>
where
	Extrinsic: Clone,
{
	fn clone(&self) -> Self {
		Self {
			pending: self.pending.clone(),
			max_capacity: self.max_capacity,
			block_limit: self.block_limit,
		}
	}
}

pub trait Dispatch {
	type Caller;
	type Call;

	fn dispatch(&mut self, caller: Self::Caller, call: Self::Call) -> DispatchResult;
}

/// Dev keyring — mirrors `sp_keyring::AccountKeyring` from the Substrate ecosystem.
///
/// Each variant derives a deterministic Ed25519 key from the UTF-8 encoding of the
/// variant name zero-padded to 32 bytes. Only for development / testing.
pub mod keyring {
	use ed25519_dalek::SigningKey;

	#[derive(Clone, Copy, Debug)]
	pub enum AccountKeyring {
		Alice,
		Bob,
		Charlie,
	}

	impl AccountKeyring {
		fn seed(self) -> [u8; 32] {
			let mut seed = [0u8; 32];
			let name: &[u8] = match self {
				Self::Alice => b"Alice",
				Self::Bob => b"Bob",
				Self::Charlie => b"Charlie",
			};
			seed[..name.len()].copy_from_slice(name);
			seed
		}

		pub fn signing_key(self) -> SigningKey {
			SigningKey::from_bytes(&self.seed())
		}

		pub fn public(self) -> super::AccountId32 {
			super::AccountId32(*self.signing_key().verifying_key().as_bytes())
		}
	}

	pub fn from_name(name: &str) -> Option<AccountKeyring> {
		match name.to_lowercase().as_str() {
			"alice" => Some(AccountKeyring::Alice),
			"bob" => Some(AccountKeyring::Bob),
			"charlie" => Some(AccountKeyring::Charlie),
			_ => None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use keyring::AccountKeyring::{Alice, Bob};
	use parity_scale_codec::Encode;

	// -----------------------------------------------------------------------
	// Mempool
	// -----------------------------------------------------------------------

	#[test]
	fn mempool_new_is_empty() {
		let pool: Mempool<i32> = Mempool::new();
		assert!(pool.is_empty());
		assert_eq!(pool.len(), 0);
	}

	#[test]
	fn mempool_submit_and_drain_all() {
		let mut pool: Mempool<i32> = Mempool::new();
		pool.submit(1).unwrap();
		pool.submit(2).unwrap();
		pool.submit(3).unwrap();
		let batch = pool.drain_for_block(10);
		assert_eq!(batch, vec![1, 2, 3]);
		assert!(pool.is_empty());
	}

	#[test]
	fn mempool_drain_partial_leaves_remainder() {
		let mut pool: Mempool<i32> = Mempool::new();
		for i in 0..5 {
			pool.submit(i).unwrap();
		}
		let batch = pool.drain_for_block(3);
		assert_eq!(batch, vec![0, 1, 2]);
		assert_eq!(pool.len(), 2);
	}

	#[test]
	fn mempool_drain_from_empty_returns_empty_vec() {
		let mut pool: Mempool<i32> = Mempool::new();
		assert_eq!(pool.drain_for_block(10), Vec::<i32>::new());
	}

	#[test]
	fn mempool_capacity_rejects_overflow() {
		let mut pool: Mempool<i32> = Mempool::with_capacity(2);
		assert!(pool.submit(1).is_ok());
		assert!(pool.submit(2).is_ok());
		assert!(pool.submit(3).is_err());
		assert_eq!(pool.len(), 2);
	}

	#[test]
	fn mempool_block_limit_signals_correctly() {
		let mut pool: Mempool<i32> = Mempool::with_block_limit(2);
		assert!(!pool.is_block_ready());
		pool.submit(1).unwrap();
		assert!(!pool.is_block_ready());
		pool.submit(2).unwrap();
		assert!(pool.is_block_ready());
		pool.drain_for_block(2);
		assert!(!pool.is_block_ready());
	}

	#[test]
	fn mempool_retain_evicts_matching() {
		let mut pool: Mempool<i32> = Mempool::new();
		for i in 0..5 {
			pool.submit(i).unwrap();
		}
		pool.retain(|x| x % 2 == 0); // keep evens
		let batch = pool.drain_for_block(10);
		assert_eq!(batch, vec![0, 2, 4]);
	}

	#[test]
	fn mempool_remove_by_index() {
		let mut pool: Mempool<i32> = Mempool::new();
		pool.submit(10).unwrap();
		pool.submit(20).unwrap();
		pool.submit(30).unwrap();
		assert_eq!(pool.remove(1), Some(20));
		assert_eq!(pool.len(), 2);
		assert_eq!(pool.drain_for_block(10), vec![10, 30]);
	}

	#[test]
	fn mempool_remove_out_of_bounds_returns_none() {
		let mut pool: Mempool<i32> = Mempool::new();
		pool.submit(1).unwrap();
		assert_eq!(pool.remove(5), None);
		assert_eq!(pool.len(), 1);
	}

	#[test]
	fn mempool_pending_extrinsics_iter() {
		let mut pool: Mempool<i32> = Mempool::new();
		pool.submit(10).unwrap();
		pool.submit(20).unwrap();
		let items: Vec<_> = pool.pending_extrinsics().collect();
		assert_eq!(items, vec![&10, &20]);
	}

	// -----------------------------------------------------------------------
	// UncheckedExtrinsic — signing & verification
	// -----------------------------------------------------------------------

	#[derive(Encode)]
	struct TestCall(u32);

	#[test]
	fn new_signed_produces_valid_extrinsic() {
		let sk = Alice.signing_key();
		let ext = UncheckedExtrinsic::new_signed(&sk, 0, TestCall(42));
		assert_eq!(ext.signer, Alice.public());
		assert_eq!(ext.nonce, 0);
		assert!(ext.verify().is_ok());
	}

	#[test]
	fn verify_rejects_tampered_nonce() {
		let sk = Alice.signing_key();
		let mut ext = UncheckedExtrinsic::new_signed(&sk, 0, TestCall(1));
		ext.nonce = 99;
		assert!(ext.verify().is_err());
	}

	#[test]
	fn verify_rejects_wrong_signer_field() {
		let sk = Alice.signing_key();
		let mut ext = UncheckedExtrinsic::new_signed(&sk, 0, TestCall(1));
		// Swap the signer field to Bob's public key — payload won't match.
		ext.signer = Bob.public();
		assert!(ext.verify().is_err());
	}

	#[test]
	fn nonces_produce_different_signatures() {
		let sk = Alice.signing_key();
		let ext0 = UncheckedExtrinsic::new_signed(&sk, 0, TestCall(1));
		let ext1 = UncheckedExtrinsic::new_signed(&sk, 1, TestCall(1));
		assert_ne!(ext0.signature, ext1.signature);
	}

	// -----------------------------------------------------------------------
	// verify_batch
	// -----------------------------------------------------------------------

	#[test]
	fn verify_batch_all_valid() {
		let sk = Alice.signing_key();
		let exts: Vec<_> =
			(0..4).map(|n| UncheckedExtrinsic::new_signed(&sk, n, TestCall(n))).collect();
		let results = verify_batch(&exts);
		assert!(results.iter().all(|r| r.is_ok()));
	}

	#[test]
	fn verify_batch_catches_tampered_entry() {
		let sk = Alice.signing_key();
		let mut exts: Vec<_> =
			(0..3).map(|n| UncheckedExtrinsic::new_signed(&sk, n, TestCall(n))).collect();
		exts[1].nonce = 99; // tamper middle entry
		let results = verify_batch(&exts);
		assert!(results[0].is_ok());
		assert!(results[1].is_err());
		assert!(results[2].is_ok());
	}
}
