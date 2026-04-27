// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Burnable} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {ITreasury} from "./interfaces/ITreasury.sol";

/// @title AIToken
/// @notice $AIT：唯一价值锚点；铸造权仅国库（LAW 第1条、TECH 蓝图）
/// @dev 交易税：fee = amount * getCurrentTaxRate() / 10_000（BPS）
contract AIToken is ERC20, ERC20Burnable, Ownable {
    uint256 public constant BPS_DENOMINATOR = 10_000;

    /// @notice 国库合约地址；接收交易税并持有税率配置
    address public treasury;

    /// @notice TaskMarket / InternalMarket 等结算合约：`from` 为免自动抽税地址时由 `Treasury.collectTax` 显式抽税，避免双重征税
    mapping(address => bool) public taxExemptSender;

    bool private _processingTax;

    event TreasuryUpdated(address indexed previousTreasury, address indexed newTreasury);
    event TaxCollected(address indexed from, address indexed to, uint256 feeAmount, uint256 netAmount);

    constructor(address initialOwner) ERC20("AI Token", "AIT") Ownable(initialOwner) {}

    /// @notice 部署后一次性将 treasury 设为国库合约（亦可用于迁移）
    function setTreasury(address newTreasury) external onlyOwner {
        require(newTreasury != address(0), "AIT: zero treasury");
        emit TreasuryUpdated(treasury, newTreasury);
        treasury = newTreasury;
    }

    function setTaxExemptSender(address account, bool exempt) external onlyOwner {
        taxExemptSender[account] = exempt;
    }

    modifier onlyTreasury() {
        require(msg.sender == treasury, "AIT: only treasury");
        _;
    }

    /// @notice 仅国库可增发（按劳分配等）
    function mint(address to, uint256 amount) external onlyTreasury {
        _mint(to, amount);
    }

    /// @notice 显式交易税转账路径，供审计与 Market/协议集成（与 transfer 行为一致）
    function taxedTransfer(address to, uint256 amount) external returns (bool) {
        return transfer(to, amount);
    }

    /// @notice 显式交易税转账路径（授权额度路径）
    function taxedTransferFrom(address from, address to, uint256 amount) external returns (bool) {
        return transferFrom(from, to, amount);
    }

    function _update(address from, address to, uint256 value) internal virtual override {
        if (from != address(0) && to != address(0) && !_processingTax && !taxExemptSender[from]) {
            require(treasury != address(0), "AIT: treasury unset");
            uint256 rate = ITreasury(treasury).getCurrentTaxRate();
            require(rate <= BPS_DENOMINATOR, "AIT: tax rate");
            uint256 fee = (value * rate) / BPS_DENOMINATOR;
            if (fee > 0) {
                _processingTax = true;
                super._update(from, treasury, fee);
                super._update(from, to, value - fee);
                _processingTax = false;
                emit TaxCollected(from, to, fee, value - fee);
                return;
            }
        }
        super._update(from, to, value);
    }
}
