pub struct Block<Header, Extrinsic> {
	pub header: Header,
	pub extrinsics: Vec<Extrinsic>,
}

pub struct Header<BlockNumber> {
	pub block_number: BlockNumber,
}

pub struct Extrinsic<Caller, Call> {
	pub caller: Caller,
	pub call: Call,
}
pub type DispatchResult = Result<(), &'static str>;

pub trait Dispatch {
	type Caller;
	type Call;

	/// This function takes a `caller` and the `call` they want to make, and returns a `Result`
	/// based on the outcome of that function call.
	fn dispatch(&mut self, caller: Self::Caller, call: Self::Call) -> DispatchResult;
}
