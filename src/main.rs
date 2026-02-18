use crate::support::Dispatch;
use clap::{Parser, Subcommand};

mod balances;
mod proof_of_existence;
mod support;
mod system;

pub mod types {
	pub type AccountId = crate::support::AccountId32;
	pub type Balance = u128;
	pub type Nonce = u32;
	pub type BlockNumber = u32;
	pub type Extrinsic = crate::support::UncheckedExtrinsic<crate::RuntimeCall>;
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
	/// Submit a signed balance transfer into the next block.
	/// The sender must be one of the dev-keyring accounts: alice, bob, charlie.
	SubmitTransfer {
		from: String,
		to: String,
		amount: types::Balance,
	},
	/// Submit a signed proof-of-existence claim into the next block.
	/// The account must be one of the dev-keyring accounts: alice, bob, charlie.
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
	use support::keyring::AccountKeyring::{Alice, Bob, Charlie};

	let mut runtime = Runtime::new();

	let alice = Alice.public();
	let bob = Bob.public();
	let charlie = Charlie.public();

	let alice_sk = Alice.signing_key();
	let bob_sk = Bob.signing_key();
	let charlie_sk = Charlie.signing_key();

	runtime.balances.set_balance(&alice, 100);

	// Track per-account nonces for the demo (assumes a freshly-wiped state.db).
	let (mut an, mut bn, mut cn) = (0u32, 0u32, 0u32); // alice, bob, charlie nonces

	// --- Block 1 ---
	let block_1 = types::Block {
		header: support::Header { block_number: 1 },
		extrinsics: vec![
			{
				let call = RuntimeCall::balances(balances::Call::transfer { to: bob, amount: 70 });
				let ext = support::UncheckedExtrinsic::new_signed(&alice_sk, an, call);
				an += 1;
				ext
			},
			{
				let call =
					RuntimeCall::balances(balances::Call::transfer { to: charlie, amount: 20 });
				let ext = support::UncheckedExtrinsic::new_signed(&alice_sk, an, call);
				an += 1;
				ext
			},
			{
				let call =
					RuntimeCall::balances(balances::Call::transfer { to: charlie, amount: 30 });
				let ext = support::UncheckedExtrinsic::new_signed(&bob_sk, bn, call);
				bn += 1;
				ext
			},
		],
	};
	let _res1 = runtime.execute_block(block_1).map_err(|e| eprintln!("{e}"));

	// --- Block 2 ---
	let block_2 = types::Block {
		header: support::Header { block_number: 2 },
		extrinsics: vec![{
			let call = RuntimeCall::balances(balances::Call::transfer { to: alice, amount: 40 });
			let ext = support::UncheckedExtrinsic::new_signed(&charlie_sk, cn, call);
			cn += 1;
			ext
		}],
	};
	let _res2 = runtime.execute_block(block_2).map_err(|e| eprintln!("{e}"));

	// --- Block 3 ---
	let block_3 = types::Block {
		header: support::Header { block_number: 3 },
		extrinsics: vec![
			{
				let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
					claim: "My first document".to_string(),
				});
				let ext = support::UncheckedExtrinsic::new_signed(&alice_sk, an, call);
				an += 1;
				ext
			},
			{
				let call = RuntimeCall::balances(balances::Call::transfer { to: alice, amount: 5 });
				let ext = support::UncheckedExtrinsic::new_signed(&bob_sk, bn, call);
				bn += 1;
				ext
			},
			{
				let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
					claim: "Patent for my invention".to_string(),
				});
				let ext = support::UncheckedExtrinsic::new_signed(&bob_sk, bn, call);
				bn += 1;
				ext
			},
			{
				let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
					claim: "Copyright on my work".to_string(),
				});
				let ext = support::UncheckedExtrinsic::new_signed(&charlie_sk, cn, call);
				cn += 1;
				ext
			},
		],
	};
	let _res3 = runtime.execute_block(block_3).map_err(|e| eprintln!("{e}"));

	// --- Block 4 ---
	let block_4 = types::Block {
		header: support::Header { block_number: 4 },
		extrinsics: vec![
			{
				let call = RuntimeCall::balances(balances::Call::transfer { to: bob, amount: 10 });
				let ext = support::UncheckedExtrinsic::new_signed(&charlie_sk, cn, call);
				cn += 1;
				ext
			},
			{
				let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
					claim: "My first document".to_string(),
				});
				let ext = support::UncheckedExtrinsic::new_signed(&bob_sk, bn, call);
				bn += 1;
				ext
			},
			{
				let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
					claim: "My first document".to_string(),
				});
				let ext = support::UncheckedExtrinsic::new_signed(&alice_sk, an, call);
				an += 1;
				ext
			},
		],
	};
	let _res4 = runtime.execute_block(block_4).map_err(|e| eprintln!("{e}"));

	// --- Block 5 ---
	let block_5 = types::Block {
		header: support::Header { block_number: 5 },
		extrinsics: vec![
			{
				let call =
					RuntimeCall::balances(balances::Call::transfer { to: charlie, amount: 3 });
				let ext = support::UncheckedExtrinsic::new_signed(&alice_sk, an, call);
				an += 1;
				ext
			},
			{
				let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
					claim: "Non-existent claim".to_string(),
				});
				let ext = support::UncheckedExtrinsic::new_signed(&alice_sk, an, call);
				an += 1;
				ext
			},
			{
				let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
					claim: "Patent for my invention".to_string(),
				});
				support::UncheckedExtrinsic::new_signed(&charlie_sk, cn, call)
			},
			{
				let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
					claim: "Patent for my invention".to_string(),
				});
				let ext = support::UncheckedExtrinsic::new_signed(&bob_sk, bn, call);
				bn += 1;
				ext
			},
			{
				let call =
					RuntimeCall::balances(balances::Call::transfer { to: alice, amount: 15 });
				let ext = support::UncheckedExtrinsic::new_signed(&bob_sk, bn, call);
				bn += 1;
				ext
			},
		],
	};
	let _res5 = runtime.execute_block(block_5).map_err(|e| eprintln!("{e}"));

	// --- Mempool demo: receive → drain → execute ---
	let mut mempool = types::Mempool::new();
	let _ = mempool.submit({
		let call = RuntimeCall::balances(balances::Call::transfer { to: bob, amount: 1 });
		support::UncheckedExtrinsic::new_signed(&alice_sk, an, call)
	});
	let _ = mempool.submit({
		let call = RuntimeCall::balances(balances::Call::transfer { to: charlie, amount: 2 });
		support::UncheckedExtrinsic::new_signed(&bob_sk, bn, call)
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

// ---------------------------------------------------------------------------
// CLI commands
// ---------------------------------------------------------------------------

fn submit_transfer(from: String, to: String, amount: types::Balance) {
	let from_kr = support::keyring::from_name(&from)
		.unwrap_or_else(|| panic!("unknown account '{from}'; use alice / bob / charlie"));
	let to_kr = support::keyring::from_name(&to)
		.unwrap_or_else(|| panic!("unknown account '{to}'; use alice / bob / charlie"));

	let mut runtime = Runtime::new();
	let signer_pub = from_kr.public();

	runtime.balances.set_balance(&signer_pub, amount * 10);

	let nonce = runtime.system.nonce(&signer_pub);
	let call = RuntimeCall::balances(balances::Call::transfer { to: to_kr.public(), amount });
	let ext = support::UncheckedExtrinsic::new_signed(&from_kr.signing_key(), nonce, call);

	let next_block_number = runtime.system.block_number().checked_add(1u32).unwrap();
	let block = types::Block {
		header: support::Header { block_number: next_block_number },
		extrinsics: vec![ext],
	};

	match runtime.execute_block(block) {
		Ok(()) => println!("{runtime:#?}"),
		Err(e) => eprintln!("Execution error: {e}"),
	}
}

fn submit_claim(account: String, claim: String) {
	let kr = support::keyring::from_name(&account)
		.unwrap_or_else(|| panic!("unknown account '{account}'; use alice / bob / charlie"));

	let mut runtime = Runtime::new();
	let signer_pub = kr.public();

	let nonce = runtime.system.nonce(&signer_pub);
	let call =
		RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim { claim });
	let ext = support::UncheckedExtrinsic::new_signed(&kr.signing_key(), nonce, call);

	let next_block_number = runtime.system.block_number().checked_add(1u32).unwrap();
	let block = types::Block {
		header: support::Header { block_number: next_block_number },
		extrinsics: vec![ext],
	};

	match runtime.execute_block(block) {
		Ok(()) => println!("{runtime:#?}"),
		Err(e) => eprintln!("Execution error: {e}"),
	}
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use parity_scale_codec::{Decode, Encode};
	use support::keyring::AccountKeyring::{Alice, Bob};

	/// Build a signed transfer extrinsic using the dev keyring.
	fn signed_transfer(
		from: support::keyring::AccountKeyring,
		nonce: u32,
		to: support::keyring::AccountKeyring,
		amount: types::Balance,
	) -> types::Extrinsic {
		let call =
			RuntimeCall::balances(balances::Call::transfer { to: to.public(), amount });
		support::UncheckedExtrinsic::new_signed(&from.signing_key(), nonce, call)
	}

	#[test]
	fn block_scale_roundtrip() {
		let ext = signed_transfer(Alice, 0, Bob, 10);
		let block =
			types::Block { header: support::Header { block_number: 1 }, extrinsics: vec![ext] };
		let encoded = block.encode();
		let decoded = types::Block::decode(&mut &encoded[..]).expect("decode succeeds");
		assert_eq!(decoded.header.block_number, block.header.block_number);
		assert_eq!(decoded.extrinsics.len(), 1);
		assert_eq!(decoded.extrinsics[0].signer, Alice.public());
	}

	#[test]
	fn signature_verification_accepts_valid() {
		let ext = signed_transfer(Alice, 0, Bob, 5);
		assert!(ext.verify().is_ok());
	}

	#[test]
	fn signature_verification_rejects_tampered_call() {
		let mut ext = signed_transfer(Alice, 0, Bob, 5);
		// Flip a bit in the call encoding by changing the nonce after signing.
		ext.nonce = 99;
		assert!(ext.verify().is_err());
	}

	#[test]
	fn mempool_submit_and_drain() {
		let mut pool = types::Mempool::new();
		let ext = signed_transfer(Alice, 0, Bob, 5);
		assert!(pool.submit(ext).is_ok());
		assert_eq!(pool.len(), 1);
		let batch = pool.drain_for_block(10);
		assert_eq!(batch.len(), 1);
		assert!(pool.is_empty());
	}

	#[test]
	fn mempool_respects_capacity() {
		let mut pool = types::Mempool::with_capacity(1);
		assert!(pool.submit(signed_transfer(Alice, 0, Bob, 1)).is_ok());
		assert!(pool.submit(signed_transfer(Bob, 0, Alice, 1)).is_err()); // MempoolFull
	}

	#[test]
	fn mempool_block_limit_signal() {
		let mut pool = types::Mempool::with_block_limit(2);
		pool.submit(signed_transfer(Alice, 0, Bob, 1)).unwrap();
		assert!(!pool.is_block_ready());
		pool.submit(signed_transfer(Bob, 0, Alice, 1)).unwrap();
		assert!(pool.is_block_ready());
		let batch = pool.drain_for_block(pool.block_limit().unwrap());
		assert_eq!(batch.len(), 2);
		assert!(!pool.is_block_ready());
	}
}
