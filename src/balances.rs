use num::traits::{CheckedAdd, CheckedSub, Zero};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Pallet<AccountId, Balance> {
	balances: BTreeMap<AccountId, Balance>,
}

impl<AccountId, Balance> Pallet<AccountId, Balance>
where
	AccountId: Ord + Clone,
	Balance: Zero + CheckedSub + CheckedAdd + Copy,
{
	pub fn new() -> Self {
		Self { balances: BTreeMap::new() }
	}

	pub fn set_balance(&mut self, who: &AccountId, amount: Balance) {
		self.balances.insert(who.clone(), amount);
	}

	pub fn balance(&self, who: &AccountId) -> Balance {
		*self.balances.get(who).unwrap_or(&Balance::zero())
	}

	pub fn transfer(
		&mut self,
		caller: AccountId,
		to: AccountId,
		amount: Balance,
	) -> Result<(), &'static str> {
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
mod test {

	use crate::{
		balances::Pallet,
		types::{AccountId, Balance},
	};
	#[test]
	fn init_balances() {
		use crate::types::{AccountId, Balance};

		let mut balances = Pallet::<AccountId, Balance>::new();

		assert_eq!(balances.balance(&"alice".to_string()), 0);
		balances.set_balance(&"alice".to_string(), 100);
		assert_eq!(balances.balance(&"alice".to_string()), 100);
		assert_eq!(balances.balance(&"bob".to_string()), 0);
	}

	#[test]
	fn transfer_balance() {
		let mut balances = Pallet::<AccountId, Balance>::new();
		balances.set_balance(&"alice".to_string(), 100);
		let mut result = balances.transfer("alice".to_string(), "bob".to_string(), 200);
		assert_eq!(result, Err("Not enough funds."));

		result = balances.transfer("alice".to_string(), "bob".to_string(), 40);
		assert_eq!(result, Ok(()));

		assert_eq!(balances.balance(&"alice".to_string()), 60);
		assert_eq!(balances.balance(&"bob".to_string()), 40);
	}
}
