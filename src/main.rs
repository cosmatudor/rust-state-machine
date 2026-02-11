mod balances;
mod system;

pub mod types {
	pub type AccountId = String;
	pub type Balance = u128;
	pub type Nonce = u32;
	pub type BlockNumber = u32;
}

#[derive(Debug)]
pub struct Runtime {
	system: system::Pallet<Self>,
	balances: balances::Pallet<types::AccountId, types::Balance>,
}

impl Runtime {
	fn new() -> Self {
		Self { system: system::Pallet::new(), balances: balances::Pallet::new() }
	}
}

impl system::Config for Runtime {
	type AccountId = types::AccountId;
	type BlockNumber = types::BlockNumber;
	type Nonce = types::Nonce;
}

fn main() {
	let mut runtime = Runtime::new();
	runtime.balances.set_balance(&"alice".to_string(), 100);

	runtime.system.inc_block_number();
	assert_eq!(runtime.system.block_number(), 1);

	runtime.system.inc_nonce(&"alice".to_string());
	let _res_tx_1 = runtime
		.balances
		.transfer("alice".to_string(), "bob".to_string(), 30)
		.map_err(|e| eprintln!("{e}"));

	runtime.system.inc_nonce(&"alice".to_string());
	let _res_tx_1 = runtime
		.balances
		.transfer("alice".to_string(), "charlie".to_string(), 20)
		.map_err(|e| eprintln!("{e}"));

	println!("{runtime:#?}");
}
