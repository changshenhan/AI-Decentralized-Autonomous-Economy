// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @notice 测试用 $AIT 占位 ERC20
contract MockAIT is ERC20 {
    constructor() ERC20("Mock AIT", "mAIT") {}

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}
