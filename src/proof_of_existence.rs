use crate::support::DispatchResult;
use core::fmt::Debug;
use std::collections::BTreeMap;

pub trait Config: crate::system::Config {
	type Content: Debug + Ord;
}

pub enum Call<T: Config> {
	CreateClaim(T::Content),
	RevokeClaim(T::Content),
}

#[derive(Debug)]
pub struct Pallet<T: Config> {
	claims: BTreeMap<T::Content, T::AccountId>,
}

impl<T: Config> Pallet<T> {
	pub fn new() -> Self {
		Self { claims: BTreeMap::new() }
	}

	pub fn get_claim(&self, claim: &T::Content) -> Option<&T::AccountId> {
		self.claims.get(claim)
	}

	pub fn create_claim(&mut self, caller: T::AccountId, claim: T::Content) -> DispatchResult {
		if self.claims.contains_key(&claim) {
			return Err(&"this content is already claimed");
		}
		self.claims.insert(claim, caller);
		Ok(())
	}

	pub fn revoke_claim(&mut self, caller: T::AccountId, claim: T::Content) -> DispatchResult {
		let owner = self.claims.get(&claim).ok_or("claim does not exist")?;
		if *owner != caller {
			return Err(&"caller is not owner");
		}
		self.claims.remove(&claim);
		Ok(())
	}
}

impl<T: Config> crate::support::Dispatch for Pallet<T> {
	type Caller = T::AccountId;
	type Call = Call<T>;

	fn dispatch(
		&mut self,
		caller: Self::Caller,
		call: Self::Call,
	) -> crate::support::DispatchResult {
		match call {
			Call::CreateClaim(claim) => {
				self.create_claim(caller, claim)?;
			},
			Call::RevokeClaim(claim) => {
				self.revoke_claim(caller, claim)?;
			},
		}
		Ok(())
	}
}

#[cfg(test)]
mod test {
	struct TestConfig;

	impl super::Config for TestConfig {
		type Content = String;
	}

	impl crate::system::Config for TestConfig {
		type AccountId = String;
		type BlockNumber = u32;
		type Nonce = u32;
	}

	#[test]
	fn basic_proof_of_existence() {
		let mut poe = super::Pallet::<TestConfig>::new();
		assert_eq!(poe.get_claim(&"Hello, world!".to_string()), None);
		assert_eq!(poe.create_claim("alice".to_string(), "Hello, world!".to_string()), Ok(()));
		assert_eq!(poe.get_claim(&"Hello, world!".to_string()), Some(&"alice".to_string()));
		assert_eq!(
			poe.create_claim("bob".to_string(), "Hello, world!".to_string()),
			Err("this content is already claimed")
		);
		assert_eq!(poe.revoke_claim("alice".to_string(), "Hello, world!".to_string()), Ok(()));
		assert_eq!(poe.create_claim("bob".to_string(), "Hello, world!".to_string()), Ok(()));
	}
}
