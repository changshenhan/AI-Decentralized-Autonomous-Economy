// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title AideCreate2Factory
/// @notice 通用 CREATE2 部署器。固定 salt + 相同 creation code + **相同工厂合约地址** ⇒ 多链可复现同一子合约地址（工厂须由同一部署者以相同 nonce 部署，见运维文档）
contract AideCreate2Factory {
    event Deployed(address indexed addr, bytes32 indexed salt);

    function deploy(bytes32 salt, bytes memory creationCode) external payable returns (address addr) {
        assembly {
            addr := create2(callvalue(), add(creationCode, 0x20), mload(creationCode), salt)
        }
        require(addr != address(0), "AideCreate2: deploy failed");
        emit Deployed(addr, salt);
    }
}
