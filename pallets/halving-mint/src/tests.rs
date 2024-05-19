use crate::{mock::*, Error, Event};
use frame_support::{assert_noop, assert_ok};

#[test]
fn set_enabled_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(HalvingMint::enabled(), false);
		assert_noop!(
			HalvingMint::set_enabled(RuntimeOrigin::signed(1), true),
			sp_runtime::DispatchError::BadOrigin,
		);
		assert_noop!(
			HalvingMint::set_enabled(RuntimeOrigin::root(), true),
			Error::<Test>::MintNotStarted,
		);
		assert_ok!(HalvingMint::start_mint_from_next_block(RuntimeOrigin::root()));
		assert_eq!(HalvingMint::enabled(), true);
		assert_ok!(HalvingMint::set_enabled(RuntimeOrigin::root(), false));
		assert_eq!(HalvingMint::enabled(), false);
		System::assert_last_event(Event::MintStateChanged { enabled: false }.into());
	});
}

#[test]
fn start_mint_too_early_fails() {
	new_test_ext().execute_with(|| {
		assert_eq!(System::block_number(), 1);
		assert_noop!(
			HalvingMint::start_mint_from_block(RuntimeOrigin::root(), 0),
			Error::<Test>::StartBlockTooEarly,
		);
		assert_noop!(
			HalvingMint::start_mint_from_block(RuntimeOrigin::root(), 1),
			Error::<Test>::StartBlockTooEarly,
		);
		assert_ok!(HalvingMint::start_mint_from_block(RuntimeOrigin::root(), 2));
		System::assert_last_event(Event::MintStarted { start_block: 2 }.into());
	});
}

#[test]
fn halving_mint_works() {
	new_test_ext().execute_with(|| {
		let beneficiary = HalvingMint::beneficiary_account();

		assert_eq!(System::block_number(), 1);
		assert_eq!(Balances::total_issuance(), 10);
		assert_eq!(Balances::free_balance(&beneficiary), 10);
		assert_ok!(HalvingMint::start_mint_from_next_block(RuntimeOrigin::root()));
		System::assert_last_event(Event::MintStarted { start_block: 2 }.into());

		run_to_block(2);
		// 50 tokens are minted
		assert_eq!(Balances::total_issuance(), 60);
		assert_eq!(Balances::free_balance(&beneficiary), 10);
		assert_eq!(Balances::free_balance(&1), 50);

		run_to_block(11);
		assert_eq!(Balances::total_issuance(), 510);
		assert_eq!(Balances::free_balance(&beneficiary), 10);
		assert_eq!(Balances::free_balance(&1), 500);

		run_to_block(12);
		// the first halving
		assert_eq!(Balances::total_issuance(), 535);
		assert_eq!(Balances::free_balance(&beneficiary), 10);
		assert_eq!(Balances::free_balance(&1), 525);

		run_to_block(22);
		// the second halving
		assert_eq!(Balances::total_issuance(), 772);
		assert_eq!(Balances::free_balance(&beneficiary), 10);
		assert_eq!(Balances::free_balance(&1), 762);

		run_to_block(52);
		// the fifth halving - only 1 token is minted
		assert_eq!(Balances::total_issuance(), 971);
		assert_eq!(Balances::free_balance(&beneficiary), 10);
		assert_eq!(Balances::free_balance(&1), 961);

		run_to_block(62);
		// the sixth halving - but 0 tokens will be minted
		assert_eq!(Balances::total_issuance(), 980);
		assert_eq!(Balances::free_balance(&beneficiary), 10);
		assert_eq!(Balances::free_balance(&1), 970);

		run_to_block(1_000);
		// no changes since the sixth halving, the total minted token will be fixated on 980,
		// the "missing" 20 comes from the integer division and the total_issuance is too small.
		//
		// we'll have much accurate result in reality where token unit is 18 decimal
		assert_eq!(Balances::total_issuance(), 980);
		assert_eq!(Balances::free_balance(&beneficiary), 10);
		assert_eq!(Balances::free_balance(&1), 970);
	});
}
