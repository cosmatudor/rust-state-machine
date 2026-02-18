use std::collections::BTreeMap;

use num::traits::{CheckedAdd, CheckedSub, One, Zero};
use parity_scale_codec::{Decode, Encode};
use crate::support::{kv_store, KeyValueStore};

const PREFIX_BLOCK_NUMBER: &[u8] = b"system:block_number";
const PREFIX_NONCE: &[u8] = b"system:nonce:";

pub trait Config {
	type AccountId: Ord + Clone + Encode + Decode;
	type Nonce: Zero + CheckedAdd + Copy + One + Encode + Decode;
	type BlockNumber: Zero + CheckedSub + CheckedAdd + Copy + One + Encode + Decode;
}

#[derive(Debug)]
pub struct Pallet<T: Config> {
	block_number: T::BlockNumber,
	nonce: BTreeMap<T::AccountId, T::Nonce>,
}

impl<T: Config> Pallet<T> {
	pub fn new() -> Self {
		let store = kv_store();

		let block_number = store
			.get(PREFIX_BLOCK_NUMBER)
			.and_then(|bytes| T::BlockNumber::decode(&mut &bytes[..]).ok())
			.unwrap_or_else(T::BlockNumber::zero);

		let mut nonce = BTreeMap::new();
		for (key, value) in store.scan_prefix(PREFIX_NONCE) {
			if key.len() <= PREFIX_NONCE.len() {
				continue;
			}
			let who_bytes = &key[PREFIX_NONCE.len()..];
			if let (Ok(account), Ok(nonce_value)) = (
				T::AccountId::decode(&mut &who_bytes[..]),
				T::Nonce::decode(&mut &value[..]),
			) {
				nonce.insert(account, nonce_value);
			}
		}

		Self { block_number, nonce }
	}

	pub fn block_number(&self) -> T::BlockNumber {
		self.block_number
	}

	pub fn nonce(&self, who: &T::AccountId) -> T::Nonce {
		*self.nonce.get(who).unwrap_or(&T::Nonce::zero())
	}

	pub fn inc_block_number(&mut self) {
		self.block_number = self.block_number.checked_add(&T::BlockNumber::one()).unwrap();
		let encoded = self.block_number.encode();
		if let Err(e) = kv_store().put(PREFIX_BLOCK_NUMBER, &encoded) {
			eprintln!("Failed to persist block number: {e}");
		}
	}

	pub fn inc_nonce(&mut self, who: &T::AccountId) {
		let user_nonce = *self.nonce.get(who).unwrap_or(&T::Nonce::zero());
		let new_nonce = user_nonce.checked_add(&T::Nonce::one()).unwrap();
		self.nonce.insert(who.clone(), new_nonce);

		let mut key = PREFIX_NONCE.to_vec();
		key.extend(who.encode());
		let encoded = new_nonce.encode();
		if let Err(e) = kv_store().put(&key, &encoded) {
			eprintln!("Failed to persist nonce for account: {e}");
		}
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
