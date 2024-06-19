#![cfg_attr(not(feature = "std"), no_std)]

use fp_evm::{PrecompileFailure, PrecompileHandle};

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_evm::AddressMapping;
use precompile_utils::prelude::*;
use sp_runtime::traits::Dispatchable;

use sp_core::{H160, H256, U256};
use sp_std::marker::PhantomData;

use pallet_stable_staking::BalanceOf;

pub type PoolId<Runtime> = <Runtime as pallet_stable_staking::Config>::PoolId;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_stable_staking::{
	NativeCheckpoint, PendingAmount, StableStakingPoolCheckpoint, StableStakingPoolEpochReward,
	StableStakingPoolReward, StakingPoolSetting, UserNativeCheckpoint,
	UserStableStakingPoolCheckpoint,
};

/// @notice Describes pool setting.
/// @param startTime: The start time of pool.
/// @param epoch: The number of epoch the pool will last.
/// @param epochRange: The number of block each epoch consist.
/// @param setupTime: The least setup time before stable staking become effective.
/// @param poolCap: The maximum staked amount pool allowed.
/// Helper struct used to encode PoolSetting.
#[derive(Debug, Clone, solidity::Codec)]
pub(crate) struct PrecompilePoolSetting {
	start_time: U256,
	epoch: U256,
	epoch_range: U256,
	setup_time: U256,
	pool_cap: U256,
}

/// @notice Describes an acutal/synthetic staking position.
/// @param effectiveTime: The average amount weight time of staking become effective.
/// @param amount: Total staked amount.
/// @param lastAddTime: The effective time of latest staking more command.
/// Helper struct used to encode StakingInfo.
#[derive(Debug, Clone, solidity::Codec)]
pub(crate) struct PrecompileStakingInfo {
	effective_time: U256,
	amount: U256,
	last_add_time: U256,
}

pub struct StableStakingPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> StableStakingPrecompile<Runtime>
where
	Runtime: pallet_stable_staking::Config + pallet_evm::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<pallet_stable_staking::Call<Runtime>>,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::AccountId: Into<H160>,
	BalanceOf<Runtime>: TryFrom<U256>,
	Runtime::PoolId: TryFrom<U256>,
	BlockNumberFor<Runtime>: TryFrom<U256>,
{
	#[precompile::public("stake(uint256,uint256)")]
	fn stake(handle: &mut impl PrecompileHandle, pool: U256, amount: U256) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let pool_id: PoolId<Runtime> = pool.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("pool index type"))
		})?;
		let amount: BalanceOf<Runtime> = amount.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("balance type"))
		})?;

		let call = pallet_stable_staking::Call::<Runtime>::stake { pool_id, amount };
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("solvePendingStake()")]
	fn solve_pending_stake(handle: &mut impl PrecompileHandle) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let call = pallet_stable_staking::Call::<Runtime>::solve_pending_stake {};
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;
		Ok(())
	}

	#[precompile::public("claimNative(uint256)")]
	fn claim_native(handle: &mut impl PrecompileHandle, until_time: U256) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let until_time: BlockNumberFor<Runtime> = until_time.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("block number type"))
		})?;
		let call = pallet_stable_staking::Call::<Runtime>::claim_native { until_time };
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;
		Ok(())
	}

	#[precompile::public("claimStable(uint256,uint256)")]
	fn claim_stable(handle: &mut impl PrecompileHandle, pool: U256, until_time: U256) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let pool_id: PoolId<Runtime> = pool.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("pool index type"))
		})?;
		let until_time: BlockNumberFor<Runtime> = until_time.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("block number type"))
		})?;
		let call = pallet_stable_staking::Call::<Runtime>::claim_stable { pool_id, until_time };
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;
		Ok(())
	}

	#[precompile::public("withdraw(uint256)")]
	fn withdraw(handle: &mut impl PrecompileHandle, pool: U256) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let pool_id: PoolId<Runtime> = pool.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("pool index type"))
		})?;
		let call = pallet_stable_staking::Call::<Runtime>::withdraw { pool_id };
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;
		Ok(())
	}

	#[precompile::public("stakingPoolSetting(uint256)")]
	#[precompile::view]
	fn staking_pool_setting(
		handle: &mut impl PrecompileHandle,
		pool: U256,
	) -> EvmResult<PrecompilePoolSetting> {
		// Storage item: StakingPoolSetting:
		// Twox64(8) + 16+ 16 * 2 + 4 * 3 = 68
		handle.record_db_read::<Runtime>(68)?;

		let pool_id: PoolId<Runtime> = pool.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("pool index type"))
		})?;
		let pool_setting = StakingPoolSetting::<Runtime>::get(pool_id);

		Ok(PrecompilePoolSetting {
			start_time: pool_setting.start_time.into(),
			epoch: pool_setting.start_time.into(),
			epoch_range: pool_setting.epoch_range.into(),
			setup_time: pool_setting.setup_time.into(),
			pool_cap: pool_setting.pool_cap.into(),
		})
	}

	#[precompile::public("stableStakingPoolReward(uint256)")]
	#[precompile::view]
	fn stable_staking_pool_rReward(
		handle: &mut impl PrecompileHandle,
		pool: U256,
	) -> EvmResult<U256> {
		// Storage item: StableStakingPoolReward:
		// Twox64(8) + 16 + 16 = 40
		handle.record_db_read::<Runtime>(40)?;

		let pool_id: PoolId<Runtime> = pool.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("pool index type"))
		})?;
		let reward = StableStakingPoolReward::<Runtime>::get(pool_id);
		Ok(reward.into())
	}

	#[precompile::public("stableStakingPoolEpochReward(uint256,uint256)")]
	#[precompile::view]
	fn stable_staking_pool_epoch_reward(
		handle: &mut impl PrecompileHandle,
		pool: U256,
		epoch: U256,
	) -> EvmResult<U256> {
		// Storage item: StableStakingPoolEpochReward:
		// Twox64(8) * 2 + 16 + 16 + 16 * 2 = 80
		handle.record_db_read::<Runtime>(80)?;

		let pool_id: PoolId<Runtime> = pool.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("pool index type"))
		})?;
		let epoch: u128 = epoch.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("epoch index type"))
		})?;
		let reward = StableStakingPoolEpochReward::<Runtime>::get(pool_id, epoch);
		Ok(reward.into())
	}

	#[precompile::public("stableStakingPoolCheckpoint(uint256)")]
	#[precompile::view]
	fn stable_staking_pool_checkpoint(
		handle: &mut impl PrecompileHandle,
		pool: U256,
	) -> EvmResult<PrecompileStakingInfo> {
		// Storage item: StableStakingPoolCheckpoint:
		// Twox64(8) + 16 + 16 + 4 * 2 = 48
		handle.record_db_read::<Runtime>(48)?;

		let pool_id: PoolId<Runtime> = pool.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("pool index type"))
		})?;
		let staking_info = StableStakingPoolCheckpoint::<Runtime>::get(pool_id);

		Ok(PrecompileStakingInfo {
			effective_time: staking_info.effective_time.into(),
			amount: staking_info.amount.into(),
			last_add_time: staking_info.last_add_time.into(),
		})
	}

	#[precompile::public("userStableStakingPoolCheckpoint(address,uint256)")]
	#[precompile::view]
	fn user_stable_staking_pool_checkpoint_evm(
		handle: &mut impl PrecompileHandle,
		user: Address,
		pool: U256,
	) -> EvmResult<PrecompileStakingInfo> {
		// Storage item: UserStableStakingPoolCheckpoint:
		// Twox64(8) * 2 + 32 + 16 + 16 + 4 * 2 = 88
		handle.record_db_read::<Runtime>(88)?;

		let user = Runtime::AddressMapping::into_account_id(user.into());
		let pool_id: PoolId<Runtime> = pool.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("pool index type"))
		})?;
		let staking_info = UserStableStakingPoolCheckpoint::<Runtime>::get(user, pool_id);

		Ok(PrecompileStakingInfo {
			effective_time: staking_info.effective_time.into(),
			amount: staking_info.amount.into(),
			last_add_time: staking_info.last_add_time.into(),
		})
	}

	#[precompile::public("userStableStakingPoolCheckpoint(bytes32,uint256)")]
	#[precompile::view]
	fn user_stable_staking_pool_checkpoint_sub(
		handle: &mut impl PrecompileHandle,
		user: H256,
		pool: U256,
	) -> EvmResult<PrecompileStakingInfo> {
		// Storage item: UserStableStakingPoolCheckpoint:
		// Twox64(8) * 2 + 32 + 16 + 16 + 4 * 2 = 88
		handle.record_db_read::<Runtime>(48)?;

		let user = user.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("address type"))
		})?;
		let pool_id: PoolId<Runtime> = pool.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("pool index type"))
		})?;
		let staking_info = UserStableStakingPoolCheckpoint::<Runtime>::get(user, pool_id);

		Ok(PrecompileStakingInfo {
			effective_time: staking_info.effective_time.into(),
			amount: staking_info.amount.into(),
			last_add_time: staking_info.last_add_time.into(),
		})
	}

	#[precompile::public("nativeCheckpoint()")]
	#[precompile::view]
	fn native_checkpoint(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileStakingInfo> {
		// Storage item: UserStableStakingPoolCheckpoint:
		// 16 + 4 * 2 = 24
		handle.record_db_read::<Runtime>(24)?;

		let staking_info = NativeCheckpoint::<Runtime>::get();

		Ok(PrecompileStakingInfo {
			effective_time: staking_info.effective_time.into(),
			amount: staking_info.amount.into(),
			last_add_time: staking_info.last_add_time.into(),
		})
	}

	#[precompile::public("userNativeCheckpoint(address)")]
	#[precompile::view]
	fn user_native_checkpoint_evm(
		handle: &mut impl PrecompileHandle,
		user: Address,
	) -> EvmResult<PrecompileStakingInfo> {
		// Storage item: UserStableStakingPoolCheckpoint:
		// Twox64(8) + 32 + 16 + 4 * 2 = 64
		handle.record_db_read::<Runtime>(64)?;

		let user = Runtime::AddressMapping::into_account_id(user.into());
		let staking_info = UserNativeCheckpoint::<Runtime>::get(user);

		Ok(PrecompileStakingInfo {
			effective_time: staking_info.effective_time.into(),
			amount: staking_info.amount.into(),
			last_add_time: staking_info.last_add_time.into(),
		})
	}

	#[precompile::public("userNativeCheckpoint(bytes32)")]
	#[precompile::view]
	fn user_native_checkpoint_sub(
		handle: &mut impl PrecompileHandle,
		user: H256,
	) -> EvmResult<PrecompileStakingInfo> {
		// Storage item: UserStableStakingPoolCheckpoint:
		// Twox64(8) + 32 + 16 + 4 * 2 = 64
		handle.record_db_read::<Runtime>(64)?;

		let user = user.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("address type"))
		})?;
		let staking_info = UserNativeCheckpoint::<Runtime>::get(user);

		Ok(PrecompileStakingInfo {
			effective_time: staking_info.effective_time.into(),
			amount: staking_info.amount.into(),
			last_add_time: staking_info.last_add_time.into(),
		})
	}

	#[precompile::public("pendingAmount(uint256)")]
	#[precompile::view]
	fn pending_amount(handle: &mut impl PrecompileHandle, pool: U256) -> EvmResult<U256> {
		// Storage item: PendingAmount:
		// Twox64(8) + 16 + 16 = 40
		handle.record_db_read::<Runtime>(40)?;

		let pool_id: PoolId<Runtime> = pool.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("pool index type"))
		})?;
		let amount = PendingAmount::<Runtime>::get(pool_id);

		Ok(amount.into())
	}
}
