#![cfg_attr(not(feature = "std"), no_std)]

use fp_evm::PrecompileHandle;

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_evm::AddressMapping;
use precompile_utils::prelude::*;
use sp_runtime::traits::Dispatchable;

use sp_core::{H160, H256, U256};
use sp_std::marker::PhantomData;

/// Alias for the Balance type for the provided Runtime and Instance.
pub type NativeBalanceOf<Runtime, Instance = ()> =
	<Runtime as pallet_balances::Config<Instance>>::Balance;
/// Alias for the Balance type for the provided Runtime and Instance.
pub type BalanceOf<Runtime, Instance = ()> = <Runtime as pallet_assets::Config<Instance>>::Balance;
pub type PoolId<Runtime> = <Runtime as pallet_stable_staking::Config>::PoolId;
use frame_system::pallet_prelude::BlockNumberFor;

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

pub struct PalletStableStakingPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> PalletStableStakingPrecompile<Runtime>
where
	Runtime: pallet_stable_staking::Config + pallet_evm::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<pallet_template::Call<Runtime>>,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::AccountId: Into<H160>,
{
	#[precompile::public("stake(uint256,uint256)")]
	fn stake(handle: &mut impl PrecompileHandle, pool: U256, amount: U256) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let pool_id: PoolId<Runtime> = pool
			.try_into()
			.map_err(|_| RevertReason::value_is_too_large("balance type").into())?;
		let amount: BalanceOf<Runtime> = amount
			.try_into()
			.map_err(|_| RevertReason::value_is_too_large("balance type").into())?;

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

		let until_time: BlockNumberFor<Runtime> = until_time
			.try_into()
			.map_err(|_| RevertReason::value_is_too_large("balance type").into())?;
		let call = pallet_stable_staking::Call::<Runtime>::claim_native { until_time };
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;
		Ok(())
	}

	#[precompile::public("claimStable(uint256,uint256)")]
	fn claim_stable(handle: &mut impl PrecompileHandle, pool: U256, until_time: U256) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let until_time: BlockNumberFor<Runtime> = until_time
			.try_into()
			.map_err(|_| RevertReason::value_is_too_large("balance type").into())?;
		let call =
			pallet_stable_staking::Call::<Runtime>::claim_stable { pool_id: pool, until_time };
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;
		Ok(())
	}

	#[precompile::public("withdraw(uint256)")]
	fn withdraw(handle: &mut impl PrecompileHandle, pool: U256) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let pool_id: PoolId<Runtime> = pool
			.try_into()
			.map_err(|_| RevertReason::value_is_too_large("balance type").into())?;
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
	}

	#[precompile::public("stableStakingPoolReward(uint256)")]
	#[precompile::view]
	fn stable_staking_pool_rReward(
		handle: &mut impl PrecompileHandle,
		pool: U256,
	) -> EvmResult<U256> {
	}

	#[precompile::public("stableStakingPoolEpochReward(uint256,uint256)")]
	#[precompile::view]
	fn stable_staking_pool_epoch_reward(
		handle: &mut impl PrecompileHandle,
		pool: U256,
		epoch: U256,
	) -> EvmResult<U256> {
	}

	#[precompile::public("stableStakingPoolCheckpoint(uint256)")]
	#[precompile::view]
	fn stable_staking_pool_checkpoint(
		handle: &mut impl PrecompileHandle,
		pool: U256,
	) -> EvmResult<PrecompileStakingInfo> {
	}

	#[precompile::public("userStableStakingPoolCheckpoint(address,uint256)")]
	#[precompile::view]
	fn user_stable_staking_pool_checkpoint_evm(
		handle: &mut impl PrecompileHandle,
		user: Address,
		pool: U256,
	) -> EvmResult<PrecompileStakingInfo> {
	}

	#[precompile::public("userStableStakingPoolCheckpoint(bytes32,uint256)")]
	#[precompile::view]
	fn user_stable_staking_pool_checkpoint_sub(
		handle: &mut impl PrecompileHandle,
		user: H256,
		pool: U256,
	) -> EvmResult<PrecompileStakingInfo> {
	}

	#[precompile::public("nativeCheckpoint()")]
	#[precompile::view]
	fn native_checkpoint(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileStakingInfo> {}

	#[precompile::public("userNativeCheckpoint(address)")]
	#[precompile::view]
	fn user_native_checkpoint_evm(
		handle: &mut impl PrecompileHandle,
		user: Address,
	) -> EvmResult<PrecompileStakingInfo> {
	}

	#[precompile::public("userNativeCheckpoint(bytes32)")]
	#[precompile::view]
	fn user_native_checkpoint_sub(
		handle: &mut impl PrecompileHandle,
		user: H256,
	) -> EvmResult<PrecompileStakingInfo> {
	}

	#[precompile::public("pendingAmount(uint256)")]
	#[precompile::view]
	fn pending_amount(handle: &mut impl PrecompileHandle, pool: U256) -> EvmResult<U256> {}
}
