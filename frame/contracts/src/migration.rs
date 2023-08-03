// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use codec::{Decode, Encode};
use core::cmp::{max, min};
use frame_support::{codec, Identity, Twox64Concat, storage_alias, LOG_TARGET};
use frame_support::pallet_prelude::*;
use frame_support::storage::migration;
use frame_support::traits::fungible::Inspect;
use frame_support::traits::{Get, OnRuntimeUpgrade, ReservableCurrency, Currency, ExistenceRequirement};
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::traits::{Saturating, Zero};
use sp_runtime::Perbill;
use sp_std::marker::PhantomData; 
use sp_std::prelude::*;
use sp_std::ops::Deref;

use crate::{AccountIdOf, BalanceOf, CodeHash, Config, Pallet, TrieId, Weight};
use crate::address::AddressGenerator;

/// Performs all necessary migrations based on `StorageVersion`.
pub struct Migration<T: Config>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let version = <Pallet<T>>::on_chain_storage_version();
		let mut weight = Weight::zero();

		if version < 4 {
			v4::migrate::<T>(&mut weight);
		}

		if version < 5 {
			v5::migrate::<T>(&mut weight);
		}

		if version < 6 {
			v6::migrate::<T>(&mut weight);
		}

		if version < 7 {
			v7::migrate::<T>(&mut weight);
		}

		if version < 8 {
			v8::migrate::<T>(&mut weight);
		}

		if version < 9 {
			v9::migrate::<T>(&mut weight);
		}

		if version < 10 {
			v10::migrate::<T>(&mut weight);
		}

		StorageVersion::new(10).put::<Pallet<T>>();
		weight.saturating_accrue(T::DbWeight::get().writes(1));

		weight
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		let version = <Pallet<T>>::on_chain_storage_version();

		if version == 7 {
			v8::pre_upgrade::<T>()?;
		}

		Ok(version.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
		let version = Decode::decode(&mut state.as_ref()).map_err(|_| "Cannot decode version")?;
		post_checks::post_upgrade::<T>(version)
	}
}

/// V4: `Schedule` is changed to be a config item rather than an in-storage value.
mod v4 {
	use super::*;

	pub fn migrate<T: Config>(weight: &mut Weight) {
		#[allow(deprecated)]
		migration::remove_storage_prefix(<Pallet<T>>::name().as_bytes(), b"CurrentSchedule", b"");
		weight.saturating_accrue(T::DbWeight::get().writes(1));
	}
}

/// V5: State rent is removed which obsoletes some fields in `ContractInfo`.
mod v5 {
	use super::*;

	type AliveContractInfo<T> =
		RawAliveContractInfo<CodeHash<T>, BalanceOf<T>, <T as frame_system::Config>::BlockNumber>;
	type TombstoneContractInfo<T> = RawTombstoneContractInfo<
		<T as frame_system::Config>::Hash,
		<T as frame_system::Config>::Hashing,
	>;

	#[derive(Decode)]
	enum OldContractInfo<T: Config> {
		Alive(AliveContractInfo<T>),
		Tombstone(TombstoneContractInfo<T>),
	}

	#[derive(Decode)]
	struct RawAliveContractInfo<CodeHash, Balance, BlockNumber> {
		trie_id: TrieId,
		_storage_size: u32,
		_pair_count: u32,
		code_hash: CodeHash,
		_rent_allowance: Balance,
		_rent_paid: Balance,
		_deduct_block: BlockNumber,
		_last_write: Option<BlockNumber>,
		_reserved: Option<()>,
	}

	#[derive(Decode)]
	struct RawTombstoneContractInfo<H, Hasher>(H, PhantomData<Hasher>);

	#[derive(Decode)]
	struct OldDeletedContract {
		_pair_count: u32,
		trie_id: TrieId,
	}

	pub type ContractInfo<T> = RawContractInfo<CodeHash<T>>;

	#[derive(Encode, Decode)]
	pub struct RawContractInfo<CodeHash> {
		pub trie_id: TrieId,
		pub code_hash: CodeHash,
		pub _reserved: Option<()>,
	}

	#[derive(Encode, Decode)]
	struct DeletedContract {
		trie_id: TrieId,
	}

	#[storage_alias]
	type ContractInfoOf<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		ContractInfo<T>,
	>;

	#[storage_alias]
	type DeletionQueue<T: Config> = StorageValue<Pallet<T>, Vec<DeletedContract>>;

	pub fn migrate<T: Config>(weight: &mut Weight) {
		<ContractInfoOf<T>>::translate(|_key, old: OldContractInfo<T>| {
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			match old {
				OldContractInfo::Alive(old) => Some(ContractInfo::<T> {
					trie_id: old.trie_id,
					code_hash: old.code_hash,
					_reserved: old._reserved,
				}),
				OldContractInfo::Tombstone(_) => None,
			}
		});

		DeletionQueue::<T>::translate(|old: Option<Vec<OldDeletedContract>>| {
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			old.map(|old| old.into_iter().map(|o| DeletedContract { trie_id: o.trie_id }).collect())
		})
		.ok();
	}
}

/// V6: Added storage deposits
mod v6 {
	use super::*;

	#[derive(Encode, Decode)]
	struct OldPrefabWasmModule {
		#[codec(compact)]
		instruction_weights_version: u32,
		#[codec(compact)]
		initial: u32,
		#[codec(compact)]
		maximum: u32,
		#[codec(compact)]
		refcount: u64,
		_reserved: Option<()>,
		code: Vec<u8>,
		original_code_len: u32,
	}

	#[derive(Encode, Decode)]
	pub struct PrefabWasmModule {
		#[codec(compact)]
		pub instruction_weights_version: u32,
		#[codec(compact)]
		pub initial: u32,
		#[codec(compact)]
		pub maximum: u32,
		pub code: Vec<u8>,
	}

	use v5::ContractInfo as OldContractInfo;

	#[derive(Encode, Decode)]
	pub struct RawContractInfo<CodeHash, Balance> {
		pub trie_id: TrieId,
		pub code_hash: CodeHash,
		pub storage_deposit: Balance,
	}

	#[derive(Encode, Decode)]
	pub struct OwnerInfo<T: Config> {
		owner: T::AccountId,
		#[codec(compact)]
		deposit: BalanceOf<T>,
		#[codec(compact)]
		refcount: u64,
	}

	pub type ContractInfo<T> = RawContractInfo<CodeHash<T>, BalanceOf<T>>;

	#[storage_alias]
	type ContractInfoOf<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		ContractInfo<T>,
	>;

	#[storage_alias]
	type CodeStorage<T: Config> = StorageMap<Pallet<T>, Identity, CodeHash<T>, PrefabWasmModule>;

	#[storage_alias]
	type OwnerInfoOf<T: Config> = StorageMap<Pallet<T>, Identity, CodeHash<T>, OwnerInfo<T>>;

	pub fn migrate<T: Config>(weight: &mut Weight) {
		<ContractInfoOf<T>>::translate(|_key, old: OldContractInfo<T>| {
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			Some(ContractInfo::<T> {
				trie_id: old.trie_id,
				code_hash: old.code_hash,
				storage_deposit: Default::default(),
			})
		});

		let nobody = T::AccountId::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes())
			.expect("Infinite input; no dead input space; qed");

		<CodeStorage<T>>::translate(|key, old: OldPrefabWasmModule| {
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 2));
			<OwnerInfoOf<T>>::insert(
				key,
				OwnerInfo {
					refcount: old.refcount,
					owner: nobody.clone(),
					deposit: Default::default(),
				},
			);
			Some(PrefabWasmModule {
				instruction_weights_version: old.instruction_weights_version,
				initial: old.initial,
				maximum: old.maximum,
				code: old.code,
			})
		});
	}
}

/// Rename `AccountCounter` to `Nonce`.
mod v7 {
	use super::*;

	pub fn migrate<T: Config>(weight: &mut Weight) {
		#[storage_alias]
		type AccountCounter<T: Config> = StorageValue<Pallet<T>, u64, ValueQuery>;
		#[storage_alias]
		type Nonce<T: Config> = StorageValue<Pallet<T>, u64, ValueQuery>;

		Nonce::<T>::set(AccountCounter::<T>::take());
		weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 2))
	}
}

/// Update `ContractInfo` with new fields that track storage deposits.
mod v8 {
	use super::*;
	use sp_io::default_child_storage as child;
	use v6::ContractInfo as OldContractInfo;

	#[derive(Encode, Decode)]
	pub struct ContractInfo<T: Config> {
		pub trie_id: TrieId,
		pub code_hash: CodeHash<T>,
		pub storage_bytes: u32,
		pub storage_items: u32,
		pub storage_byte_deposit: BalanceOf<T>,
		pub storage_item_deposit: BalanceOf<T>,
		pub storage_base_deposit: BalanceOf<T>,
	}

	#[storage_alias]
	type ContractInfoOf<T: Config, V> =
		StorageMap<Pallet<T>, Twox64Concat, <T as frame_system::Config>::AccountId, V>;

	pub fn migrate<T: Config>(weight: &mut Weight) {
		<ContractInfoOf<T, ContractInfo<T>>>::translate_values(|old: OldContractInfo<T>| {
			// Count storage items of this contract
			let mut storage_bytes = 0u32;
			let mut storage_items = 0u32;
			let mut key = Vec::new();
			while let Some(next) = child::next_key(&old.trie_id, &key) {
				key = next;
				let mut val_out = [];
				let len = child::read(&old.trie_id, &key, &mut val_out, 0)
					.expect("The loop conditions checks for existence of the key; qed");
				storage_bytes.saturating_accrue(len);
				storage_items.saturating_accrue(1);
			}

			let storage_byte_deposit =
				T::DepositPerByte::get().saturating_mul(storage_bytes.into());
			let storage_item_deposit =
				T::DepositPerItem::get().saturating_mul(storage_items.into());
			let storage_base_deposit = old
				.storage_deposit
				.saturating_sub(storage_byte_deposit)
				.saturating_sub(storage_item_deposit);

			// Reads: One read for each storage item plus the contract info itself.
			// Writes: Only the new contract info.
			weight.saturating_accrue(
				T::DbWeight::get().reads_writes(u64::from(storage_items) + 1, 1),
			);

			Some(ContractInfo {
				trie_id: old.trie_id,
				code_hash: old.code_hash,
				storage_bytes,
				storage_items,
				storage_byte_deposit,
				storage_item_deposit,
				storage_base_deposit,
			})
		});
	}

	#[cfg(feature = "try-runtime")]
	pub fn pre_upgrade<T: Config>() -> Result<(), &'static str> {
		use frame_support::traits::ReservableCurrency;
		for (key, value) in ContractInfoOf::<T, OldContractInfo<T>>::iter() {
			let reserved = T::Currency::reserved_balance(&key);
			ensure!(reserved >= value.storage_deposit, "Reserved balance out of sync.");
		}
		Ok(())
	}
}

/// Update `CodeStorage` with the new `determinism` field.
mod v9 {
	use super::*;
	use crate::Determinism;
	use v6::PrefabWasmModule as OldPrefabWasmModule;

	#[derive(Encode, Decode)]
	pub struct PrefabWasmModule {
		#[codec(compact)]
		pub instruction_weights_version: u32,
		#[codec(compact)]
		pub initial: u32,
		#[codec(compact)]
		pub maximum: u32,
		pub code: Vec<u8>,
		pub determinism: Determinism,
	}

	#[storage_alias]
	type CodeStorage<T: Config> = StorageMap<Pallet<T>, Identity, CodeHash<T>, PrefabWasmModule>;

	pub fn migrate<T: Config>(weight: &mut Weight) {
		<CodeStorage<T>>::translate_values(|old: OldPrefabWasmModule| {
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			Some(PrefabWasmModule {
				instruction_weights_version: old.instruction_weights_version,
				initial: old.initial,
				maximum: old.maximum,
				code: old.code,
				determinism: Determinism::Deterministic,
			})
		});
	}
}

mod v10 {
	use super::*;
	use v8::ContractInfo as OldContractInfo;

	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct ContractInfo<T: Config> {
		pub trie_id: TrieId,
		deposit_account: DepositAccount<T>,
		pub code_hash: CodeHash<T>,
		storage_bytes: u32,
		storage_items: u32,
		pub storage_byte_deposit: BalanceOf<T>,
		storage_item_deposit: BalanceOf<T>,
		storage_base_deposit: BalanceOf<T>,
	}

	#[storage_alias]
	pub type ContractInfoOf<T: Config, V> =
		StorageMap<Pallet<T>, Twox64Concat, <T as frame_system::Config>::AccountId, V>;

	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct DepositAccount<T: Config>(AccountIdOf<T>);

	impl<T: Config> Deref for DepositAccount<T> {
		type Target = AccountIdOf<T>;

		fn deref(&self) -> &Self::Target {
			&self.0
		}
	}

	pub fn migrate<T: Config>(weight: &mut Weight) {
		<ContractInfoOf<T, ContractInfo<T>>>::translate(|account, contract: OldContractInfo<T>| {
			log::debug!(target: LOG_TARGET, "Account: 0x{} ", HexDisplay::from(&account.encode()));
			// Get the new deposit account address
			let deposit_account: DepositAccount<T> =
				DepositAccount(T::AddressGenerator::deposit_address(&account));

			let min_balance = Pallet::<T>::min_balance();
			// Calculate the existing deposit, that should be reserved on the contract account
			let old_deposit = contract
				.storage_base_deposit
				.saturating_add(contract.storage_item_deposit)
				.saturating_add(contract.storage_byte_deposit);

			// Unreserve the existing deposit
			// Note we can't use repatriate_reserve, because it only works with existing accounts
			let remaining = T::Currency::unreserve(&account, old_deposit);
			if !remaining.is_zero() {
				log::warn!(
					target: LOG_TARGET,
					"Partially unreserved. Remaining {:?} out of {:?} asked",
					remaining,
					old_deposit
				);
			}

			// Attempt to transfer the old deposit to the deposit account.
			let amount = old_deposit
				.saturating_sub(min_balance)
				.min(T::Currency::reducible_balance(&account, true));

			let new_deposit = T::Currency::transfer_no_cdd(
				&account,
				&deposit_account,
				amount,
				ExistenceRequirement::KeepAlive,
			)
			.map(|_| {
				log::debug!(
					target: LOG_TARGET,
					"Transferred deposit ({:?}) to deposit account",
					amount
				);
				amount
			})
			// If it fails we fallback to minting the ED.
			.unwrap_or_else(|err| {
				log::error!(target: LOG_TARGET, "Failed to transfer the base deposit, reason: {:?}", err);
				T::Currency::deposit_creating(&deposit_account, min_balance);
				min_balance
			});

			// Calculate the new base_deposit to store in the contract:
			// Ideally, it should be the same as the old one
			// Ideally, it should be at least 2xED (for the contract and deposit accounts).
			// It can't be more than the `new_deposit`.
			let new_base_deposit = min(
				max(contract.storage_base_deposit, min_balance.saturating_add(min_balance)),
				new_deposit,
			);

			// Calculate the ratio to adjust storage_byte and storage_item deposits.
			let new_deposit_without_base = new_deposit.saturating_sub(new_base_deposit);
			let old_deposit_without_base =
				old_deposit.saturating_sub(contract.storage_base_deposit);
			let ratio = Perbill::from_rational(new_deposit_without_base, old_deposit_without_base);

			// Calculate the new storage deposits based on the ratio
			let storage_byte_deposit = ratio.mul_ceil(contract.storage_byte_deposit);
			let storage_item_deposit = ratio.mul_ceil(contract.storage_item_deposit);

			// Recalculate the new base deposit, instead of using new_base_deposit to avoid rounding
			// errors
			let storage_base_deposit = new_deposit
				.saturating_sub(storage_byte_deposit)
				.saturating_sub(storage_item_deposit);

			// Reads: One read for each storage item plus the contract info itself.
			// Writes: Only the new contract info.
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));

			Some(ContractInfo {
				trie_id: contract.trie_id,
				deposit_account,
				code_hash: contract.code_hash,
				storage_bytes: contract.storage_bytes,
				storage_items: contract.storage_items,
				storage_byte_deposit,
				storage_item_deposit,
				storage_base_deposit,
			})
		});
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade_step() -> Result<Vec<u8>, TryRuntimeError> {
		let sample: Vec<_> = ContractInfoOf::<T>::iter().take(10).collect();

		log::debug!(target: LOG_TARGET, "Taking sample of {} contracts", sample.len());
		Ok(sample.encode())
	}
}

// Post checks always need to be run against the latest storage version. This is why we
// do not scope them in the per version modules. They always need to be ported to the latest
// version.
#[cfg(feature = "try-runtime")]
mod post_checks {
	use super::*;
	use crate::Determinism;
	use sp_io::default_child_storage as child;
	use v8::ContractInfo;
	use v9::PrefabWasmModule;

	#[storage_alias]
	type CodeStorage<T: Config> = StorageMap<Pallet<T>, Identity, CodeHash<T>, PrefabWasmModule>;

	#[storage_alias]
	type ContractInfoOf<T: Config, V> =
		StorageMap<Pallet<T>, Twox64Concat, <T as frame_system::Config>::AccountId, V>;

	pub fn post_upgrade<T: Config>(old_version: StorageVersion) -> Result<(), &'static str> {
		if old_version < 7 {
			return Ok(())
		}

		if old_version < 8 {
			v8::<T>()?;
		}

		if old_version < 9 {
			v9::<T>()?;
		}

		if old_version < 10 {
			v10::<T>()?;
		}

		Ok(())
	}

	fn v8<T: Config>() -> Result<(), &'static str> {
		use frame_support::traits::ReservableCurrency;
		for (key, value) in ContractInfoOf::<T, ContractInfo<T>>::iter() {
			let reserved = T::Currency::reserved_balance(&key);
			let stored = value
				.storage_base_deposit
				.saturating_add(value.storage_byte_deposit)
				.saturating_add(value.storage_item_deposit);
			ensure!(reserved >= stored, "Reserved balance out of sync.");

			let mut storage_bytes = 0u32;
			let mut storage_items = 0u32;
			let mut key = Vec::new();
			while let Some(next) = child::next_key(&value.trie_id, &key) {
				key = next;
				let mut val_out = [];
				let len = child::read(&value.trie_id, &key, &mut val_out, 0)
					.expect("The loop conditions checks for existence of the key; qed");
				storage_bytes.saturating_accrue(len);
				storage_items.saturating_accrue(1);
			}
			ensure!(storage_bytes == value.storage_bytes, "Storage bytes do not match.",);
			ensure!(storage_items == value.storage_items, "Storage items do not match.",);
		}
		Ok(())
	}

	fn v9<T: Config>() -> Result<(), &'static str> {
		for value in CodeStorage::<T>::iter_values() {
			ensure!(
				value.determinism == Determinism::Deterministic,
				"All pre-existing codes need to be deterministic."
			);
		}
		Ok(())
	}

	fn v10<T: Config>() -> Result<(), &'static str> {
		let sample = <Vec<(T::AccountId, old::ContractInfo<T>)> as Decode>::decode(&mut &state[..])
			.expect("pre_upgrade_step provides a valid state; qed");

		log::debug!(target: LOG_TARGET, "Validating sample of {} contracts", sample.len());
		for (account, old_contract) in sample {
			log::debug!(target: LOG_TARGET, "===");
			log::debug!(target: LOG_TARGET, "Account: 0x{} ", HexDisplay::from(&account.encode()));
			let contract = ContractInfoOf::<T>::get(&account).unwrap();
			ensure!(old_contract.trie_id == contract.trie_id, "invalid trie_id");
			ensure!(old_contract.code_hash == contract.code_hash, "invalid code_hash");
			ensure!(old_contract.storage_bytes == contract.storage_bytes, "invalid storage_bytes");
			ensure!(old_contract.storage_items == contract.storage_items, "invalid storage_items");

			let deposit =
				<<T as Config>::Currency as frame_support::traits::Currency<_>>::total_balance(
					&contract.deposit_account,
				);
			ensure!(
				deposit ==
					contract
						.storage_base_deposit
						.saturating_add(contract.storage_item_deposit)
						.saturating_add(contract.storage_byte_deposit),
				"deposit mismatch"
			);
		}

		Ok(())
	}
}
