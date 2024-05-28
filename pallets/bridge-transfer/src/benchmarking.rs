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

//! bridge-transfer benchmark file

#![cfg(feature = "runtime-benchmarks")]
#![allow(clippy::type_complexity)]
#![allow(clippy::duplicated_attributes)]
#![allow(unused)]
#![allow(clippy::useless_vec)]
use super::*;
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_support::{
	ensure,
	traits::{Currency, SortedMembers},
	PalletId,
};
use frame_system::RawOrigin;
use hex_literal::hex;
use pallet_bridge::{BalanceOf as balance, EnsureOrigin, Get};
use sp_arithmetic::traits::Saturating;
use sp_runtime::traits::AccountIdConversion;
use sp_std::vec;

const MAXIMUM_ISSURANCE: u32 = 20_000;

type Currency<T> = Currency<<T as frame_system::Config>::AccountId>;

fn create_user<T: Config + pallet_balances::Config>(
	string: &'static str,
	n: u32,
	seed: u32,
) -> T::AccountId {
	let user = account(string, n, seed);

	let default_balance = Currency::<T>::minimum_balance().saturating_mul(MAXIMUM_ISSURANCE.into());
	let _ = Currency::<T>::deposit_creating(&user, default_balance);
	user
}

benchmarks! {
	transfer_assets{
		let sender:T::AccountId = create_user::<T>("sender",0u32,1u32);

		ensure!(T::TransferNativeMembers::contains(&sender),"add transfer_native_member failed");

		let dest_chain = 0;

		pallet_bridge::Pallet::<T>::whitelist_chain(
			RawOrigin::Root.into(),
			dest_chain,
		)?;

		let r_id = hex!("0000000000000000000000000000000a21dfe87028f214dd976be8479f5af001");

	}:_(RawOrigin::Signed(sender),50u32.into(),vec![0u8, 0u8, 0u8, 0u8],dest_chain,r_id)

	transfer{
		let r_id = hex!("0000000000000000000000000000000a21dfe87028f214dd976be8479f5af001");

		let sender = PalletId(*b"litry/bg").into_account_truncating();

		let default_balance =
		Currency::<T>::minimum_balance().saturating_mul(MAXIMUM_ISSURANCE.into());
		let _ = Currency::<T>::deposit_creating(&sender, default_balance);

		let to_account:T::AccountId = create_user::<T>("to",1u32,2u32);

	}:_(RawOrigin::Signed(sender),to_account,50u32.into(),r_id)
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
