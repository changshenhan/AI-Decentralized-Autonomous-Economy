const { expect } = require("chai");
const { ethers } = require("hardhat");
const { time } = require("@nomicfoundation/hardhat-network-helpers");

describe("AI_Economist_Controller", function () {
  it("税率须在 0.1%-5% BPS 内，且 7 天内不能再次调整", async function () {
    const [admin, botOwner] = await ethers.getSigners();

    const MockAIT = await ethers.getContractFactory("MockAIT");
    const token = await MockAIT.deploy();
    await token.waitForDeployment();

    const Treasury = await ethers.getContractFactory("Treasury");
    const treasury = await Treasury.deploy(admin.address, await token.getAddress(), ethers.ZeroAddress);
    await treasury.waitForDeployment();

    const Controller = await ethers.getContractFactory("AI_Economist_Controller");
    const controller = await Controller.deploy(await treasury.getAddress(), botOwner.address);
    await controller.waitForDeployment();

    const ECONOMIST_ROLE = await treasury.ECONOMIST_ROLE();
    await treasury.connect(admin).grantRole(ECONOMIST_ROLE, await controller.getAddress());
    await treasury.connect(admin).revokeRole(ECONOMIST_ROLE, admin.address);

    await expect(controller.connect(botOwner).proposeNewTaxRate(9)).to.be.revertedWithCustomError(
      controller,
      "TaxRateOutOfRange"
    );

    await expect(controller.connect(botOwner).proposeNewTaxRate(600)).to.be.revertedWithCustomError(
      controller,
      "TaxRateOutOfRange"
    );

    await controller.connect(botOwner).proposeNewTaxRate(250);
    expect(await treasury.taxRateBps()).to.equal(250n);

    await expect(controller.connect(botOwner).updateSalaryCoefficient(ethers.parseEther("1"))).to.be.revertedWithCustomError(
      controller,
      "EconomistCooldownActive"
    );

    await time.increase(168n * 3600n + 1n);

    await controller.connect(botOwner).updateSalaryCoefficient(ethers.parseEther("2"));
    expect(await treasury.weeklySalaryCoefficientS()).to.equal(ethers.parseEther("2"));
  });
});
