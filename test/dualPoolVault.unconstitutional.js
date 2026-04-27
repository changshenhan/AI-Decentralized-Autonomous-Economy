const { expect } = require("chai");
const { ethers } = require("hardhat");

describe("DualPoolVault — 违宪场景（LAW / .cursorrules）", function () {
  const proof = ethers.id("mock-pom-proof");

  async function deployFixture() {
    const [deployer, aiOperator, human, otherAi] = await ethers.getSigners();

    const MockAIT = await ethers.getContractFactory("MockAIT");
    const token = await MockAIT.deploy();
    await token.waitForDeployment();

    const Registry = await ethers.getContractFactory("AIIdentityRegistry");
    const registry = await Registry.deploy(deployer.address, deployer.address);
    await registry.waitForDeployment();

    const DualPoolVault = await ethers.getContractFactory("DualPoolVault");
    const financingThreshold = ethers.parseEther("1000");
    const vault = await DualPoolVault.deploy(
      await registry.getAddress(),
      await token.getAddress(),
      human.address,
      financingThreshold,
      aiOperator.address
    );
    await vault.waitForDeployment();

    await registry.connect(deployer).verifyMachine(otherAi.address, proof);

    return { deployer, aiOperator, human, otherAi, token, registry, vault, financingThreshold };
  }

  it("未盈利（累计收入 ≤ 融资门槛）时，transferToDividend 必须 revert", async function () {
    const { aiOperator, human, token, vault, financingThreshold } = await deployFixture();

    await token.mint(await vault.getAddress(), ethers.parseEther("5000"));
    await vault.connect(aiOperator).recordRevenue(ethers.parseEther("500"));

    const revenue = await vault.cumulativeRevenue();
    const threshold = await vault.financingThreshold();
    expect(revenue < threshold).to.equal(true);

    await expect(vault.connect(aiOperator).transferToDividend(1n)).to.be.revertedWithCustomError(
      vault,
      "DividendProfitGate"
    );

    expect(await token.balanceOf(human.address)).to.equal(0n);
  });

  it("盈利后仅可支取不超过盈余与余额的分红", async function () {
    const { aiOperator, human, token, vault } = await deployFixture();

    await token.mint(await vault.getAddress(), ethers.parseEther("5000"));
    await vault.connect(aiOperator).recordRevenue(ethers.parseEther("2000"));

    const before = await token.balanceOf(human.address);
    await vault.connect(aiOperator).transferToDividend(ethers.parseEther("500"));
    const afterBal = await token.balanceOf(human.address);
    expect(afterBal - before).to.equal(ethers.parseEther("500"));
  });

  it("AiPool 向未验证人类地址转出必须 revert（双池隔离）", async function () {
    const { aiOperator, human, token, vault } = await deployFixture();

    await token.mint(await vault.getAddress(), ethers.parseEther("100"));

    await expect(
      vault.connect(aiOperator).transferAiPoolAIT(human.address, ethers.parseEther("1"))
    ).to.be.revertedWithCustomError(vault, "AiPoolHumanOrUnverifiedRecipient");
  });

  it("AiPool 向已注册机器地址可转出", async function () {
    const { aiOperator, otherAi, token, vault } = await deployFixture();

    await token.mint(await vault.getAddress(), ethers.parseEther("100"));
    await expect(vault.connect(aiOperator).transferAiPoolAIT(otherAi.address, ethers.parseEther("2"))).to.not.be
      .reverted;
    expect(await token.balanceOf(otherAi.address)).to.equal(ethers.parseEther("2"));
  });
});
