use once_cell::sync::Lazy;
use parity_scale_codec::{Decode, Encode};
use rocksdb::{IteratorMode, Options, DB};

#[derive(Clone, Encode, Decode)]
pub struct Block<Header, Extrinsic> {
	pub header: Header,
	pub extrinsics: Vec<Extrinsic>,
}

#[derive(Clone, Encode, Decode)]
pub struct Header<BlockNumber> {
	pub block_number: BlockNumber,
}

#[derive(Clone, Encode, Decode)]
pub struct Extrinsic<Caller, Call> {
	pub caller: Caller,
	pub call: Call,
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

/// Helper to access the global key–value store.
pub fn kv_store() -> RocksDbStore {
	RocksDbStore
}

/// Pending extrinsics waiting to be included in a block.
/// Clear boundary between "received" and "applied" transactions.
#[derive(Debug, Default)]
pub struct Mempool<Extrinsic> {
	pending: std::collections::VecDeque<Extrinsic>,
	max_capacity: Option<usize>,
}

#[derive(Debug)]
pub struct MempoolFull;

impl<Extrinsic> Mempool<Extrinsic> {
	/// New mempool with no capacity limit.
	pub fn new() -> Self {
		Self { pending: std::collections::VecDeque::new(), max_capacity: None }
	}

	/// New mempool that rejects new extrinsics when `max_len` is reached.
	pub fn with_capacity(max_len: usize) -> Self {
		Self {
			pending: std::collections::VecDeque::new(),
			max_capacity: Some(max_len),
		}
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
