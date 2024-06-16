use super::{
	mock::{
		assert_events, balances, new_test_ext, Assets, Balances, RuntimeCall, RuntimeEvent,
		RuntimeOrigin, StableStaking, System, Test, ENDOWED_BALANCE, USER_A, USER_B, USER_C,
	},
	pallet_stable_staking, *,
};
use frame_support::{assert_noop, assert_ok};
use hex_literal::hex;
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
		let pool_setup: PoolSetting = PoolSetting {
			start_time: 100u64,
			epoch: 10u128,
			epoch_range: 100u64,
			setup_time: 200u64,
			pool_cap: 1_000_000_000u64,
		};
		assert_noop!(
			StableStaking::create_staking_pool(RuntimeOrigin::root(), 1u128, pool_setup),
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
		assert_events(vec![
			RuntimeEvent::StableStaking(Event::RewardUpdated {
				pool_id: 1u128,
				epoch: 0u128,
				amount: 2000u64,
			}),
		]);
		// Staking pool reward storage efffective
		assert_eq!(StableStaking::stable_staking_pool_reward(1u128), 2000u64);
		assert_eq!(StableStaking::stable_staking_pool_epoch_reward(1u128, 0u128), Some(2000u64));
		assert_eq!(StableStaking::stable_staking_pool_epoch_reward(1u128, 1u128), None);
		// Staking pool balance effective
		let native_token_pool = HavlingMintId::get().into_account_truncating();
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
		assert_events(vec![
			RuntimeEvent::StableStaking(Event::RewardUpdated {
				pool_id: 1u128,
				epoch: 10u128,
				amount: 2000u64,
			}),
		]);

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
		let global_staking_info = StableStaking::native_checkpoint().unwrap();
		assert_eq!(global_staking_info, StakingInfo { effective_time: 301, amount: 2000u64, last_add_time: 301});
		let user_a_staking_info = StableStaking::user_native_checkpoint(USER_A).unwrap();
		assert_eq!(user_a_staking_info, StakingInfo { effective_time: 301, amount: 2000u64, last_add_time: 301});
		let pending_set_up = StableStaking::pending_setup();
		assert_eq(pending_set_up.len(), 1);
		let pending_set_up_element = pending_set_up.get(0).unwrap();
		// Pool set up time = 200
		// So user enter at 301 need to wait till 600 to make it effective and receiving Stable staking reward
		assert_eq!(pending_set_up_element, StakingInfoWithOwner { who: USER_A, pool_id: 1u128, staking_info: StakingInfo { effective_time: 600, amount: 2000u64, last_add_time: 600}});

		// Second user B stake
		fast_forward_to(311u64);
		assert_ok!(StableStaking::stake(RuntimeOrigin::signed(USER_B), 1u128, 1000u64));
		let global_staking_info = StableStaking::native_checkpoint().unwrap();
		// Synthetic (301, 2000), (311, 1000) = (304.3333, 3000)
		assert_eq!(global_staking_info, StakingInfo { effective_time: 304, amount: 3000u64, last_add_time: 311});
		// user a unchanged
		let user_a_staking_info = StableStaking::user_native_checkpoint(USER_A).unwrap();
		assert_eq!(user_a_staking_info, StakingInfo { effective_time: 301, amount: 2000u64, last_add_time: 301});
		// user b
		let user_b_staking_info = StableStaking::user_native_checkpoint(USER_B).unwrap();
		assert_eq!(user_b_staking_info, StakingInfo { effective_time: 311, amount: 1000u64, last_add_time: 311});
		// Pending set up storage change
		let pending_set_up = StableStaking::pending_setup();
		assert_eq(pending_set_up.len(), 2);
		// Pool set up time = 200
		// So user enter at 311 need to wait till 600 to make it effective and receiving Stable staking reward
		assert_eq!(pending_set_up_element, StakingInfoWithOwner { who: USER_B, pool_id: 1u128, staking_info: StakingInfo { effective_time: 600, amount: 1000u64, last_add_time: 600}});

		// Can not stake if no AIUSD registed
		<AIUSDAssetId<Test>>::kill();
		assert_noop!(
			StableStaking::stake(RuntimeOrigin::signed(USER_C), 1u128, 3000u64),
			Error::<Test>::NoAssetId
		);
	})
}

// double add behavior of checkpoint
// claim reward behavior of checkpoint
// withdraw behavior of checkingpoint
