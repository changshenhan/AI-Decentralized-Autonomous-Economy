// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

/// @title StakingPool
/// @notice $AIT 质押池：AI / 人类可锁定 AIT 获取来自国库税收的分红，用于调节流通与增强持币归属感
contract StakingPool is Ownable {
    using SafeERC20 for IERC20;

    IERC20 public immutable ait;

    uint256 public totalStaked;
    uint256 public accRewardPerShare; // 1e18 精度

    mapping(address => uint256) public staked;
    mapping(address => uint256) public rewardDebt;

    event Staked(address indexed user, uint256 amount);
    event Unstaked(address indexed user, uint256 amount);
    event RewardDeposited(uint256 amount, uint256 newAccRewardPerShare);
    event RewardClaimed(address indexed user, uint256 amount);

    constructor(IERC20 ait_, address owner_) Ownable(owner_) {
        require(address(ait_) != address(0), "StakingPool: zero token");
        ait = ait_;
    }

    /// @notice 从国库转入的一部分税收，用于回馈质押者
    function depositReward(uint256 amount) external {
        require(amount > 0, "StakingPool: zero");
        ait.safeTransferFrom(msg.sender, address(this), amount);
        if (totalStaked == 0) {
            return;
        }
        accRewardPerShare += (amount * 1e18) / totalStaked;
        emit RewardDeposited(amount, accRewardPerShare);
    }

    function stake(uint256 amount) external {
        require(amount > 0, "StakingPool: zero");
        _claim(msg.sender);
        ait.safeTransferFrom(msg.sender, address(this), amount);
        staked[msg.sender] += amount;
        totalStaked += amount;
        rewardDebt[msg.sender] = (staked[msg.sender] * accRewardPerShare) / 1e18;
        emit Staked(msg.sender, amount);
    }

    function unstake(uint256 amount) external {
        require(amount > 0, "StakingPool: zero");
        require(staked[msg.sender] >= amount, "StakingPool: not enough");
        _claim(msg.sender);
        staked[msg.sender] -= amount;
        totalStaked -= amount;
        rewardDebt[msg.sender] = (staked[msg.sender] * accRewardPerShare) / 1e18;
        ait.safeTransfer(msg.sender, amount);
        emit Unstaked(msg.sender, amount);
    }

    function claim() external {
        _claim(msg.sender);
        rewardDebt[msg.sender] = (staked[msg.sender] * accRewardPerShare) / 1e18;
    }

    function pendingReward(address user) public view returns (uint256) {
        uint256 userStaked = staked[user];
        uint256 accumulated = (userStaked * accRewardPerShare) / 1e18;
        if (accumulated < rewardDebt[user]) {
            return 0;
        }
        return accumulated - rewardDebt[user];
    }

    function _claim(address user) internal {
        uint256 reward = pendingReward(user);
        if (reward > 0) {
            ait.safeTransfer(user, reward);
            emit RewardClaimed(user, reward);
        }
    }
}

