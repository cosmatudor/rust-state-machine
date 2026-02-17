use crate::support::{DispatchResult, kv_store, KeyValueStore};
use core::fmt::Debug;
use parity_scale_codec::{Decode, Encode};
use std::collections::BTreeMap;

const PREFIX_POE: &[u8] = b"poe:";

pub trait Config: crate::system::Config {
	type Content: Debug + Ord + Encode + Decode;
}

#[derive(Debug)]
pub struct Pallet<T: Config> {
	claims: BTreeMap<T::Content, T::AccountId>,
}

impl<T: Config> Pallet<T> {
	pub fn new() -> Self {
		let store = kv_store();
		let mut claims = BTreeMap::new();

		for (key, value) in store.scan_prefix(PREFIX_POE) {
			if key.len() <= PREFIX_POE.len() {
				continue;
			}
			let content_bytes = &key[PREFIX_POE.len()..];
			if let (Ok(content), Ok(owner)) = (
				T::Content::decode(&mut &content_bytes[..]),
				T::AccountId::decode(&mut &value[..]),
			) {
				claims.insert(content, owner);
			}
		}

		Self { claims }
	}

	fn claim_key(claim: &T::Content) -> Vec<u8> {
		let mut key = PREFIX_POE.to_vec();
		key.extend(claim.encode());
		key
	}

	#[allow(dead_code)]
	pub fn get_claim(&self, claim: &T::Content) -> Option<&T::AccountId> {
		self.claims.get(claim)
	}
}

#[macros::call]
impl<T: Config> Pallet<T> {
    pub fn create_claim(&mut self, caller: T::AccountId, claim: T::Content) -> DispatchResult {
		if self.claims.contains_key(&claim) {
			return Err(&"this content is already claimed");
		}
		self.claims.insert(claim, caller);

		let last_claim = self.claims.keys().last().expect("inserted; map not empty");
		let owner = self.claims.get(last_claim).expect("owner exists");
		let key = Self::claim_key(last_claim);
		let encoded_owner = owner.encode();
		if let Err(e) = kv_store().put(&key, &encoded_owner) {
			eprintln!("Failed to persist PoE claim: {e}");
		}
		Ok(())
	}

	pub fn revoke_claim(&mut self, caller: T::AccountId, claim: T::Content) -> DispatchResult {
		let owner = self.claims.get(&claim).ok_or("claim does not exist")?;
		if *owner != caller {
			return Err(&"caller is not owner");
		}
		self.claims.remove(&claim);

		let key = Self::claim_key(&claim);
		if let Err(e) = kv_store().delete(&key) {
			eprintln!("Failed to delete PoE claim from storage: {e}");
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
