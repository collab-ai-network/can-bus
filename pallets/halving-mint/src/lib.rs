//! # Pallet halving-mint
//!
//! This pallet mints the (native) token in a halving way.
//!
//! It will be parameterized with the total issuance count and halving interval (in blocks),
//! The minted token is deposited to the `beneficiary` account, which should be a privated
//! account derived from the PalletId(similar to treasury). There's a trait `OnTokenMinted`
//! to hook the callback into other pallet.
//!
//! The main parameters:
//! - total issuance
//! - halving interval
//! - beneficiary account
//! are defined as runtime constants. It implies that once onboarded, they can be changed
//! only by runtime upgrade. Thus it has a stronger guarantee in comparison to extrinsics.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::too_many_arguments)]

use frame_support::traits::Currency;
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod traits;
pub use traits::OnTokenMinted;

pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		pallet_prelude::*,
		traits::{ReservableCurrency, StorageVersion},
		PalletId,
	};
	use frame_system::pallet_prelude::{BlockNumberFor, *};
	use sp_runtime::traits::{AccountIdConversion, One, Zero};

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The origin to control the minting configuration
		type ManagerOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		/// The total issuance of the (native) token
		#[pallet::constant]
		type TotalIssuance: Get<BalanceOf<Self>>;
		/// Halving internal in blocks, we force u32 type, BlockNumberFor<T> implements
		/// AtLeast32BitUnsigned so it's safe
		#[pallet::constant]
		type HalvingInterval: Get<u32>;
		/// The beneficiary PalletId, used for deriving its sovereign AccountId
		#[pallet::constant]
		type BeneficiaryId: Get<PalletId>;
		/// Hook for other pallets to deal with OnTokenMinted event
		type OnTokenMinted: OnTokenMinted<Self::AccountId, BalanceOf<Self>>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		MintStateChanged { enabled: bool },
		MintStarted { start_block: BlockNumberFor<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		MintStateUnchanged,
		MintAlreadyStarted,
		MintNotStarted,
	}

	#[pallet::storage]
	#[pallet::getter(fn enabled)]
	pub type Enabled<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn start_block)]
	pub type StartBlock<T: Config> = StorageValue<_, BlockNumberFor<T>, OptionQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub enabled: bool,
		pub start_block: Option<BlockNumberFor<T>>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { enabled: false, start_block: None }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			Enabled::<T>::put(self.enabled);
			if let Some(n) = self.start_block {
				StartBlock::<T>::put(n);
			}
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			let mut weight = Weight::zero();
			if Self::enabled() {
				if let Some(start_block) = Self::start_block() {
					// 2 reads: `enabled`, `start_block`
					weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 0));

					// should not happen but a sanity check
					if now < start_block {
						return weight;
					}

					let halving_interval = T::HalvingInterval::get();

					// calculate the amount of initially minted tokens before first halving
					let mut minted = T::TotalIssuance::get() / (halving_interval * 2).into();
					// halving round index
					let halving_round = (now - start_block) / halving_interval.into();

					// 2 reads: `total_issuance`, `halving_interval`
					weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 0));

					// if we want to use bit shift, we need to:
					//   1. know the overlfow limit similar to what bitcoin has: `if (halvings >=
					//      64) return 0;` so 127 for u128
					//   2. coerce the `halving_round` to u32
					// but both `halving_round` and `minted` are associated types that need to be
					// defined during runtime binding thus plain division is used
					let mut i = BlockNumberFor::<T>::zero();
					while i < halving_round {
						minted = minted / 2u32.into();
						i += BlockNumberFor::<T>::one();
					}

					// theoreticlaly we can deal with the minted tokens directly in the trait impl
					// pallet, without depositing to an account first.
					// but the purpose of having the extra logic is to make sure the tokens are
					// minted to the beneficiary account, regardless of what happens callback. Even
					// if the callback errors out, it's guaranteed that the tokens are
					// already minted (and stored on an account), which resonates with the "fair
					// launch" concept.
					//
					// Also imagine there's no callback impl, in this case the tokens will still be
					// minted and accumulated.
					let _ = T::Currency::deposit_creating(&Self::beneficiary_account(), minted);
					weight = weight.saturating_add(T::OnTokenMinted::token_minted(
						Self::beneficiary_account(),
						minted,
					));
				}
			}
			weight
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((195_000_000, DispatchClass::Normal))]
		pub fn set_enabled(origin: OriginFor<T>, enabled: bool) -> DispatchResultWithPostInfo {
			T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(enabled != Self::enabled(), Error::<T>::MintStateUnchanged);
			Enabled::<T>::put(enabled);
			Self::deposit_event(Event::MintStateChanged { enabled });
			Ok(Pays::No.into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight((195_000_000, DispatchClass::Normal))]
		pub fn start_mint(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(StartBlock::<T>::get().is_none(), Error::<T>::MintAlreadyStarted);
			let start_block = frame_system::Pallet::<T>::block_number();
			Enabled::<T>::put(true);
			StartBlock::<T>::put(start_block);

			// set the beneficiary account as self-sufficient to not get reaped even provider and
			// consumer rc is 0 TODO: where to dec_sufficients?
			if frame_system::Pallet::<T>::sufficients(&Self::beneficiary_account()) == 0 {
				frame_system::Pallet::<T>::inc_sufficients(&Self::beneficiary_account());
			}
			Self::deposit_event(Event::MintStarted { start_block });
			Ok(Pays::No.into())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn beneficiary_account() -> T::AccountId {
			T::BeneficiaryId::get().into_account_truncating()
		}
	}
}
