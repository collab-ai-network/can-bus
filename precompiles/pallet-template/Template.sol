// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.3;

interface ITemplate {
    /// @custom:selector 0x82692679
    function doSomething() external;

    /// @custom:selector 0x75819ce6
    function causeError() external;

    /// @dev Event emited when a doSomething has been performed.
    /// @custom:selector 0xa60b7315b043a32db8b87b8117f43e32672d769699d8b40879a17b4c5123577b
    /// @param operator address The operator address
    /// @param value uint32 The amount of tokens transfered.
    event SomethingStored(address indexed operator, uint32 value);
}
