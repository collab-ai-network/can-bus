// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.3;

interface IStableStaking {
    /// @notice Describes pool setting.
    /// @param startTime: The start time of pool.
    /// @param epoch: The number of epoch the pool will last.
    /// @param epochRange: The number of block each epoch consist.
	/// @param setupTime: The least setup time before stable staking become effective.
	/// @param poolCap: The maximum staked amount pool allowed.
    struct PoolSetting {
        uint256 startTime;
        uint256 epoch;
		uint256 epochRange;
		uint256 setupTime;
		uint256 poolCap;
    }

    /// @notice Describes an acutal/synthetic staking position.
    /// @param effectiveTime: The average amount weight time of staking become effective.
    /// @param amount: Total staked amount.
    /// @param lastAddTime: The effective time of latest staking more command.
	struct StakingInfo {
		uint256 effectiveTime;
		uint256 amount;
		uint256 lastAddTime;
	}

	/// @notice Used to stake a specific pool.
	/// @param pool: Pool id.
	/// @param amount: The amount of tokens to be staked.
    /// @custom:selector 0x7b0472f0
	/// 				 stake(uint256,uint256)
    function stake(uint256 pool, uint256 amount) external;

	/// @notice Used to manually solving expired but ineffective pending staking.
    /// @custom:selector 0x63d38daf
	/// 				 solvePendingStake()
    function solvePendingStake() external;

	/// @notice Used to claim sender's native token staking reward up unitl specific block.
	/// @param untilTime: The time claim command up to.
	/// @custom:selector 0xdc9b262e
	/// 				 claimNative(uint256)
    function claimNative(uint256 untilTime) external;
	
	/// @notice Used to claim sender's stable token staking reward up unitl specific block of a specific pool
	/// @param pool: Pool id.
	/// @param untilTime: The time claim command up to.
	/// @custom:selector 0x26095361
	/// 				 claimStable(uint256,uint256)
    function claimStable(uint256 pool, uint256 untilTime) external;

	/// @notice Used to withdraw a specific pool after it expired.
	/// @param pool: Pool id.
	/// @custom:selector 0x2e1a7d4d
	/// 				 withdraw(uint256)
    function withdraw(uint256 pool) external;

	/// @notice Used to query the seting of specific pool.
	/// @param pool: Pool id.
	/// @return PoolSetting
	/// @custom:selector 0xbd360c1c
	/// 				 stakingPoolSetting(uint256)
	function stakingPoolSetting(uint256 pool) external view returns (PoolSetting memory);

	/// @notice Used to query the current unclaimed stable token reward of specific pool.
	/// @param pool: Pool id.
	/// @return The amount of reward.
	/// @custom:selector 0x12b5a598
	/// 				 stableStakingPoolReward(uint256)
	function stableStakingPoolReward(uint256 pool) external view returns (uint256);

	/// @notice Used to query the current unclaimed stable token reward of specific pool at specific epoch.
	/// @param pool: Pool id.
	/// @param epoch: Epoch index.
	/// @return The amount of reward.
	/// @custom:selector 0x754cc7fd
	/// 				 stableStakingPoolEpochReward(uint256,uint256)
	function stableStakingPoolEpochReward(uint256 pool, uint256 epoch) external view returns (uint256);

	/// @notice Used to query the current stable staking position of specific pool.
	/// @param pool: Pool id.
	/// @return StakingInfo
	/// @custom:selector 0xb25dc4d3
	/// 				 stableStakingPoolCheckpoint(uint256)
	function stableStakingPoolCheckpoint(uint256 pool) external view returns (StakingInfo memory);

	/// @notice Used to query user's current stable staking position of specific pool.
	/// @param pool: Pool id.
	/// @param user: user address (H160).
	/// @return StakingInfo
	/// @custom:selector 0x160fa8ef
	/// 				 userStableStakingPoolCheckpoint(address,uint256)
	function userStableStakingPoolCheckpoint(address user, uint256 pool) external view returns (StakingInfo memory);

	/// @notice Used to query user's current stable staking position of specific pool.
	/// @param pool: Pool id.
	/// @param user: user address (H256).
	/// @return StakingInfo
	/// @custom:selector 0x3d3287fc
	/// 				 userStableStakingPoolCheckpoint(bytes32,uint256)
	function userStableStakingPoolCheckpoint(bytes32 user, uint256 pool) external view returns (StakingInfo memory);

	/// @notice Used to query the current global native staking position.
	/// @return StakingInfo
	/// @custom:selector 0xd2de1062
	/// 				 nativeCheckpoint()
	function nativeCheckpoint() external view returns (StakingInfo memory);
	
	/// @notice Used to query user's native staking position.
	/// @param user: user address (H160).
	/// @return StakingInfo
	/// @custom:selector 0xd0ba36c5
	/// 				 userNativeCheckpoint(address)
	function userNativeCheckpoint(address user) external view returns (StakingInfo memory);

	/// @notice Used to query user's native staking position.
	/// @param user: user address (H256).
	/// @return StakingInfo
	/// @custom:selector 0xe889da18
	/// 				 userNativeCheckpoint(bytes32)
	function userNativeCheckpoint(bytes32 user) external view returns (StakingInfo memory);

	/// @notice Used to query pending amount solved for stable staking of specific pool.
	/// @param pool: Pool id.
	/// @return The amount of pending staking.
	/// @custom:selector 0x4836c543
	/// 				 pendingAmount(uint256)
	function pendingAmount(uint256 pool) external view returns (uint256);
}
