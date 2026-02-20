use support::Dispatch;

pub mod balances;
pub mod proof_of_existence;
pub mod support;
pub mod system;

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

#[macros::runtime]
#[derive(Debug)]
pub struct Runtime {
	pub system: system::Pallet<Self>,
	pub balances: balances::Pallet<Self>,
	pub proof_of_existence: proof_of_existence::Pallet<Self>,
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

/// Seed dev accounts on a brand-new chain (block_number == 0) and execute the genesis block.
pub fn maybe_apply_genesis(runtime: &mut Runtime) {
	if runtime.system.block_number() != 0 {
		return;
	}
	use support::keyring::AccountKeyring::{Alice, Bob, Charlie};
	runtime.balances.set_balance(&Alice.public(), 1_000_000);
	runtime.balances.set_balance(&Bob.public(), 1_000_000);
	runtime.balances.set_balance(&Charlie.public(), 1_000_000);

	let genesis = types::Block { header: support::Header { block_number: 1 }, extrinsics: vec![] };
	runtime.execute_block(genesis).expect("genesis block must succeed");
	println!("[genesis] Alice / Bob / Charlie each funded with 1_000_000");
}
