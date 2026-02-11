use std::collections::BTreeMap;

use num::traits::{CheckedAdd, CheckedSub, One, Zero};

pub trait Config {
	type AccountId: Ord + Clone;
	type Nonce: Zero + CheckedAdd + Copy + One;
	type BlockNumber: Zero + CheckedSub + CheckedAdd + Copy + One;
}

#[derive(Debug)]
pub struct Pallet<T: Config> {
	block_number: T::BlockNumber,
	nonce: BTreeMap<T::AccountId, T::Nonce>,
}

impl<T: Config> Pallet<T> {
	pub fn new() -> Self {
		Self { block_number: T::BlockNumber::zero(), nonce: BTreeMap::new() }
	}

	pub fn block_number(&self) -> T::BlockNumber {
		self.block_number
	}

	pub fn inc_block_number(&mut self) {
		self.block_number = self.block_number.checked_add(&T::BlockNumber::one()).unwrap();
	}

	pub fn inc_nonce(&mut self, who: &T::AccountId) {
		let user_nonce = *self.nonce.get(who).unwrap_or(&T::Nonce::zero());
		let new_nonce = user_nonce.checked_add(&T::Nonce::one()).unwrap();
		self.nonce.insert(who.clone(), new_nonce);
	}
}

#[cfg(test)]
mod test {
	use crate::system::Pallet;

	struct TestConfig;
	impl super::Config for TestConfig {
		type AccountId = String;
		type BlockNumber = u32;
		type Nonce = u32;
	}

	#[test]
	fn init_system() {
		let mut system = Pallet::<TestConfig>::new();
		system.inc_block_number();
		system.inc_nonce(&"alice".to_string());

		assert_eq!(system.block_number(), 1);
		assert_eq!(system.nonce.get("alice"), Some(&1));
		assert_eq!(system.nonce.get("bob"), None);
	}
}
