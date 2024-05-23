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

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use crate::weights::WeightInfo;
	use frame_support::{
		pallet_prelude::*,
		traits::{fungible::Mutate, Currency, SortedMembers, StorageVersion},
		transactional,
	};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{BadOrigin, CheckedAdd, CheckedSub};
	use sp_std::vec::Vec;

	pub use pallet_bridge as bridge;

	type ResourceId = bridge::ResourceId;

	type BalanceOf<T> = <<T as bridge::Config>::Currency as Currency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + bridge::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Specifies the origin check provided by the bridge for calls that can only be called by
		/// the bridge pallet
		type BridgeOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

		/// The priviledged accounts to call the transfer_native
		type TransferNativeMembers: SortedMembers<Self::AccountId>;

		// Handler of asset transfer/burn/mint etc.
		type BridgeHandler: BridgeHandler<BalanceOf<Self>, Self::AccountId, ResourceId>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A certain amount of native tokens was minted
		NativeTokenMinted {
			to: T::AccountId,
			amount: BalanceOf<T>,
		},
		AssetTokenMinted {
			to: T::AccountId,
			amount: BalanceOf<T>,
		},
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Transfers some amount of non-native token to some recipient on a (whitelisted)
		/// destination chain.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::transfer_native())]
		#[transactional]
		pub fn transfer_assets(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
			recipient: Vec<u8>,
			dest_id: bridge::BridgeChainId,
			resource_id: ResourceId,
		) -> DispatchResult {
			let source = ensure_signed(origin)?;
			ensure!(T::TransferNativeMembers::contains(&source), BadOrigin);
			let actual_dest_amount =
				T::BridgeHandler::prepare_token_bridge_out(resource_id, source, amount)?;
			<bridge::Pallet<T>>::signal_transfer_fungible(
				source,
				dest_id,
				resource_id,
				recipient,
				actual_dest_amount,
			)
		}

		/// Executes a simple currency transfer using the bridge account as the source
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::transfer())]
		pub fn transfer(
			origin: OriginFor<T>,
			to: T::AccountId,
			amount: BalanceOf<T>,
			rid: ResourceId,
		) -> DispatchResult {
			T::BridgeOrigin::ensure_origin(origin)?;
			T::BridgeHandler::prepare_token_bridge_in(rid, to, amount)?;
			Ok(())
		}
	}

	pub trait BridgeHandler<B, A, R> {
		fn prepare_token_bridge_in(resource_id: R, who: A, amount: B) -> Result<B, DispatchError>;
		// Return actual amount to target chain after deduction e.g fee
		fn prepare_token_bridge_out(resource_id: R, who: A, amount: B) -> Result<B, DispatchError>;
	}
}
