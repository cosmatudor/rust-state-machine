use crate::support::{DispatchResult, KeyValueStore, kv_store};
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
			if let (Ok(content), Ok(owner)) =
				(T::Content::decode(&mut &content_bytes[..]), T::AccountId::decode(&mut &value[..]))
			{
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
mod tests {
	use super::*;

	struct TestConfig;
	impl crate::system::Config for TestConfig {
		type AccountId = String;
		type BlockNumber = u32;
		type Nonce = u32;
	}
	impl Config for TestConfig {
		type Content = String;
	}

	fn new() -> Pallet<TestConfig> {
		Pallet::<TestConfig>::new()
	}

	#[test]
	fn get_claim_returns_none_for_missing_content() {
		assert_eq!(new().get_claim(&"ghost".to_string()), None);
	}

	#[test]
	fn create_claim_stores_owner() {
		let mut poe = new();
		assert_eq!(poe.create_claim("alice".to_string(), "doc".to_string()), Ok(()));
		assert_eq!(poe.get_claim(&"doc".to_string()), Some(&"alice".to_string()));
	}

	#[test]
	fn create_duplicate_claim_fails() {
		let mut poe = new();
		poe.create_claim("alice".to_string(), "doc".to_string()).unwrap();
		assert_eq!(
			poe.create_claim("bob".to_string(), "doc".to_string()),
			Err("this content is already claimed")
		);
		// original owner unchanged
		assert_eq!(poe.get_claim(&"doc".to_string()), Some(&"alice".to_string()));
	}

	#[test]
	fn revoke_claim_removes_it() {
		let mut poe = new();
		poe.create_claim("alice".to_string(), "doc".to_string()).unwrap();
		assert_eq!(poe.revoke_claim("alice".to_string(), "doc".to_string()), Ok(()));
		assert_eq!(poe.get_claim(&"doc".to_string()), None);
	}

	#[test]
	fn revoke_nonexistent_claim_fails() {
		let mut poe = new();
		assert_eq!(
			poe.revoke_claim("alice".to_string(), "ghost".to_string()),
			Err("claim does not exist")
		);
	}

	#[test]
	fn revoke_claim_wrong_owner_fails() {
		let mut poe = new();
		poe.create_claim("alice".to_string(), "doc".to_string()).unwrap();
		assert_eq!(
			poe.revoke_claim("bob".to_string(), "doc".to_string()),
			Err("caller is not owner")
		);
		// claim still belongs to alice
		assert_eq!(poe.get_claim(&"doc".to_string()), Some(&"alice".to_string()));
	}

	#[test]
	fn reclaim_after_revoke_succeeds() {
		let mut poe = new();
		poe.create_claim("alice".to_string(), "doc".to_string()).unwrap();
		poe.revoke_claim("alice".to_string(), "doc".to_string()).unwrap();
		assert_eq!(poe.create_claim("bob".to_string(), "doc".to_string()), Ok(()));
		assert_eq!(poe.get_claim(&"doc".to_string()), Some(&"bob".to_string()));
	}

	#[test]
	fn multiple_claims_are_independent() {
		let mut poe = new();
		poe.create_claim("alice".to_string(), "doc1".to_string()).unwrap();
		poe.create_claim("bob".to_string(), "doc2".to_string()).unwrap();
		assert_eq!(poe.get_claim(&"doc1".to_string()), Some(&"alice".to_string()));
		assert_eq!(poe.get_claim(&"doc2".to_string()), Some(&"bob".to_string()));
	}

	#[test]
	fn revoking_one_claim_does_not_affect_others() {
		let mut poe = new();
		poe.create_claim("alice".to_string(), "doc1".to_string()).unwrap();
		poe.create_claim("alice".to_string(), "doc2".to_string()).unwrap();
		poe.revoke_claim("alice".to_string(), "doc1".to_string()).unwrap();
		assert_eq!(poe.get_claim(&"doc1".to_string()), None);
		assert_eq!(poe.get_claim(&"doc2".to_string()), Some(&"alice".to_string()));
	}
}
