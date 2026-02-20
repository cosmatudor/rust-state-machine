use num::traits::{CheckedAdd, CheckedSub, Zero};
use parity_scale_codec::{Decode, Encode};
use std::collections::BTreeMap;

use crate::{
	support::{KeyValueStore, kv_store},
	system,
};

const PREFIX_BALANCE: &[u8] = b"balances:";

pub trait Config: system::Config {
	type Balance: Zero + CheckedSub + CheckedAdd + Copy + Encode + Decode;
}

#[derive(Debug)]
pub struct Pallet<T: Config> {
	balances: BTreeMap<T::AccountId, T::Balance>,
}

impl<T: Config> Pallet<T> {
	pub fn new() -> Self {
		let store = kv_store();
		let mut balances = BTreeMap::new();

		for (key, value) in store.scan_prefix(PREFIX_BALANCE) {
			if key.len() <= PREFIX_BALANCE.len() {
				continue;
			}
			let account_bytes = &key[PREFIX_BALANCE.len()..];
			if let (Ok(account), Ok(balance)) =
				(T::AccountId::decode(&mut &account_bytes[..]), T::Balance::decode(&mut &value[..]))
			{
				balances.insert(account, balance);
			}
		}

		Self { balances }
	}

	fn balance_key(who: &T::AccountId) -> Vec<u8> {
		let mut key = PREFIX_BALANCE.to_vec();
		key.extend(who.encode());
		key
	}

	pub fn set_balance(&mut self, who: &T::AccountId, amount: T::Balance) {
		self.balances.insert(who.clone(), amount);

		let key = Self::balance_key(who);
		let encoded = amount.encode();
		if let Err(e) = kv_store().put(&key, &encoded) {
			eprintln!("Failed to persist balance: {e}");
		}
	}

	pub fn balance(&self, who: &T::AccountId) -> T::Balance {
		*self.balances.get(who).unwrap_or(&T::Balance::zero())
	}
}

#[macros::call]
impl<T: Config> Pallet<T> {
	pub fn transfer(
		&mut self,
		caller: T::AccountId,
		to: T::AccountId,
		amount: T::Balance,
	) -> crate::support::DispatchResult {
		let caller_balance = self.balance(&caller);
		let to_balance = self.balance(&to);

		let new_caller_balance = caller_balance.checked_sub(&amount).ok_or("Not enough funds.")?;

		let new_to_balance = to_balance.checked_add(&amount).ok_or("Overflow")?;

		self.set_balance(&caller, new_caller_balance);
		self.set_balance(&to, new_to_balance);

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::system;

	struct TestConfig;
	impl system::Config for TestConfig {
		type AccountId = String;
		type BlockNumber = u32;
		type Nonce = u32;
	}
	impl Config for TestConfig {
		type Balance = u128;
	}

	fn new() -> Pallet<TestConfig> {
		Pallet::<TestConfig>::new()
	}

	#[test]
	fn unknown_account_balance_is_zero() {
		assert_eq!(new().balance(&"alice".to_string()), 0);
	}

	#[test]
	fn set_and_get_balance() {
		let mut p = new();
		p.set_balance(&"alice".to_string(), 100);
		assert_eq!(p.balance(&"alice".to_string()), 100);
		assert_eq!(p.balance(&"bob".to_string()), 0);
	}

	#[test]
	fn transfer_ok() {
		let mut p = new();
		p.set_balance(&"alice".to_string(), 100);
		assert_eq!(p.transfer("alice".to_string(), "bob".to_string(), 40), Ok(()));
		assert_eq!(p.balance(&"alice".to_string()), 60);
		assert_eq!(p.balance(&"bob".to_string()), 40);
	}

	#[test]
	fn transfer_exact_balance_empties_sender() {
		let mut p = new();
		p.set_balance(&"alice".to_string(), 100);
		assert_eq!(p.transfer("alice".to_string(), "bob".to_string(), 100), Ok(()));
		assert_eq!(p.balance(&"alice".to_string()), 0);
		assert_eq!(p.balance(&"bob".to_string()), 100);
	}

	#[test]
	fn transfer_insufficient_funds_leaves_balances_unchanged() {
		let mut p = new();
		p.set_balance(&"alice".to_string(), 50);
		assert_eq!(p.transfer("alice".to_string(), "bob".to_string(), 51), Err("Not enough funds."));
		assert_eq!(p.balance(&"alice".to_string()), 50);
		assert_eq!(p.balance(&"bob".to_string()), 0);
	}

	#[test]
	fn transfer_from_zero_balance_fails() {
		let mut p = new();
		assert_eq!(p.transfer("alice".to_string(), "bob".to_string(), 1), Err("Not enough funds."));
	}

	#[test]
	fn multiple_transfers_accumulate() {
		let mut p = new();
		p.set_balance(&"alice".to_string(), 100);
		p.transfer("alice".to_string(), "bob".to_string(), 30).unwrap();
		p.transfer("alice".to_string(), "bob".to_string(), 30).unwrap();
		assert_eq!(p.balance(&"alice".to_string()), 40);
		assert_eq!(p.balance(&"bob".to_string()), 60);
	}
}
