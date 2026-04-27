// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice 仅 Treasury 可调用的铸造入口
interface IAITokenMint {
    function mint(address to, uint256 amount) external;
}
