use clap::{Parser, Subcommand};

mod network;
mod node;

// Re-import from the library so child modules (node.rs) can reach them via `crate::*`.
use rust_state_machine::{
	Runtime, RuntimeCall, balances, maybe_apply_genesis, proof_of_existence, support, types,
};

#[derive(Parser)]
#[command(name = "rust-state-machine", version, about = "State machine")]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	/// Start the P2P node, listening on the given TCP port.
	Start {
		/// Local TCP port for libp2p (e.g. 4001).
		#[arg(long, default_value_t = 4001)]
		port: u16,
		/// Optional peer multiaddr to dial on startup
		/// (e.g. /ip4/127.0.0.1/tcp/4002).
		#[arg(long)]
		peer: Option<String>,
		/// If given, expose an HTTP RPC server on this port.
		#[arg(long)]
		rpc_port: Option<u16>,
		/// Path to the RocksDB database directory (default: ./state.db).
		#[arg(long)]
		db_path: Option<String>,
	},
	/// Print the current chain state (balances, nonces, PoE claims) and exit.
	State {
		/// Path to the RocksDB database directory (default: ./state.db).
		#[arg(long)]
		db_path: Option<String>,
	},
	/// Delete the database and reset the chain to a clean state.
	Reset {
		/// Path to the RocksDB database directory (default: ./state.db).
		#[arg(long)]
		db_path: Option<String>,
	},
	/// Submit a signed balance transfer into the next block.
	/// The sender must be one of the dev-keyring accounts: alice, bob, charlie.
	SubmitTransfer {
		from: String,
		to: String,
		amount: types::Balance,
		/// HTTP RPC URL of a running node (e.g. http://127.0.0.1:8000).
		/// If given, the extrinsic is submitted to the running node.
		/// If omitted, the transfer is executed locally in a one-shot runtime.
		#[arg(long)]
		node: Option<String>,
	},
	/// Submit a signed proof-of-existence claim into the next block.
	/// The account must be one of the dev-keyring accounts: alice, bob, charlie.
	SubmitClaim {
		account: String,
		claim: String,
		/// HTTP RPC URL of a running node (e.g. http://127.0.0.1:8000).
		#[arg(long)]
		node: Option<String>,
	},
}

fn main() {
	let cli = Cli::parse();

	match cli.command {
		Commands::Start { port, peer, rpc_port, db_path } => {
			if let Some(path) = db_path {
				support::init_db_path(&path);
			}
			let dial_addr =
				peer.map(|s| s.parse::<libp2p::Multiaddr>().expect("invalid multiaddr"));
			tokio::runtime::Builder::new_multi_thread()
				.enable_all()
				.build()
				.unwrap()
				.block_on(node::run(port, dial_addr, rpc_port))
				.unwrap();
		},
		Commands::State { db_path } => {
			if let Some(path) = db_path {
				support::init_db_path(&path);
			}
			let runtime = Runtime::new();
			println!("{runtime:#?}");
		},
		Commands::Reset { db_path } => {
			let path = db_path.as_deref().unwrap_or("state.db");
			if std::path::Path::new(path).exists() {
				std::fs::remove_dir_all(path)
					.unwrap_or_else(|e| eprintln!("failed to delete '{path}': {e}"));
				println!("Reset: deleted '{path}'");
			} else {
				println!("Nothing to reset — '{path}' does not exist");
			}
		},
		Commands::SubmitTransfer { from, to, amount, node } =>
			submit_transfer(from, to, amount, node),
		Commands::SubmitClaim { account, claim, node } => submit_claim(account, claim, node),
	}
}

#[allow(dead_code)]
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
				let call =
					RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
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
				let call =
					RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
						claim: "Patent for my invention".to_string(),
					});
				let ext = support::UncheckedExtrinsic::new_signed(&bob_sk, bn, call);
				bn += 1;
				ext
			},
			{
				let call =
					RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
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
				let call =
					RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim {
						claim: "My first document".to_string(),
					});
				let ext = support::UncheckedExtrinsic::new_signed(&bob_sk, bn, call);
				bn += 1;
				ext
			},
			{
				let call =
					RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
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
				let call =
					RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
						claim: "Non-existent claim".to_string(),
					});
				let ext = support::UncheckedExtrinsic::new_signed(&alice_sk, an, call);
				an += 1;
				ext
			},
			{
				let call =
					RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
						claim: "Patent for my invention".to_string(),
					});
				support::UncheckedExtrinsic::new_signed(&charlie_sk, cn, call)
			},
			{
				let call =
					RuntimeCall::proof_of_existence(proof_of_existence::Call::revoke_claim {
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

	// --- Mempool demo ---
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
	let _res_mempool = runtime
		.execute_block(block_from_mempool)
		.map_err(|e| eprintln!("Mempool block: {e}"));

	println!("{runtime:#?}");
}

fn submit_transfer(from: String, to: String, amount: types::Balance, node: Option<String>) {
	use parity_scale_codec::Encode;

	let from_kr = support::keyring::from_name(&from)
		.unwrap_or_else(|| panic!("unknown account '{from}'; use alice / bob / charlie"));
	let to_kr = support::keyring::from_name(&to)
		.unwrap_or_else(|| panic!("unknown account '{to}'; use alice / bob / charlie"));

	let call = RuntimeCall::balances(balances::Call::transfer { to: to_kr.public(), amount });

	if let Some(url) = node {
		let signer_pub = from_kr.public();
		let account_hex: String =
			signer_pub.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
		let nonce: u32 = ureq::get(&format!("{url}/nonce/{account_hex}"))
			.call()
			.unwrap_or_else(|e| panic!("failed to get nonce: {e}"))
			.into_string()
			.unwrap()
			.trim()
			.parse()
			.expect("nonce must be a number");
		let ext = support::UncheckedExtrinsic::new_signed(&from_kr.signing_key(), nonce, call);
		let bytes = ext.encode();
		match ureq::post(&format!("{url}/submit"))
			.set("Content-Type", "application/octet-stream")
			.send_bytes(&bytes)
		{
			Ok(res) => println!("Submitted (HTTP {})", res.status()),
			Err(ureq::Error::Status(code, res)) => {
				eprintln!("Server error {code}: {}", res.into_string().unwrap_or_default())
			},
			Err(e) => eprintln!("Connection error: {e}"),
		}
	} else {
		let mut runtime = Runtime::new();
		let signer_pub = from_kr.public();
		runtime.balances.set_balance(&signer_pub, amount * 10);
		let nonce = runtime.system.nonce(&signer_pub);
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
}

fn submit_claim(account: String, claim: String, node: Option<String>) {
	use parity_scale_codec::Encode;

	let kr = support::keyring::from_name(&account)
		.unwrap_or_else(|| panic!("unknown account '{account}'; use alice / bob / charlie"));

	let call = RuntimeCall::proof_of_existence(proof_of_existence::Call::create_claim { claim });

	if let Some(url) = node {
		let signer_pub = kr.public();
		let account_hex: String =
			signer_pub.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
		let nonce: u32 = ureq::get(&format!("{url}/nonce/{account_hex}"))
			.call()
			.unwrap_or_else(|e| panic!("failed to get nonce: {e}"))
			.into_string()
			.unwrap()
			.trim()
			.parse()
			.expect("nonce must be a number");
		let ext = support::UncheckedExtrinsic::new_signed(&kr.signing_key(), nonce, call);
		let bytes = ext.encode();
		match ureq::post(&format!("{url}/submit"))
			.set("Content-Type", "application/octet-stream")
			.send_bytes(&bytes)
		{
			Ok(res) => println!("Submitted (HTTP {})", res.status()),
			Err(ureq::Error::Status(code, res)) => {
				eprintln!("Server error {code}: {}", res.into_string().unwrap_or_default())
			},
			Err(e) => eprintln!("Connection error: {e}"),
		}
	} else {
		let mut runtime = Runtime::new();
		let signer_pub = kr.public();
		let nonce = runtime.system.nonce(&signer_pub);
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
}
