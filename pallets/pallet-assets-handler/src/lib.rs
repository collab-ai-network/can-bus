// Copyright 2020-2024 Trust Computing GmbH.
// This file is part of Litentry.
//
// Litentry is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Litentry is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Litentry.  If not, see <https://www.gnu.org/licenses/>.

//! A pallet for temporary fix of onchain accountInfo.
//! No storage for this pallet and it should be removed right after fixing.
#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	pallet_prelude::*,
	traits::{
		tokens::{fungible::Mutate, fungibles::Mutate},
		Currency, ReservableCurrency, StorageVersion,
	},
};
use frame_system::pallet_prelude::*;
use hex_literal::hex;
pub use pallet::*;
use sp_runtime::traits::StaticLookup;

use sp_std::vec::Vec;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
type AssetId<T> = <T as pallet_assets::Config>::AssetId;
type ResourceId = pallet_bridge::ResourceId;

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct AssetInfo {
	fee: Balance,
	// None for native token
	asset: Option<AssetId>,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config + pallet_balances::Config + pallet_assets::Config
	{
		/// Overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin used to administer the pallet
		type BridgeCommitteeOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The units in which we record balances.
		type Balance: Member
			+ Parameter
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen
			+ TypeInfo;

		/// Treasury account to receive assets fee
		type TreasuryAccount: Get<Self::AccountId>;
	}

	// Resource Id of pallet assets token
	#[pallet::storage]
	#[pallet::getter(fn resource_to_asset_info)]
	pub type ResourceToAssetInfo<T: Config> =
		StorageMap<_, Twox64Concat, ResourceId, AssetInfo, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		// asset id = None means native token
		ResourceUpdated { resource_id: ResourceId, asset: AssetInfo },
		ResourceRemoved { resource_id: ResourceId },
	}

	#[pallet::error]
	pub enum Error<T> {
		InvalidResourceId,
		CannotPayAsFee,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Stores an asset id on chain under an associated resource ID.
		#[pallet::call_index(0)]
		#[pallet::weight({1000})]
		pub fn set_resource(
			origin: OriginFor<T>,
			resource_id: ResourceId,
			asset: AssetInfo,
		) -> DispatchResult {
			T::BridgeCommitteeOrigin::ensure_origin(origin)?;
			ResourceToAssetInfo::<T>::insert(resource_id, asset);
			Self::deposit_event(Event::ResourceUpdated(resource_id, asset));
			Ok(())
		}

		/// Removes a resource ID from the resource mapping.
		///
		/// After this call, bridge transfers with the associated resource ID will
		/// be rejected.
		#[pallet::call_index(1)]
		#[pallet::weight({1000})]
		pub fn remove_resource(origin: OriginFor<T>, resource_id: ResourceId) -> DispatchResult {
			T::BridgeCommitteeOrigin::ensure_origin(origin)?;
			ResourceToAssetInfo::<T>::remove(resource_id);
			Self::deposit_event(Event::ResourceRemoved(resource_id));
			Ok(())
		}
	}

	impl<T, Balance> BridgeHandler<Balance, T::AcountId, ResourceId> for Pallet<T>
	where
		T: Config<Balance = Balance>
			+ pallet_asset::Config<Balance = Balance>
			+ pallet_balances::Config<Balance = Balance>,
	{
		fn prepare_token_bridge_in(
			resource_id: ResourceId,
			who: T::AcountId,
			amount: Balance,
		) -> Result<Balance, DispatchError> {
			let asset_info = Self::resource_to_asset_info(resource_id);
			match asset_info {
				None => Err(Error::<T>::InvalidResourceId.into()),
				// Native token
				Some(asset_info_inner) if asset_info_inner.asset == None => {
					pallet_balances::Pallet::<T>::mint_into(who, amount)
				},
				// pallet assets
				Some(asset_info_inner) => pallet_assets::Pallet::<T>::mint_into(asset, who, amount),
			}
		}
		// Return actual amount to target chain after deduction e.g fee
		fn prepare_token_bridge_out(
			resource_id: ResourceId,
			who: T::AcountId,
			amount: Balance,
		) -> Result<Balance, DispatchError> {
			let asset_info = Self::resource_to_asset_info(resource_id);
			match asset_info {
				None => Err(Error::<T>::InvalidResourceId.into()),
				// Native token
				Some(AssetInfo { fee: fee, asset: None }) => {
					let burn_amount = pallet_balances::Pallet::<T>::burn_from(
						who,
						amount,
						Preservation::Preserve,
						Precision::Exact,
						Fortitude::Polite,
					)?;
					ensure!(burn_amount > fee, Error::<T>::CannotPayAsFee);
					pallet_balances::Pallet::<T>::mint_into(T::TreasuryAccount::get(), fee)?;
					Ok((burn_amount - fee))
				},
				// pallet assets
				Some(AssetInfo { fee: fee, asset: Some(asset) }) => {
					let burn_amount = pallet_assets::Pallet::<T>::burn_from(
						asset,
						who,
						amount,
						Preservation::Expendable,
						Precision::Exact,
						Fortitude::Polite,
					)?;
					ensure!(burn_amount > fee, Error::<T>::CannotPayAsFee);
					pallet_assets::Pallet::<T>::mint_into(asset, T::TreasuryAccount::get(), fee)?;
					Ok((burn_amount - fee))
				},
			}
		}
	}
}
