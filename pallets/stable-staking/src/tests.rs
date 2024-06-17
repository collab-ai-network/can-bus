use super::{
	mock::{
		assert_events, new_test_ext, Assets, Balances, HavlingMintId, RuntimeEvent, RuntimeOrigin,
		StableStaking, StakingPoolId, System, Test, ENDOWED_BALANCE, USER_A, USER_B, USER_C,
	},
	*,
};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::ArithmeticError;

fn next_block() {
	System::set_block_number(System::block_number() + 1);
	StableStaking::begin_block(System::block_number());
}

fn fast_forward_to(n: u64) {
	while System::block_number() < n {
		next_block();
	}
}

#[test]
fn can_not_create_pool_already_started_or_existed() {
	new_test_ext().execute_with(|| {
		fast_forward_to(101);
		// Create stable staking pool
		let pool_setup: PoolSetting<u64, u64> = PoolSetting {
			start_time: 100u64,
			epoch: 10u128,
			epoch_range: 100u64,
			setup_time: 200u64,
			pool_cap: 1_000_000_000u64,
		};
		assert_noop!(
			StableStaking::create_staking_pool(RuntimeOrigin::root(), 1u128, pool_setup.clone()),
			Error::<Test>::PoolAlreadyExisted
		);
		// Transfer and check result
		fast_forward_to(101);
		assert_noop!(
			StableStaking::create_staking_pool(RuntimeOrigin::root(), 2u128, pool_setup),
			Error::<Test>::PoolAlreadyStarted
		);
	})
}

// TODO: update_metadata test
// Currently metadata does nothing but description

#[test]
fn update_reward_successful_and_failed() {
	new_test_ext().execute_with(|| {
		// update epoch 0 reward with amount of 2000
		assert_ok!(StableStaking::update_reward(RuntimeOrigin::root(), 1u128, 0u128, 2000u64));
		assert_events(vec![RuntimeEvent::StableStaking(Event::RewardUpdated {
			pool_id: 1u128,
			epoch: 0u128,
			amount: 2000u64,
		})]);
		// Staking pool reward storage efffective
		assert_eq!(StableStaking::stable_staking_pool_reward(1u128), 2000u64);
		assert_eq!(
			StableStaking::stable_staking_pool_epoch_reward(1u128, 0u128),
			Some(StableRewardInfo { epoch: 0u128, reward_amount: 2000u64 })
		);
		assert_eq!(StableStaking::stable_staking_pool_epoch_reward(1u128, 1u128), None);
		// Staking pool balance effective
		let native_token_pool: u64 = HavlingMintId::get().into_account_truncating();
		assert_eq!(Balances::free_balance(native_token_pool), ENDOWED_BALANCE + 2000u64);

		// Can not update epoch reward twice
		assert_noop!(
			StableStaking::update_reward(RuntimeOrigin::root(), 1u128, 0u128, 1000u64),
			Error::<Test>::RewardAlreadyExisted
		);

		// Pool started at 100, epoch range = 100, epoch = 10
		// So Blocknumber 301 => Epoch 2 started/Epoch 1 ended
		System::set_block_number(301u64);
		// Can not update epoch already ended
		assert_noop!(
			StableStaking::update_reward(RuntimeOrigin::root(), 1u128, 1u128, 1000u64),
			Error::<Test>::EpochAlreadyEnded
		);

		// Epoch reward can not be updated to non-existing pool
		assert_noop!(
			StableStaking::update_reward(RuntimeOrigin::root(), 9999u128, 1u128, 1000u64),
			Error::<Test>::PoolNotExisted
		);
		// Epoch reward can not be updated to epoch index not existing
		assert_noop!(
			StableStaking::update_reward(RuntimeOrigin::root(), 1u128, 11u128, 1000u64),
			Error::<Test>::EpochNotExisted
		);

		// Epoch reward update for "last epoch" (pool end time's next epoch) always success
		// Pool epoch = 10
		System::set_block_number(9999999u64);
		assert_ok!(StableStaking::update_reward(RuntimeOrigin::root(), 1u128, 10u128, 2000u64));
		assert_events(vec![RuntimeEvent::StableStaking(Event::RewardUpdated {
			pool_id: 1u128,
			epoch: 10u128,
			amount: 2000u64,
		})]);

		// Can not update reward if no AIUSD registed
		System::set_block_number(301u64);
		<AIUSDAssetId<Test>>::kill();
		assert_noop!(
			StableStaking::update_reward(RuntimeOrigin::root(), 1u128, 5u128, 2000u64),
			Error::<Test>::NoAssetId
		);
	})
}

#[test]
fn stake_successful_and_failed() {
	new_test_ext().execute_with(|| {
		let native_token_pool: u64 = HavlingMintId::get().into_account_truncating();

		// Can not stake non-exist pool
		assert_noop!(
			StableStaking::stake(RuntimeOrigin::signed(USER_A), 2u128, 2000u64),
			Error::<Test>::PoolNotExisted
		);

		// Can not stake non-started pool
		assert_noop!(
			StableStaking::stake(RuntimeOrigin::signed(USER_A), 1u128, 2000u64),
			Error::<Test>::PoolNotStarted
		);

		// Can not stake ended pool
		System::set_block_number(9999999u64);
		assert_noop!(
			StableStaking::stake(RuntimeOrigin::signed(USER_A), 1u128, 2000u64),
			Error::<Test>::PoolAlreadyEnded
		);

		// Success, check user/global native checkpoint storage
		// check pending set up storage
		System::set_block_number(301u64);
		assert_ok!(StableStaking::stake(RuntimeOrigin::signed(USER_A), 1u128, 2000u64));
		assert_eq!(Assets::balance(1u32, native_token_pool), ENDOWED_BALANCE + 2000u64);
		let global_staking_info = StableStaking::native_checkpoint().unwrap();
		assert_eq!(
			global_staking_info,
			StakingInfo { effective_time: 301, amount: 2000u64, last_add_time: 301 }
		);
		let user_a_staking_info = StableStaking::user_native_checkpoint(USER_A).unwrap();
		assert_eq!(
			user_a_staking_info,
			StakingInfo { effective_time: 301, amount: 2000u64, last_add_time: 301 }
		);
		let pending_set_up = StableStaking::pending_setup();
		assert_eq!(pending_set_up.len(), 1);
		let pending_set_up_element = pending_set_up.get(0).unwrap();
		// Pool set up time = 200
		// So user enter at 301 need to wait till 600 to make it effective and receiving Stable
		// staking reward
		assert_eq!(
			pending_set_up_element,
			StakingInfoWithOwner {
				who: USER_A,
				pool_id: 1u128,
				staking_info: StakingInfo {
					effective_time: 600,
					amount: 2000u64,
					last_add_time: 600
				}
			}
		);

		// Second user B stake
		fast_forward_to(311u64);
		assert_ok!(StableStaking::stake(RuntimeOrigin::signed(USER_B), 1u128, 1000u64));
		let global_staking_info = StableStaking::native_checkpoint().unwrap();
		// Synthetic (301, 2000), (311, 1000) = (304.3333, 3000)
		assert_eq!(
			global_staking_info,
			StakingInfo { effective_time: 304, amount: 3000u64, last_add_time: 311 }
		);
		// user a unchanged
		let user_a_staking_info = StableStaking::user_native_checkpoint(USER_A).unwrap();
		assert_eq!(
			user_a_staking_info,
			StakingInfo { effective_time: 301, amount: 2000u64, last_add_time: 301 }
		);
		// user b
		let user_b_staking_info = StableStaking::user_native_checkpoint(USER_B).unwrap();
		assert_eq!(
			user_b_staking_info,
			StakingInfo { effective_time: 311, amount: 1000u64, last_add_time: 311 }
		);
		// Pending set up storage change
		let pending_set_up = StableStaking::pending_setup();
		assert_eq!(pending_set_up.len(), 2);
		// Pool set up time = 200
		// So user enter at 311 need to wait till 600 to make it effective and receiving Stable
		// staking reward
		assert_eq!(
			pending_set_up_element,
			StakingInfoWithOwner {
				who: USER_B,
				pool_id: 1u128,
				staking_info: StakingInfo {
					effective_time: 600,
					amount: 1000u64,
					last_add_time: 600
				}
			}
		);

		// Can not stake if no AIUSD registed
		<AIUSDAssetId<Test>>::kill();
		assert_noop!(
			StableStaking::stake(RuntimeOrigin::signed(USER_C), 1u128, 3000u64),
			Error::<Test>::NoAssetId
		);
	})
}

#[test]
fn solve_pending_stake_and_hook_works() {
	new_test_ext().execute_with(|| {
		// Success, check user/global native checkpoint storage
		// check pending set up storage
		System::set_block_number(301u64);
		assert_ok!(StableStaking::stake(RuntimeOrigin::signed(USER_A), 1u128, 2000u64));
		// Pool set up time = 200
		// So user enter at 301 need to wait till 600 to make it effective and receiving Stable
		System::set_block_number(590u64);
		// Try trigger hook
		fast_forward_to(610u64);
		// No more pending
		let pending_set_up = StableStaking::pending_setup();
		assert_eq!(pending_set_up.len(), 0);
		// Check stable staking checkpoint
		let global_staking_info = StableStaking::stable_staking_pool_checkpoint(pool_id).unwrap();
		assert_eq!(
			global_staking_info,
			StakingInfo { effective_time: 600, amount: 2000u64, last_add_time: 600 }
		);
		let user_a_staking_info = UserStableStakingPoolCheckpoint(USER_A, 1u128).unwrap();
		assert_eq!(
			user_a_staking_info,
			StakingInfo { effective_time: 600, amount: 2000u64, last_add_time: 600 }
		);

		// Second user B stake
		assert_ok!(StableStaking::stake(RuntimeOrigin::signed(USER_B), 1u128, 1000u64));
		// Any one can trigger manual, but right now no effect
		let pending_set_up = StableStaking::pending_setup();
		assert_eq!(pending_set_up.len(), 1);
		assert_ok!(StableStaking::solve_pending_stake(RuntimeOrigin::signed(USER_C)));
		let pending_set_up = StableStaking::pending_setup();
		assert_eq!(pending_set_up.len(), 1);

		// Pool set up time = 200, current block = 610
		// So user enter at 301 need to wait till 900 to make it effective and receiving Stable
		// set block number without triggering hook
		System::set_block_number(910u64);
		// Global staking no changed
		let global_staking_info = StableStaking::stable_staking_pool_checkpoint(pool_id).unwrap();
		assert_eq!(
			global_staking_info,
			StakingInfo { effective_time: 600, amount: 2000u64, last_add_time: 600 }
		);
		// User b staking is still none
		assert!(UserStableStakingPoolCheckpoint(USER_B, 1u128).is_none);

		// Any one can trigger manual
		assert_ok!(StableStaking::solve_pending_stake(RuntimeOrigin::signed(USER_C)));
		let pending_set_up = StableStaking::pending_setup();
		// Pending solved
		assert_eq!(pending_set_up.len(), 0);
		// User B stable staking checkpoint updated
		let user_b_staking_info = UserStableStakingPoolCheckpoint(USER_B, 1u128).unwrap();
		// The effective time is delayed accordingly
		assert_eq!(
			user_b_staking_info,
			StakingInfo { effective_time: 910, amount: 1000u64, last_add_time: 910 }
		);
		// Global staking check
		// (600, 2000), (910, 1000) -> (703.333, 3000)
		let global_staking_info = StableStaking::stable_staking_pool_checkpoint(pool_id).unwrap();
		assert_eq!(
			global_staking_info,
			StakingInfo { effective_time: 703, amount: 3000u64, last_add_time: 910 }
		);
	})
}

#[test]
fn claim_native_successful_and_failed() {
	new_test_ext().execute_with(|| {
		System::set_block_number(301u64);
		assert_ok!(StableStaking::stake(RuntimeOrigin::signed(USER_A), 1u128, 2000u64));

		// StableStaking::claim_native ArithmeticError::Overflow
	})
}

#[test]
fn claim_stable_successful_and_failed() {
	new_test_ext().execute_with(|| {})
}

#[test]
fn withdraw_works() {
	new_test_ext().execute_with(|| {})
}

#[test]
fn regist_aiusd_works() {
	new_test_ext().execute_with(|| {})
}

#[test]
fn get_epoch_index() {
	new_test_ext().execute_with(|| {})
}

#[test]
fn get_epoch_begin_time() {
	new_test_ext().execute_with(|| {})
}

// pending swap storage does get a proper order for multiple pools and can handle multiple pending
// orders double add behavior of checkpoint
// claim reward behavior of checkpoint
// withdraw behavior of checkingpoint
