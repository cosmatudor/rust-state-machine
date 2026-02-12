use crate::support::Dispatch;

mod balances;
mod proof_of_existence;
mod support;
mod system;

pub mod types {
	pub type AccountId = String;
	pub type Balance = u128;
	pub type Nonce = u32;
	pub type BlockNumber = u32;
	pub type Extrinsic = crate::support::Extrinsic<AccountId, crate::RuntimeCall>;
	pub type Header = crate::support::Header<BlockNumber>;
	pub type Block = crate::support::Block<Header, Extrinsic>;
	pub type Content = String;
}

pub enum RuntimeCall {
	Balances(balances::Call<Runtime>),
	PoE(proof_of_existence::Call<Runtime>),
}

#[derive(Debug)]
pub struct Runtime {
	system: system::Pallet<Self>,
	balances: balances::Pallet<Self>,
	proof_of_existence: proof_of_existence::Pallet<Self>,
}

impl system::Config for Runtime {
	type AccountId = types::AccountId;
	type BlockNumber = types::BlockNumber;
	type Nonce = types::Nonce;
}

impl balances::Config for Runtime {
	type Balance = types::Balance;
}

impl proof_of_existence::Config for Runtime {
	type Content = types::Content;
}

impl Runtime {
	fn new() -> Self {
		Self {
			system: system::Pallet::new(),
			balances: balances::Pallet::new(),
			proof_of_existence: proof_of_existence::Pallet::new(),
		}
	}

	// Execute a block of extrinsics. Increments the block number.
	fn execute_block(&mut self, block: types::Block) -> support::DispatchResult {
		self.system.inc_block_number();
		if self.system.block_number() != block.header.block_number {
			return Err(&"block number does not match what is expected")
		}

		for (i, support::Extrinsic { caller, call }) in block.extrinsics.into_iter().enumerate() {
			self.system.inc_nonce(&caller);
			let _res = self.dispatch(caller.clone(), call).map_err(|e| {
				eprintln!(
					"Extrinsic Error\n\tBlock Number: {}\n\tExtrinsic Number: {}\n\tError: {}",
					block.header.block_number, i, e
				)
			});
		}

		Ok(())
	}
}

impl crate::support::Dispatch for Runtime {
	type Caller = <Runtime as system::Config>::AccountId;
	type Call = RuntimeCall;

	// Dispatch a call on behalf of a caller.
	fn dispatch(
		&mut self,
		caller: Self::Caller,
		runtime_call: Self::Call,
	) -> support::DispatchResult {
		match runtime_call {
			RuntimeCall::Balances(call) => {
				self.balances.dispatch(caller, call)?;
			},
			RuntimeCall::PoE(call) => {
				self.proof_of_existence.dispatch(caller, call)?;
			},
		}
		Ok(())
	}
}

fn main() {
	let mut runtime = Runtime::new();
	runtime.balances.set_balance(&"alice".to_string(), 100);

	let block_1 = types::Block {
		header: support::Header { block_number: 1 },
		extrinsics: vec![
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::Balances(crate::balances::Call::Transfer("bob".to_string(), 70)),
			},
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::Balances(crate::balances::Call::Transfer(
					"charlie".to_string(),
					20,
				)),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::Balances(crate::balances::Call::Transfer(
					"charlie".to_string(),
					30,
				)),
			},
		],
	};

	let _res1 = runtime.execute_block(block_1).map_err(|e| eprintln!("{e}"));

	let block_2 = types::Block {
		header: support::Header { block_number: 2 },
		extrinsics: vec![support::Extrinsic {
			caller: "charlie".to_string(),
			call: RuntimeCall::Balances(crate::balances::Call::Transfer("alice".to_string(), 40)),
		}],
	};

	let _res2 = runtime.execute_block(block_2).map_err(|e| eprintln!("{e}"));

	let block_3 = types::Block {
		header: support::Header { block_number: 3 },
		extrinsics: vec![
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::PoE(proof_of_existence::Call::CreateClaim(
					"My first document".to_string(),
				)),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::Balances(balances::Call::Transfer("alice".to_string(), 5)),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::PoE(proof_of_existence::Call::CreateClaim(
					"Patent for my invention".to_string(),
				)),
			},
			support::Extrinsic {
				caller: "charlie".to_string(),
				call: RuntimeCall::PoE(proof_of_existence::Call::CreateClaim(
					"Copyright on my work".to_string(),
				)),
			},
		],
	};

	let _res3 = runtime.execute_block(block_3).map_err(|e| eprintln!("{e}"));

	let block_4 = types::Block {
		header: support::Header { block_number: 4 },
		extrinsics: vec![
			support::Extrinsic {
				caller: "charlie".to_string(),
				call: RuntimeCall::Balances(balances::Call::Transfer("bob".to_string(), 10)),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::PoE(proof_of_existence::Call::CreateClaim(
					"My first document".to_string(),
				)),
			},
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::PoE(proof_of_existence::Call::RevokeClaim(
					"My first document".to_string(),
				)),
			},
		],
	};

	let _res4 = runtime.execute_block(block_4).map_err(|e| eprintln!("{e}"));

	let block_5 = types::Block {
		header: support::Header { block_number: 5 },
		extrinsics: vec![
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::Balances(balances::Call::Transfer("charlie".to_string(), 3)),
			},
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::PoE(proof_of_existence::Call::RevokeClaim(
					"Non-existent claim".to_string(),
				)),
			},
			support::Extrinsic {
				caller: "charlie".to_string(),
				call: RuntimeCall::PoE(proof_of_existence::Call::RevokeClaim(
					"Patent for my invention".to_string(),
				)),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::PoE(proof_of_existence::Call::RevokeClaim(
					"Patent for my invention".to_string(),
				)),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::Balances(balances::Call::Transfer("alice".to_string(), 15)),
			},
		],
	};

	let _res5 = runtime.execute_block(block_5).map_err(|e| eprintln!("{e}"));

	println!("{runtime:#?}");
}
