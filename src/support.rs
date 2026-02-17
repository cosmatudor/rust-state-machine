use parity_scale_codec::{Decode, Encode};

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
