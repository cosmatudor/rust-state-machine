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

#[macros::runtime]
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

fn main() {
	let mut runtime = Runtime::new();
	runtime.balances.set_balance(&"alice".to_string(), 100);

	let block_1 = types::Block {
		header: support::Header { block_number: 1 },
		extrinsics: vec![
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::balances(crate::balances::Call::transfer{to:"bob".to_string(),amount: 70}),
			},
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::balances(crate::balances::Call::transfer{
					to: "charlie".to_string(),
					amount: 20,
			}),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::balances(crate::balances::Call::transfer{
					to: "charlie".to_string(),
					amount: 30,
			}),
			},
		],
	};

	let _res1 = runtime.execute_block(block_1).map_err(|e| eprintln!("{e}"));

	let block_2 = types::Block {
		header: support::Header { block_number: 2 },
		extrinsics: vec![support::Extrinsic {
			caller: "charlie".to_string(),
			call: RuntimeCall::balances(crate::balances::Call::transfer{to:"alice".to_string(), amount: 40}),
		}],
	};

	let _res2 = runtime.execute_block(block_2).map_err(|e| eprintln!("{e}"));

	let block_3 = types::Block {
		header: support::Header { block_number: 3 },
		extrinsics: vec![
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
					claim: "My first document".to_string(),
				}),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::balances(balances::Call::transfer {
					to: "alice".to_string(),
					amount: 5,
				}),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
					claim: "Patent for my invention".to_string(),
				}),
			},
			support::Extrinsic {
				caller: "charlie".to_string(),
				call: RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
					claim: "Copyright on my work".to_string(),
				}),
			},
		],
	};

	let _res3 = runtime.execute_block(block_3).map_err(|e| eprintln!("{e}"));

	let block_4 = types::Block {
		header: support::Header { block_number: 4 },
		extrinsics: vec![
			support::Extrinsic {
				caller: "charlie".to_string(),
				call: RuntimeCall::balances(balances::Call::transfer {
					to: "bob".to_string(),
					amount: 10,
				}),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
					claim: "My first document".to_string(),
				}),
			},
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
					claim: "My first document".to_string(),
				}),
			},
		],
	};

	let _res4 = runtime.execute_block(block_4).map_err(|e| eprintln!("{e}"));

	let block_5 = types::Block {
		header: support::Header { block_number: 5 },
		extrinsics: vec![
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::balances(balances::Call::transfer {
					to: "charlie".to_string(),
					amount: 3,
				}),
			},
			support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
					claim: "Non-existent claim".to_string(),
				}),
			},
			support::Extrinsic {
				caller: "charlie".to_string(),
				call: RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
					claim: "Patent for my invention".to_string(),
				}),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
					claim: "Patent for my invention".to_string(),
				}),
			},
			support::Extrinsic {
				caller: "bob".to_string(),
				call: RuntimeCall::balances(balances::Call::transfer {
					to: "alice".to_string(),
					amount: 15,
				}),
			},
		],
	};

	let _res5 = runtime.execute_block(block_5).map_err(|e| eprintln!("{e}"));

	println!("{runtime:#?}");
}
