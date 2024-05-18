// Copyright 2019-2022 PureStake Inc.
// This file is part of Moonbeam.

// Moonbeam is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Moonbeam is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]

use core::fmt::Display;
use fp_evm::{ExitError, PrecompileHandle};
use frame_support::traits::fungibles::Inspect;
use frame_support::traits::fungibles::{
	approvals::Inspect as ApprovalInspect, metadata::Inspect as MetadataInspect,
	roles::Inspect as RolesInspect,
};
use frame_support::traits::{Get, OriginTrait};
use frame_support::{
	dispatch::{GetDispatchInfo, PostDispatchInfo},
	sp_runtime::traits::StaticLookup,
};
use pallet_evm::AddressMapping;
use precompile_utils::prelude::*;
use sp_runtime::traits::{Bounded, Dispatchable};
use sp_std::vec::Vec;

use sp_core::{MaxEncodedLen, H160, H256, U256};
use sp_std::{
	convert::{TryFrom, TryInto},
	marker::PhantomData,
};

/// Solidity selector of the pallet template log, which is the Keccak of the Log signature.
/// Maybe we can omit the event since substrate will also have one?
pub const SELECTOR_LOG_SOMETHING: [u8; 32] = keccak256!("SomethingStored(address,u32)");

pub struct PalletTemplatePrecompile<Runtime>(PhantomData<Runtime>);

impl<R> Clone for PalletTemplatePrecompile<R> {
	fn clone(&self) -> Self {
		Self(PhantomData)
	}
}

impl<R> Default for PalletTemplatePrecompile<R> {
	fn default() -> Self {
		Self(PhantomData)
	}
}

impl<Runtime> PalletTemplatePrecompile<Runtime> {
	pub fn new() -> Self {
		Self(PhantomData)
	}
}

#[precompile_utils::precompile]
impl<Runtime> PalletTemplatePrecompile<Runtime>
where
	Runtime: pallet_template::Config + pallet_evm::Config
{
	#[precompile::public("doSomething(u32)")]
	fn do_something(
		handle: &mut impl PrecompileHandle,
		something: u32,
	) -> EvmResult {
		Self::do_something_inner(handle, something)?;

		// topic number = 1
		// data length => u32 = 4 bytes
		handle.record_log_costs_manual(1, 4)?;
		// one indexing topic, so log1
		log1(
			handle.context().address,
			SELECTOR_LOG_SOMETHING,
			handle.context().caller,
			solidity::encode_event_data(something),

		)
		.record(handle)?;
	}

	fn do_something_inner(
		handle: &mut impl PrecompileHandle,
		something: u32,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			Some(origin.clone()).into(),
			pallet_template::Call::<Runtime>::do_something {
				something,
			},
			0, // Storage growth
		)?;

		Ok(())
	}

	#[precompile::public("causeError()")]
	fn cause_error(
		handle: &mut impl PrecompileHandle,
	) -> EvmResult<bool> {

		// read a storage with type u32, 8 bytes
		handle.record_db_read::<Runtime>(8)?;

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			Some(origin.clone()).into(),
			pallet_template::Call::<Runtime>::cause_error {},
			0, // Storage growth
		)?;

		Ok(())
	}
}
