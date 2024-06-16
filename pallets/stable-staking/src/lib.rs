#![cfg_attr(not(feature = "std"), no_std)]
use frame_support::{
	pallet_prelude::*,
	traits::{
		tokens::{
			fungible::{Inspect as FInspect, Mutate as FMutate},
			fungibles::{Inspect as FsInspect, Mutate as FsMutate},
			Preservation,
		},
		StorageVersion,
	},
	PalletId,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::{
	traits::{AccountIdConversion, AtLeast32BitUnsigned, CheckedAdd, CheckedDiv, CheckedMul, One},
	ArithmeticError, Perquintill, Saturating,
};
use sp_std::{collections::vec_deque::VecDeque, fmt::Debug, prelude::*};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[derive(PartialEq, Eq, Clone, Encode, Debug, Decode, TypeInfo)]
pub struct StakingInfo<BlockNumber, Balance> {
	// For a single position or
	// Synthetic overall average effective_time weighted by staked amount
	effective_time: BlockNumber,
	// Staked amount
	amount: Balance,
	// This is recorded for not allowing weight calculation when time < some of history effective
	// time
	last_add_time: BlockNumber,
}

#[derive(PartialEq, Eq, Clone, Encode, Debug, Decode, TypeInfo)]
pub struct StakingInfoWithOwner<AccountId, PoolId, StakingInfo> {
	who: AccountId,
	pool_id: PoolId,
	staking_info: StakingInfo,
}

#[derive(PartialEq, Eq, Clone, Encode, Debug, Decode, TypeInfo)]
pub struct StableRewardInfo<Balance> {
	// Epoch index
	epoch: u128,
	// Staked amount
	reward_amount: Balance,
}

impl<BlockNumber, Balance> StakingInfo<BlockNumber, Balance>
where
	Balance: AtLeast32BitUnsigned + Copy,
	BlockNumber: AtLeast32BitUnsigned + Copy,
{
	// Mixing a new added staking position, replace the checkpoint with Synthetic new one
	// Notice: The logic will be wrong if weight calculated time is before any single added
	// effective_time
	fn add(&mut self, effective_time: BlockNumber, amount: Balance) -> Option<()> {
		// If last_add_time always > effective_time, only new added effective time can effect
		// last_add_time
		self.last_add_time = self.last_add_time.max(effective_time);

		// We try force all types into u128, then convert it back
		let e: u128 = effective_time.try_into().ok()?;
		let s: u128 = amount.try_into().ok()?;

		let oe: u128 = self.effective_time.try_into().ok()?;
		let os: u128 = self.amount.try_into().ok()?;

		let new_amount: u128 = os.checked_add(s)?;
		// (oe * os + e * s) / (os + s)
		let new_effective_time: u128 =
			(oe.checked_mul(os)?.checked_add(e.checked_mul(s)?)?).checked_div(new_amount)?;
		self.amount = new_amount.try_into().ok()?;
		self.effective_time = new_effective_time.try_into().ok()?;
		Some(())
	}

	// Claim/Update stake info and return the consumed weight
	fn claim(&mut self, n: BlockNumber) -> Option<u128> {
		// Claim time before last_add_time is not allowed, since weight can not be calculated
		let weight = self.weight(n)?;
		self.effective_time = n;

		Some(weight)
	}

	// consume corresponding weight, change effective time without changing staked amount, return
	// the changed effective time This function is mostly used for Synthetic checkpoint change
	fn claim_based_on_weight(&mut self, weight: u128) -> Option<BlockNumber> {
		let oe: u128 = self.effective_time.try_into().ok()?;
		let os: u128 = self.amount.try_into().ok()?;

		let delta_e: u128 = weight.checked_div(os)?;
		let new_effective_time: BlockNumber = (oe + delta_e).try_into().ok()?;
		self.effective_time = new_effective_time;

		Some(new_effective_time)
	}

	// Withdraw staking amount and return the amount after withdrawal
	fn withdraw(&mut self, v: Balance) -> Option<Balance> {
		self.amount = self.amount.checked_sub(&v)?;

		Some(self.amount)
	}

	// You should never use n < any single effective_time
	// it only works for n > all effective_time
	fn weight(&self, n: BlockNumber) -> Option<u128> {
		// Estimate weight before last_add_time can be biased so not allowed
		if self.last_add_time > n {
			return None;
		}

		let e: u128 = n.checked_sub(&self.effective_time)?.try_into().ok()?;
		let s: u128 = self.amount.try_into().ok()?;
		e.checked_mul(s)
	}

	// Force estimate weight regardless
	fn weight_force(&self, n: BlockNumber) -> Option<u128> {
		let e: u128 = n.checked_sub(&self.effective_time)?.try_into().ok()?;
		let s: u128 = self.amount.try_into().ok()?;
		e.checked_mul(s)
	}
}

#[derive(PartialEq, Eq, Clone, Encode, Debug, Decode, TypeInfo)]
pub struct PoolSetting<BlockNumber, Balance> {
	// The start time of staking pool
	pub start_time: BlockNumber,
	// How many epoch will staking pool last, n > 0, valid epoch index :[0..n)
	pub epoch: u128,
	// How many blocks each epoch consist
	pub epoch_range: BlockNumber,
	// The number of block regarding setup for purchasing hardware which deliver no non-native
	// token reward
	pub setup_time: BlockNumber,
	// Max staked amount of pool
	pub pool_cap: Balance,
}

impl<BlockNumber, Balance> PoolSetting<BlockNumber, Balance>
where
	Balance: AtLeast32BitUnsigned + Copy,
	BlockNumber: AtLeast32BitUnsigned + Copy,
{
	fn end_time(&self) -> Option<BlockNumber> {
		let er: u128 = self.epoch_range.try_into().ok()?;
		let st: u128 = self.start_time.try_into().ok()?;
		let result = st.checked_add(er.checked_mul(self.epoch)?)?;
		result.try_into().ok()
	}
}

#[derive(Clone, Encode, Decode, Eq, PartialEq, Default, RuntimeDebug, MaxEncodedLen, TypeInfo)]
pub struct PoolMetadata<BoundedString> {
	/// The user friendly name of this staking pool. Limited in length by `PoolStringLimit`.
	pub name: BoundedString,
	/// The short description for this staking pool. Limited in length by `PoolStringLimit`.
	pub description: BoundedString,
}

#[frame_support::pallet]
pub mod pallet {
	use frame_support::transactional;

	use super::*;

	type BalanceOf<T> =
		<<T as Config>::Fungibles as FsInspect<<T as frame_system::Config>::AccountId>>::Balance;
	type NativeBalanceOf<T> =
		<<T as Config>::Fungible as FInspect<<T as frame_system::Config>::AccountId>>::Balance;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_assets::Config {
		/// Overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin used to administer the pallet
		type StakingPoolCommitteeOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Identifier for the class of pool.
		type PoolId: Member
			+ Parameter
			+ Clone
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen
			+ Default;

		type Fungibles: FsMutate<Self::AccountId>;

		type Fungible: FMutate<Self::AccountId>;

		/// staking account to receive assets of notion
		#[pallet::constant]
		type StableTokenBeneficiaryId: Get<PalletId>;

		/// The beneficiary PalletId, used for deriving its sovereign AccountId for providing native
		/// token reward
		#[pallet::constant]
		type NativeTokenBeneficiaryId: Get<PalletId>;

		/// The maximum length of a pool name or short description stored on-chain.
		#[pallet::constant]
		type PoolStringLimit: Get<u32>;
	}

	// Metadata of staking pools
	#[pallet::storage]
	#[pallet::getter(fn staking_pool_metadata)]
	pub type StakingPoolMetadata<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::PoolId,
		PoolMetadata<BoundedVec<u8, T::PoolStringLimit>>,
		OptionQuery,
	>;

	// Setting of staking pools
	#[pallet::storage]
	#[pallet::getter(fn staking_pool_setting)]
	pub type StakingPoolSetting<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::PoolId,
		PoolSetting<BlockNumberFor<T>, BalanceOf<T>>,
		OptionQuery,
	>;

	// staking pools' stable token reward waiting claiming
	// Pool id => unclaimed reward part
	#[pallet::storage]
	#[pallet::getter(fn stable_staking_pool_reward)]
	pub type StableStakingPoolReward<T: Config> =
		StorageMap<_, Twox64Concat, T::PoolId, BalanceOf<T>, ValueQuery>;

	// staking pools' stable token reward waiting claiming
	// Pool id, epcoh index => unclaimed reward part
	// TODO: This part is not used and only for recording purpose
	// Currently all reward if user did not claimed on time will be mixed equally based on staking
	// weight That means if APY is low may leads to user not claiming their reward on purpose
	#[pallet::storage]
	#[pallet::getter(fn stable_staking_pool_epoch_reward)]
	pub type StableStakingPoolEpochReward<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::PoolId,
		Twox64Concat,
		u128,
		StableRewardInfo<BalanceOf<T>>,
		OptionQuery,
	>;

	// Checkpoint of single stable staking pool
	// For stable token reward distribution
	// Setup time effect already excluded
	#[pallet::storage]
	#[pallet::getter(fn stable_staking_pool_checkpoint)]
	pub type StableStakingPoolCheckpoint<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::PoolId,
		StakingInfo<BlockNumberFor<T>, BalanceOf<T>>,
		OptionQuery,
	>;

	// Checkpoint of user staking on one single stable staking pool
	// For stable token reward distribution
	// Setup time effect already excluded
	#[pallet::storage]
	#[pallet::getter(fn user_stable_staking_pool_checkpoint)]
	pub type UserStableStakingPoolCheckpoint<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::AccountId,
		Twox64Concat,
		T::PoolId,
		StakingInfo<BlockNumberFor<T>, BalanceOf<T>>,
		OptionQuery,
	>;

	// Checkpoint of overall staking condition synthetic by tracking all staking pool
	// For native token reward distribution
	// Setup time effect included
	#[pallet::storage]
	#[pallet::getter(fn native_checkpoint)]
	pub type NativeCheckpoint<T: Config> =
		StorageValue<_, StakingInfo<BlockNumberFor<T>, BalanceOf<T>>, OptionQuery>;

	// Checkpoint of overall staking condition of a single user synthetic by tracking all staking
	// pool For native token reward distribution
	// Setup time effect included
	#[pallet::storage]
	#[pallet::getter(fn user_native_checkpoint)]
	pub type UserNativeCheckpoint<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		StakingInfo<BlockNumberFor<T>, BalanceOf<T>>,
		OptionQuery,
	>;

	// Temporary holding of all staking extrinsic that during setup period
	// This is not related to native token reward distribution
	#[pallet::storage]
	#[pallet::getter(fn pending_setup)]
	pub type PendingSetup<T: Config> = StorageValue<
		_,
		VecDeque<
			StakingInfoWithOwner<
				T::AccountId,
				T::PoolId,
				StakingInfo<BlockNumberFor<T>, BalanceOf<T>>,
			>,
		>,
		ValueQuery,
	>;

	// Asset id of AIUSD
	#[pallet::storage]
	#[pallet::getter(fn aiusd_asset_id)]
	pub type AIUSDAssetId<T: Config> =
		StorageValue<_, <T::Fungibles as FsInspect<T::AccountId>>::AssetId, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		StakingPoolCreated {
			pool_id: T::PoolId,
			start_time: BlockNumberFor<T>,
			epoch: u128,
			epoch_range: BlockNumberFor<T>,
			setup_time: BlockNumberFor<T>,
			pool_cap: BalanceOf<T>,
		},
		/// New metadata has been set for a staking pool.
		MetadataSet {
			pool_id: T::PoolId,
			name: Vec<u8>,
			description: Vec<u8>,
		},
		/// Metadata has been removed for a staking pool.
		MetadataRemoved {
			pool_id: T::PoolId,
		},
		/// Reward updated
		RewardUpdated {
			pool_id: T::PoolId,
			epoch: u128,
			amount: BalanceOf<T>,
		},
		PendingStakingSolved {
			who: T::AccountId,
			pool_id: T::PoolId,
			effective_time: BlockNumberFor<T>,
			amount: BalanceOf<T>,
		},
		Staked {
			who: T::AccountId,
			pool_id: T::PoolId,
			target_effective_time: BlockNumberFor<T>,
			amount: BalanceOf<T>,
		},
		NativeRewardClaimed {
			who: T::AccountId,
			until_time: BlockNumberFor<T>,
			amount: NativeBalanceOf<T>,
		},
		StableRewardClaimed {
			who: T::AccountId,
			pool_id: T::PoolId,
			until_time: BlockNumberFor<T>,
			amount: BalanceOf<T>,
		},
		Withdraw {
			who: T::AccountId,
			pool_id: T::PoolId,
			time: BlockNumberFor<T>,
			amount: BalanceOf<T>,
		},
		AIUSDRegisted {
			asset_id: <T::Fungibles as FsInspect<T::AccountId>>::AssetId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		RewardAlreadyExisted,
		PoolAlreadyStarted,
		PoolAlreadyEnded,
		PoolAlreadyExisted,
		PoolNotEnded,
		PoolNotExisted,
		PoolNotStarted,
		BadMetadata,
		CannotClaimFuture,
		EpochAlreadyEnded,
		EpochNotExisted,
		NoAssetId,
		TypeIncompatible,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Weight: see `begin_block`
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			if let Some(latest_pending_setup) = <PendingSetup<T>>::get().get(0) {
				// Only trigger if latest pending is effective
				if latest_pending_setup.staking_info.effective_time <= n {
					// Even if we fail to solve_pending, we can not allow error out
					let _ = Self::solve_pending(n);
				}
			}
			Weight::zero()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a staking pool
		#[pallet::call_index(0)]
		#[pallet::weight({1000})]
		pub fn create_staking_pool(
			origin: OriginFor<T>,
			pool_id: T::PoolId,
			setting: PoolSetting<BlockNumberFor<T>, BalanceOf<T>>,
		) -> DispatchResult {
			T::StakingPoolCommitteeOrigin::ensure_origin(origin)?;
			ensure!(
				frame_system::Pallet::<T>::block_number() <= setting.start_time,
				Error::<T>::PoolAlreadyStarted
			);
			ensure!(
				!StakingPoolSetting::<T>::contains_key(&pool_id),
				Error::<T>::PoolAlreadyExisted
			);
			<StakingPoolSetting<T>>::insert(pool_id.clone(), setting.clone());
			Self::deposit_event(Event::StakingPoolCreated {
				pool_id,
				start_time: setting.start_time,
				epoch: setting.epoch,
				epoch_range: setting.epoch_range,
				setup_time: setting.setup_time,
				pool_cap: setting.pool_cap,
			});
			Ok(())
		}

		// name = None will cause remove of metadata
		#[pallet::call_index(1)]
		#[pallet::weight({1000})]
		pub fn update_metadata(
			origin: OriginFor<T>,
			pool_id: T::PoolId,
			name: Option<Vec<u8>>,
			description: Vec<u8>,
		) -> DispatchResult {
			T::StakingPoolCommitteeOrigin::ensure_origin(origin)?;
			ensure!(StakingPoolSetting::<T>::contains_key(&pool_id), Error::<T>::PoolNotExisted);
			if let Some(name_inner) = name {
				let bounded_name: BoundedVec<u8, T::PoolStringLimit> =
					name_inner.clone().try_into().map_err(|_| Error::<T>::BadMetadata)?;

				let bounded_description: BoundedVec<u8, T::PoolStringLimit> =
					description.clone().try_into().map_err(|_| Error::<T>::BadMetadata)?;
				<StakingPoolMetadata<T>>::insert(
					pool_id.clone(),
					PoolMetadata { name: bounded_name, description: bounded_description },
				);
				Self::deposit_event(Event::MetadataSet { pool_id, name: name_inner, description });
			} else {
				<StakingPoolMetadata<T>>::remove(pool_id.clone());
				Self::deposit_event(Event::MetadataRemoved { pool_id });
			}
			Ok(())
		}

		/// Update a reward for a staking pool of specific epoch
		/// Each epoch can be only updated once
		#[pallet::call_index(2)]
		#[pallet::weight({1000})]
		#[transactional]
		pub fn update_reward(
			origin: OriginFor<T>,
			pool_id: T::PoolId,
			epoch: u128,
			reward: BalanceOf<T>,
		) -> DispatchResult {
			T::StakingPoolCommitteeOrigin::ensure_origin(origin)?;

			let current_block = frame_system::Pallet::<T>::block_number();
			// get_epoch_index return setting.epoch if time > pool end time
			let current_epoch = Self::get_epoch_index(pool_id.clone(), current_block)?;
			let setting =
				<StakingPoolSetting<T>>::get(pool_id.clone()).ok_or(Error::<T>::PoolNotExisted)?;
			// Yes.. This allow update epoch = "last" epoch, no matter how expired it is.
			ensure!(setting.epoch >= epoch, Error::<T>::EpochNotExisted);
			ensure!(current_epoch <= epoch, Error::<T>::EpochAlreadyEnded);

			let asset_id = <AIUSDAssetId<T>>::get().ok_or(Error::<T>::NoAssetId)?;
			let actual_reward = T::Fungibles::mint_into(
				asset_id,
				&Self::stable_token_beneficiary_account(),
				reward,
			)?;

			<StableStakingPoolEpochReward<T>>::try_mutate(
				&pool_id,
				&epoch,
				|maybe_reward| -> DispatchResult {
					ensure!(maybe_reward.is_none(), Error::<T>::RewardAlreadyExisted);

					*maybe_reward = Some(StableRewardInfo { epoch, reward_amount: actual_reward });
					Self::deposit_event(Event::<T>::RewardUpdated {
						pool_id: pool_id.clone(),
						epoch,
						amount: actual_reward,
					});
					Ok(())
				},
			)?;

			<StableStakingPoolReward<T>>::try_mutate(&pool_id, |maybe_reward| -> DispatchResult {
				*maybe_reward = *maybe_reward + actual_reward;
				Ok(())
			})?;
			Ok(())
		}

		// Participate Staking Pool
		#[pallet::call_index(3)]
		#[pallet::weight({1000})]
		#[transactional]
		pub fn stake(
			origin: OriginFor<T>,
			pool_id: T::PoolId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let source = ensure_signed(origin)?;
			let current_block = frame_system::Pallet::<T>::block_number();
			let setting =
				<StakingPoolSetting<T>>::get(pool_id.clone()).ok_or(Error::<T>::PoolNotExisted)?;
			// Pool started and not closed soon
			let end_time = setting.end_time().ok_or(ArithmeticError::Overflow)?;
			ensure!(setting.start_time < current_block, Error::<T>::PoolNotStarted);

			// Try get the beginning block of latest incoming valid epoch for pending staking
			let effective_epoch = Self::get_epoch_index(
				pool_id.clone(),
				current_block
					.checked_add(&setting.setup_time)
					.ok_or(ArithmeticError::Overflow)?,
			)?
			.checked_add(One::one())
			.ok_or(ArithmeticError::Overflow)?;
			let effective_time = Self::get_epoch_begin_time(pool_id.clone(), effective_epoch)?;
			ensure!(end_time > effective_time, Error::<T>::PoolAlreadyEnded);

			// Insert into pending storage waiting for hook to trigger
			<PendingSetup<T>>::mutate(|maybe_order| {
				let order = StakingInfoWithOwner {
					who: source.clone(),
					pool_id: pool_id.clone(),
					staking_info: StakingInfo {
						effective_time,
						amount,
						last_add_time: effective_time,
					},
				};
				maybe_order.push_back(order);
				// Make sure the first element has earlies effective time
				maybe_order.make_contiguous().sort_by(|a, b| {
					a.staking_info.effective_time.cmp(&b.staking_info.effective_time)
				});
			});
			// Native staking effect immediately
			Self::do_native_add(source.clone(), amount, current_block)?;
			let asset_id = <AIUSDAssetId<T>>::get().ok_or(Error::<T>::NoAssetId)?;
			T::Fungibles::transfer(
				asset_id,
				&source,
				&Self::stable_token_beneficiary_account(),
				amount,
				Preservation::Expendable,
			)?;
			Self::deposit_event(Event::<T>::Staked {
				who: source,
				pool_id,
				target_effective_time: effective_time,
				amount,
			});
			Ok(())
		}

		// In case the hook for pending staking is skipped
		#[pallet::call_index(4)]
		#[pallet::weight({1000})]
		#[transactional]
		pub fn solve_pending_stake(origin: OriginFor<T>) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			let current_block = frame_system::Pallet::<T>::block_number();
			// Deposit event inner
			Self::solve_pending(current_block)?;
			Ok(())
		}

		// Claim native token reward
		#[pallet::call_index(5)]
		#[pallet::weight({1000})]
		#[transactional]
		pub fn claim_native(origin: OriginFor<T>, until_time: BlockNumberFor<T>) -> DispatchResult {
			let source = ensure_signed(origin)?;
			Self::do_native_claim(source, until_time)
		}

		// Claim stable token reward
		#[pallet::call_index(6)]
		#[pallet::weight({1000})]
		#[transactional]
		pub fn claim_stable(
			origin: OriginFor<T>,
			pool_id: T::PoolId,
			until_time: BlockNumberFor<T>,
		) -> DispatchResult {
			let source = ensure_signed(origin)?;
			Self::do_stable_claim(source, pool_id, until_time)
		}

		// Withdraw AIUSD along with reward if any
		#[pallet::call_index(7)]
		#[pallet::weight({1000})]
		#[transactional]
		pub fn withdraw(origin: OriginFor<T>, pool_id: T::PoolId) -> DispatchResult {
			let source = ensure_signed(origin)?;
			let setting =
				<StakingPoolSetting<T>>::get(pool_id.clone()).ok_or(Error::<T>::PoolNotExisted)?;
			// Pool closed
			let current_block = frame_system::Pallet::<T>::block_number();
			let end_time = setting.end_time().ok_or(ArithmeticError::Overflow)?;
			ensure!(end_time < current_block, Error::<T>::PoolNotEnded);
			// Claim reward
			Self::do_native_claim(source.clone(), current_block.clone())?;
			Self::do_stable_claim(source.clone(), pool_id.clone(), current_block)?;
			// Withdraw and clean/modify all storage
			Self::do_withdraw(source, pool_id)
		}

		// Registing AIUSD asset id
		#[pallet::call_index(8)]
		#[pallet::weight({1000})]
		#[transactional]
		pub fn regist_aiusd(
			origin: OriginFor<T>,
			asset_id: <T::Fungibles as FsInspect<T::AccountId>>::AssetId,
		) -> DispatchResult {
			T::StakingPoolCommitteeOrigin::ensure_origin(origin)?;
			<AIUSDAssetId<T>>::put(asset_id.clone());
			Self::deposit_event(Event::<T>::AIUSDRegisted { asset_id });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		// return setting.epoch if time >= pool end_time
		fn get_epoch_index(
			pool_id: T::PoolId,
			time: BlockNumberFor<T>,
		) -> Result<u128, sp_runtime::DispatchError> {
			let setting =
				<StakingPoolSetting<T>>::get(pool_id).ok_or(Error::<T>::PoolNotExisted)?;
			// If start_time > time, means epoch 0
			let index_bn = time
				.saturating_sub(setting.start_time)
				.checked_div(&setting.epoch_range)
				.ok_or(ArithmeticError::Overflow)?;
			let index: u128 = index_bn.try_into().or(Err(ArithmeticError::Overflow))?;
			if index >= setting.epoch {
				return Ok(setting.epoch);
			} else {
				return Ok(index);
			}
		}

		// return pool ending time if epoch >= setting.epoch
		fn get_epoch_begin_time(
			pool_id: T::PoolId,
			epoch: u128,
		) -> Result<BlockNumberFor<T>, sp_runtime::DispatchError> {
			let setting =
				<StakingPoolSetting<T>>::get(pool_id).ok_or(Error::<T>::PoolNotExisted)?;
			// If epoch larger than setting
			if epoch >= setting.epoch {
				return Ok(setting.end_time().ok_or(ArithmeticError::Overflow)?);
			}
			let epoch_bn: BlockNumberFor<T> =
				epoch.try_into().or(Err(ArithmeticError::Overflow))?;
			let result = setting
				.start_time
				.checked_add(
					&setting.epoch_range.checked_mul(&epoch_bn).ok_or(ArithmeticError::Overflow)?,
				)
				.ok_or(ArithmeticError::Overflow)?;
			return Ok(result)
		}

		// For native_staking
		fn do_native_add(
			who: T::AccountId,
			amount: BalanceOf<T>,
			effective_time: BlockNumberFor<T>,
		) -> DispatchResult {
			<NativeCheckpoint<T>>::try_mutate(|maybe_checkpoint| {
				if let Some(checkpoint) = maybe_checkpoint {
					checkpoint.add(effective_time, amount).ok_or(ArithmeticError::Overflow)?;
				} else {
					*maybe_checkpoint =
						Some(StakingInfo { effective_time, amount, last_add_time: effective_time });
				}
				Ok::<(), DispatchError>(())
			})?;
			<UserNativeCheckpoint<T>>::try_mutate(&who, |maybe_checkpoint| {
				if let Some(checkpoint) = maybe_checkpoint {
					checkpoint.add(effective_time, amount).ok_or(ArithmeticError::Overflow)?;
				} else {
					*maybe_checkpoint =
						Some(StakingInfo { effective_time, amount, last_add_time: effective_time });
				}
				Ok::<(), DispatchError>(())
			})?;
			Ok(())
		}

		// For stable_staking
		fn do_stable_add(
			who: T::AccountId,
			pool_id: T::PoolId,
			amount: BalanceOf<T>,
			effective_time: BlockNumberFor<T>,
		) -> DispatchResult {
			<StableStakingPoolCheckpoint<T>>::try_mutate(&pool_id, |maybe_checkpoint| {
				if let Some(checkpoint) = maybe_checkpoint {
					checkpoint.add(effective_time, amount).ok_or(ArithmeticError::Overflow)?;
				} else {
					*maybe_checkpoint =
						Some(StakingInfo { effective_time, amount, last_add_time: effective_time });
				}
				Ok::<(), DispatchError>(())
			})?;
			<UserStableStakingPoolCheckpoint<T>>::try_mutate(&who, &pool_id, |maybe_checkpoint| {
				if let Some(checkpoint) = maybe_checkpoint {
					checkpoint.add(effective_time, amount).ok_or(ArithmeticError::Overflow)?;
				} else {
					*maybe_checkpoint =
						Some(StakingInfo { effective_time, amount, last_add_time: effective_time });
				}
				Ok::<(), DispatchError>(())
			})?;
			Ok(())
		}

		fn do_native_claim(who: T::AccountId, until_time: BlockNumberFor<T>) -> DispatchResult {
			let beneficiary_account: T::AccountId = Self::native_token_beneficiary_account();
			let current_block = frame_system::Pallet::<T>::block_number();
			ensure!(until_time <= current_block, Error::<T>::CannotClaimFuture);
			// NativeBalanceOf
			let reward_pool = T::Fungible::balance(&beneficiary_account);

			if let Some(mut ncp) = <NativeCheckpoint<T>>::get() {
				if let Some(mut user_ncp) = <UserNativeCheckpoint<T>>::get(who.clone()) {
					// get weight and update stake info
					let user_claimed_weight =
						user_ncp.claim(until_time).ok_or(ArithmeticError::Overflow)?;
					let proportion = Perquintill::from_rational(
						user_claimed_weight,
						ncp.weight_force(until_time).ok_or(ArithmeticError::Overflow)?,
					);
					// Do not care what new Synthetic effective_time of staking pool
					let _ = ncp
						.claim_based_on_weight(user_claimed_weight)
						.ok_or(ArithmeticError::Overflow)?;

					let reward_pool_u128: u128 =
						reward_pool.try_into().or(Err(ArithmeticError::Overflow))?;
					let distributed_reward_u128: u128 = proportion * reward_pool_u128;
					let distributed_reward: NativeBalanceOf<T> =
						distributed_reward_u128.try_into().or(Err(ArithmeticError::Overflow))?;
					T::Fungible::transfer(
						&beneficiary_account,
						&who,
						distributed_reward,
						Preservation::Expendable,
					)?;
					// Adjust checkpoint
					<NativeCheckpoint<T>>::put(ncp);
					<UserNativeCheckpoint<T>>::insert(&who, user_ncp);
					Self::deposit_event(Event::<T>::NativeRewardClaimed {
						who,
						until_time,
						amount: distributed_reward,
					});
				}
			}
			Ok(())
		}

		fn do_stable_claim(
			who: T::AccountId,
			pool_id: T::PoolId,
			until_time: BlockNumberFor<T>,
		) -> DispatchResult {
			let current_block = frame_system::Pallet::<T>::block_number();
			ensure!(until_time <= current_block, Error::<T>::CannotClaimFuture);
			// BalanceOf
			let reward_pool = <StableStakingPoolReward<T>>::get(pool_id.clone());
			let asset_id = <AIUSDAssetId<T>>::get().ok_or(Error::<T>::NoAssetId)?;

			if let Some(mut scp) = <StableStakingPoolCheckpoint<T>>::get(pool_id.clone()) {
				if let Some(mut user_scp) =
					<UserStableStakingPoolCheckpoint<T>>::get(who.clone(), pool_id.clone())
				{
					// get weight and update stake info
					let user_claimed_weight =
						user_scp.claim(until_time).ok_or(ArithmeticError::Overflow)?;
					let proportion = Perquintill::from_rational(
						user_claimed_weight,
						scp.weight_force(until_time).ok_or(ArithmeticError::Overflow)?,
					);
					// Do not care what new Synthetic effective_time of staking pool
					let _ = scp
						.claim_based_on_weight(user_claimed_weight)
						.ok_or(ArithmeticError::Overflow)?;

					let reward_pool_u128: u128 =
						reward_pool.try_into().or(Err(ArithmeticError::Overflow))?;
					let distributed_reward_u128: u128 = proportion * reward_pool_u128;
					let distributed_reward: BalanceOf<T> =
						distributed_reward_u128.try_into().or(Err(ArithmeticError::Overflow))?;
					T::Fungibles::transfer(
						asset_id,
						&Self::stable_token_beneficiary_account(),
						&who,
						distributed_reward,
						Preservation::Expendable,
					)?;
					// Adjust checkpoint and reward storage
					<StableStakingPoolReward<T>>::insert(
						&pool_id,
						reward_pool - distributed_reward,
					);
					<StableStakingPoolCheckpoint<T>>::insert(&pool_id, scp);
					<UserStableStakingPoolCheckpoint<T>>::insert(&who, &pool_id, user_scp);
					Self::deposit_event(Event::<T>::StableRewardClaimed {
						who,
						pool_id,
						until_time,
						amount: distributed_reward,
					});
				}
			}
			Ok(())
		}

		fn do_withdraw(who: T::AccountId, pool_id: T::PoolId) -> DispatchResult {
			let current_block = frame_system::Pallet::<T>::block_number();
			let asset_id = <AIUSDAssetId<T>>::get().ok_or(Error::<T>::NoAssetId)?;
			if let Some(mut scp) = <StableStakingPoolCheckpoint<T>>::get(pool_id.clone()) {
				if let Some(user_scp) =
					<UserStableStakingPoolCheckpoint<T>>::get(who.clone(), pool_id.clone())
				{
					// Return notion
					T::Fungibles::transfer(
						asset_id,
						&Self::stable_token_beneficiary_account(),
						&who,
						user_scp.amount,
						Preservation::Expendable,
					)?;
					// Correct global stable staking pool
					scp.withdraw(user_scp.amount).ok_or(ArithmeticError::Overflow)?;
					<StableStakingPoolCheckpoint<T>>::insert(&pool_id, scp);
					// Correct global native staking pool
					// stable token balance type
					let user_scp_amount_sb: BalanceOf<T> =
						user_scp.amount.try_into().or(Err(ArithmeticError::Overflow))?;
					if let Some(mut ncp) = <NativeCheckpoint<T>>::get() {
						ncp.withdraw(user_scp_amount_sb).ok_or(ArithmeticError::Overflow)?;
						<NativeCheckpoint<T>>::put(ncp);
					}
					// Clean user stable staking storage
					<UserStableStakingPoolCheckpoint<T>>::remove(who.clone(), pool_id.clone());
					// Clean user native staking storage if zero, modify otherwise
					if let Some(mut user_ncp) = <UserNativeCheckpoint<T>>::get(who.clone()) {
						if user_ncp.amount == user_scp_amount_sb {
							<UserNativeCheckpoint<T>>::remove(who.clone());
						} else {
							user_ncp
								.withdraw(user_scp_amount_sb)
								.ok_or(ArithmeticError::Overflow)?;
							<UserNativeCheckpoint<T>>::insert(who.clone(), user_ncp);
						}
					}

					Self::deposit_event(Event::<T>::Withdraw {
						who,
						pool_id,
						time: current_block,
						amount: user_scp.amount,
					});
				}
			}
			Ok(())
		}

		fn solve_pending(n: BlockNumberFor<T>) -> DispatchResult {
			let mut pending_setup = <PendingSetup<T>>::take();
			loop {
				match pending_setup.pop_front() {
					// Latest Pending tx effective
					Some(x) if x.staking_info.effective_time <= n => {
						Self::do_stable_add(
							x.who.clone(),
							x.pool_id.clone(),
							x.staking_info.amount,
							n,
						)?;
						Self::deposit_event(Event::<T>::PendingStakingSolved {
							who: x.who,
							pool_id: x.pool_id,
							effective_time: n,
							amount: x.staking_info.amount,
						});
					},
					// Latest Pending tx not effective
					Some(x) => {
						pending_setup.push_front(x);
						break;
					},
					// No pending tx
					_ => {
						break;
					},
				}
			}
			<PendingSetup<T>>::put(pending_setup);
			Ok(())
		}

		pub fn native_token_beneficiary_account() -> T::AccountId {
			T::NativeTokenBeneficiaryId::get().into_account_truncating()
		}

		pub fn stable_token_beneficiary_account() -> T::AccountId {
			T::StableTokenBeneficiaryId::get().into_account_truncating()
		}
	}
}
