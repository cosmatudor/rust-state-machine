use core::fmt;
use once_cell::sync::Lazy;
use parity_scale_codec::{Decode, Encode};
use rocksdb::{IteratorMode, Options, DB};

/// A 32-byte account identifier displayed as abbreviated hex — mirrors `sp_core::crypto::AccountId32`.
///
/// `Debug` shows `0xXXXXXXXX…YYYYYYYY` (first 4 + last 4 bytes) so `BTreeMap` keys
/// are human-readable in debug output without printing 32 raw integers.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Encode, Decode)]
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
		// Show first 4 and last 4 bytes like Substrate's SS58 truncation.
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
	/// Ed25519 public key of the sender — this IS the `AccountId` in the runtime.
	pub signer: AccountId32,
	/// Ed25519 signature over SCALE(`signer.0 ‖ nonce ‖ call`).
	pub signature: [u8; 64],
	/// Sender's nonce at submission time. Prevents replay attacks.
	pub nonce: u32,
	pub call: Call,
}

impl<Call: Encode> UncheckedExtrinsic<Call> {
	/// Build and sign a new extrinsic.
	pub fn new_signed(sk: &ed25519_dalek::SigningKey, nonce: u32, call: Call) -> Self {
		use ed25519_dalek::Signer;
		let signer = AccountId32(*sk.verifying_key().as_bytes());
		let payload = (signer.as_bytes(), nonce, &call).encode();
		let signature = sk.sign(&payload).to_bytes();
		Self { signer, signature, nonce, call }
	}

	/// Verify the signature. Returns `Err` if the public key or signature is invalid.
	pub fn verify(&self) -> DispatchResult {
		use ed25519_dalek::Verifier;
		let vk = ed25519_dalek::VerifyingKey::from_bytes(self.signer.as_bytes())
			.map_err(|_| "invalid public key")?;
		let sig = ed25519_dalek::Signature::from_bytes(&self.signature);
		let payload = (self.signer.as_bytes(), self.nonce, &self.call).encode();
		vk.verify(&payload, &sig).map_err(|_| "invalid signature")
	}
}

/// `AccountId32` already has a compact hex `Debug`; show the signature truncated too.
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

/// Key–value store abstraction.
pub trait KeyValueStore {
	fn get(&self, key: &[u8]) -> Option<Vec<u8>>;
	fn put(&self, key: &[u8], value: &[u8]) -> Result<(), String>;
	fn delete(&self, key: &[u8]) -> Result<(), String>;
	fn scan_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)>;
}

static ROCKS_DB: Lazy<DB> = Lazy::new(|| {
	let mut opts = Options::default();
	opts.create_if_missing(true);
	DB::open(&opts, "state.db").expect("failed to open RocksDB at ./state.db")
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
			.filter_map(|(k, v)| {
				if k.starts_with(prefix) {
					Some((k.to_vec(), v.to_vec()))
				} else {
					None
				}
			})
			.collect()
	}
}

pub fn kv_store() -> RocksDbStore {
	RocksDbStore
}

/// Pending extrinsics waiting to be included in a block.
/// Clear boundary between "received" and "applied" transactions.
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
	/// New mempool with no capacity limit and no block limit.
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

	/// The configured per-block extrinsic limit, if any.
	pub fn block_limit(&self) -> Option<usize> {
		self.block_limit
	}

	/// Iterator over all pending extrinsics in order. Used by the persistence layer.
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
		if index < self.pending.len() {
			self.pending.remove(index)
		} else {
			None
		}
	}

	pub fn len(&self) -> usize {
		self.pending.len()
	}

	pub fn is_empty(&self) -> bool {
		self.pending.is_empty()
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

	/// This function takes a `caller` and the `call` they want to make, and returns a `Result`
	/// based on the outcome of that function call.
	fn dispatch(&mut self, caller: Self::Caller, call: Self::Call) -> DispatchResult;
}

/// Dev keyring — mirrors `sp_keyring::AccountKeyring` from the Substrate ecosystem.
///
/// Each variant derives a deterministic Ed25519 key from the UTF-8 encoding of the
/// variant name zero-padded to 32 bytes. Only for development / testing; never use
/// hardcoded seeds in production.
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

	/// Resolve a human-readable account name to an `AccountKeyring` variant.
	pub fn from_name(name: &str) -> Option<AccountKeyring> {
		match name.to_lowercase().as_str() {
			"alice" => Some(AccountKeyring::Alice),
			"bob" => Some(AccountKeyring::Bob),
			"charlie" => Some(AccountKeyring::Charlie),
			_ => None,
		}
	}
}
