use crate::support::Dispatch;
use clap::{Parser, Subcommand};

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
	pub type Mempool = crate::support::Mempool<Extrinsic>;
}

#[derive(Parser)]
#[command(name = "rust-state-machine", version, about = "State machine")]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	/// Run the hardcoded demo scenario (blocks 1–5 + mempool block).
	Demo,
	/// Submit a single balance transfer as a one-extrinsic block.
	SubmitTransfer {
		from: String,
		to: String,
		amount: types::Balance,
	},
	/// Submit a single proof-of-existence claim as a one-extrinsic block.
	SubmitClaim {
		account: String,
		claim: String,
	},
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
	let cli = Cli::parse();

	match cli.command {
		Commands::Demo => run_demo(),
		Commands::SubmitTransfer { from, to, amount } => submit_transfer(from, to, amount),
		Commands::SubmitClaim { account, claim } => submit_claim(account, claim),
	}
}

fn run_demo() {
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

	// --- Mempool demo: receive → mempool → drain for block → execute ---
	let mut mempool = types::Mempool::new();
	let _ = mempool.submit(support::Extrinsic {
		caller: "alice".to_string(),
		call: RuntimeCall::balances(balances::Call::transfer {
			to: "bob".to_string(),
			amount: 1,
		}),
	});
	let _ = mempool.submit(support::Extrinsic {
		caller: "bob".to_string(),
		call: RuntimeCall::balances(balances::Call::transfer {
			to: "charlie".to_string(),
			amount: 2,
		}),
	});
	let batch = mempool.drain_for_block(2);
	let block_from_mempool = types::Block {
		header: support::Header {
			block_number: runtime.system.block_number().checked_add(1u32).unwrap(),
		},
		extrinsics: batch,
	};
	let _res_mempool =
		runtime.execute_block(block_from_mempool).map_err(|e| eprintln!("Mempool block: {e}"));

	println!("{runtime:#?}");
}

fn submit_transfer(from: String, to: String, amount: types::Balance) {
	let mut runtime = Runtime::new();
	// For now, give the sender some funds so the transfer can succeed.
	runtime.balances.set_balance(&from, amount * 10);

	let block = types::Block {
		header: support::Header { block_number: 1 },
		extrinsics: vec![support::Extrinsic {
			caller: from.clone(),
			call: RuntimeCall::balances(crate::balances::Call::transfer { to, amount }),
		}],
	};

	let res = runtime.execute_block(block);
	match res {
		Ok(()) => println!("{runtime:#?}"),
		Err(e) => eprintln!("Execution error: {e}"),
	}
}

fn submit_claim(account: String, claim: String) {
	let mut runtime = Runtime::new();

	let block = types::Block {
		header: support::Header { block_number: 1 },
		extrinsics: vec![support::Extrinsic {
			caller: account.clone(),
			call: RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim { claim }),
		}],
	};

	let res = runtime.execute_block(block);
	match res {
		Ok(()) => println!("{runtime:#?}"),
		Err(e) => eprintln!("Execution error: {e}"),
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	use parity_scale_codec::{Decode, Encode};

	#[test]
	fn block_scale_roundtrip() {
		let block = types::Block {
			header: support::Header { block_number: 1 },
			extrinsics: vec![support::Extrinsic {
				caller: "alice".to_string(),
				call: RuntimeCall::balances(crate::balances::Call::transfer {
					to: "bob".to_string(),
					amount: 10,
				}),
			}],
		};
		let encoded = block.encode();
		let decoded = types::Block::decode(&mut &encoded[..]).expect("decode succeeds");
		assert_eq!(decoded.header.block_number, block.header.block_number);
		assert_eq!(decoded.extrinsics.len(), 1);
		assert_eq!(decoded.extrinsics[0].caller, "alice");
	}

	#[test]
	fn mempool_submit_and_drain() {
		let mut pool = types::Mempool::new();
		let ext = support::Extrinsic {
			caller: "alice".to_string(),
			call: RuntimeCall::balances(crate::balances::Call::transfer {
				to: "bob".to_string(),
				amount: 5,
			}),
		};
		assert!(pool.submit(ext).is_ok());
		assert_eq!(pool.len(), 1);
		let batch = pool.drain_for_block(10);
		assert_eq!(batch.len(), 1);
		assert!(pool.is_empty());
	}

	#[test]
	fn mempool_respects_capacity() {
		let mut pool = types::Mempool::with_capacity(1);
		let ext = support::Extrinsic {
			caller: "alice".to_string(),
			call: RuntimeCall::balances(crate::balances::Call::transfer {
				to: "bob".to_string(),
				amount: 1,
			}),
		};
		assert!(pool.submit(ext).is_ok());
		let ext2 = support::Extrinsic {
			caller: "bob".to_string(),
			call: RuntimeCall::balances(crate::balances::Call::transfer {
				to: "alice".to_string(),
				amount: 1,
			}),
		};
		assert!(pool.submit(ext2).is_err()); // MempoolFull
	}
}

