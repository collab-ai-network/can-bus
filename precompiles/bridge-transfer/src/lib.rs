#![cfg_attr(not(feature = "std"), no_std)]

use fp_evm::{PrecompileFailure, PrecompileHandle};

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_evm::AddressMapping;
use precompile_utils::prelude::*;
use sp_runtime::traits::Dispatchable;

use sp_core::{H256, U256};
use sp_std::marker::PhantomData;

use pallet_bridge_transfer::BalanceOf;

pub struct BridgeTransferPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> BridgeTransferPrecompile<Runtime>
where
	Runtime: pallet_bridge_transfer::Config + pallet_evm::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<pallet_bridge_transfer::Call<Runtime>>,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	BalanceOf<Runtime>: TryFrom<U256> + Into<U256>,
{
	#[precompile::public("transferAssets(uint256,uint8,bytes32,bytes)")]
	fn transfer_assets(
		handle: &mut impl PrecompileHandle,
		amount: U256,
		dest_id: u8,
		resource_id: H256,
		recipient: UnboundedBytes,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let amount: BalanceOf<Runtime> = amount.try_into().map_err(|_| {
			Into::<PrecompileFailure>::into(RevertReason::value_is_too_large("balance type"))
		})?;
		let recipient: Vec<u8> = recipient.into();
		let resource_id = resource_id.into();

		let call = pallet_bridge_transfer::Call::<Runtime>::transfer_assets {
			amount,
			recipient,
			dest_id,
			resource_id,
		};
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}
}
