use crate::{mock::*, Error, Event};
use frame_support::{assert_noop, assert_ok};

#[test]
fn set_enabled_require_origin_check() {
	new_test_ext().execute_with(|| {
		assert_eq!(HalvingMint::enabled(), false);
		assert_noop!(
			HalvingMint::set_enabled(RuntimeOrigin::signed(1), true),
			sp_runtime::DispatchError::BadOrigin,
		);
		assert_ok!(HalvingMint::set_enabled(RuntimeOrigin::root(), true));
		assert_eq!(HalvingMint::enabled(), true);
		System::assert_last_event(Event::MintStateChanged { enabled: true }.into());
	});
}
