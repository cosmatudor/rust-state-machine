use std::collections::BTreeMap;

use crate::support::{KeyValueStore, kv_store};
use num::traits::{CheckedAdd, CheckedSub, One, Zero};
use parity_scale_codec::{Decode, Encode};

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
			if let (Ok(account), Ok(nonce_value)) =
				(T::AccountId::decode(&mut &who_bytes[..]), T::Nonce::decode(&mut &value[..]))
			{
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
mod tests {
	use super::*;

	struct TestConfig;
	impl Config for TestConfig {
		type AccountId = String;
		type BlockNumber = u32;
		type Nonce = u32;
	}

	fn new() -> Pallet<TestConfig> {
		Pallet::<TestConfig>::new()
	}

	#[test]
	fn block_number_starts_at_zero() {
		assert_eq!(new().block_number(), 0);
	}

	#[test]
	fn inc_block_number_increments_by_one() {
		let mut s = new();
		s.inc_block_number();
		assert_eq!(s.block_number(), 1);
		s.inc_block_number();
		assert_eq!(s.block_number(), 2);
	}

	#[test]
	fn nonce_starts_at_zero_for_unknown_account() {
		assert_eq!(new().nonce(&"alice".to_string()), 0);
	}

	#[test]
	fn inc_nonce_increments_target_account() {
		let mut s = new();
		s.inc_nonce(&"alice".to_string());
		assert_eq!(s.nonce(&"alice".to_string()), 1);
	}

	#[test]
	fn inc_nonce_does_not_affect_other_accounts() {
		let mut s = new();
		s.inc_nonce(&"alice".to_string());
		assert_eq!(s.nonce(&"bob".to_string()), 0);
	}

	#[test]
	fn inc_nonce_multiple_times() {
		let mut s = new();
		s.inc_nonce(&"alice".to_string());
		s.inc_nonce(&"alice".to_string());
		s.inc_nonce(&"alice".to_string());
		assert_eq!(s.nonce(&"alice".to_string()), 3);
	}

	#[test]
	fn multiple_accounts_track_nonces_independently() {
		let mut s = new();
		s.inc_nonce(&"alice".to_string());
		s.inc_nonce(&"alice".to_string());
		s.inc_nonce(&"bob".to_string());
		assert_eq!(s.nonce(&"alice".to_string()), 2);
		assert_eq!(s.nonce(&"bob".to_string()), 1);
		assert_eq!(s.nonce(&"charlie".to_string()), 0);
	}
}
