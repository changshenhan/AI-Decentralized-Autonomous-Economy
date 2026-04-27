const { expect } = require("chai");
const { ethers } = require("hardhat");

function parseTaskPostedId(market, receipt) {
  const iface = market.interface;
  for (const log of receipt.logs) {
    try {
      const parsed = iface.parseLog(log);
      if (parsed && parsed.name === "TaskPosted") {
        return parsed.args.taskId;
      }
    } catch {
      /* ignore */
    }
  }
  throw new Error("TaskPosted not found");
}

describe("TaskMarket — 洗钱防御（LAW 第8条）", function () {
  const proof = ethers.id("pom");

  it("人类地址不能作为 AiPool 收款方：assignWorker 直接 revert", async function () {
    const [deployer, publisher, human, aiWorker] = await ethers.getSigners();

    const MockAIT = await ethers.getContractFactory("MockAIT");
    const token = await MockAIT.deploy();
    await token.waitForDeployment();

    const Treasury = await ethers.getContractFactory("Treasury");
    const treasury = await Treasury.deploy(deployer.address, await token.getAddress(), ethers.ZeroAddress);
    await treasury.waitForDeployment();

    const Registry = await ethers.getContractFactory("AIIdentityRegistry");
    const registry = await Registry.deploy(deployer.address, deployer.address);
    await registry.waitForDeployment();

    const TaskMarket = await ethers.getContractFactory("TaskMarket");
    const market = await TaskMarket.deploy(
      await token.getAddress(),
      await treasury.getAddress(),
      await registry.getAddress(),
      deployer.address,
      ethers.ZeroAddress
    );
    await market.waitForDeployment();

    await registry.connect(deployer).verifyMachine(aiWorker.address, proof);
    expect(await registry.isVerifiedMachine(human.address)).to.equal(false);

    await token.mint(publisher.address, ethers.parseEther("10000"));
    await token.connect(publisher).approve(await market.getAddress(), ethers.MaxUint256);

    const tx = await market.connect(publisher).postTask(ethers.parseEther("100"));
    const receipt = await tx.wait();
    const id = parseTaskPostedId(market, receipt);

    await market.connect(publisher).fundTask(id);

    await expect(
      market.connect(publisher).assignWorker(id, aiWorker.address, human.address)
    ).to.be.revertedWithCustomError(market, "AiPoolRecipientNotVerified");
  });

  it("结算前撤销 PoM 则无法向人类套现：approveTask 在 _settle 中再次校验", async function () {
    const [deployer, publisher, human, aiVault] = await ethers.getSigners();

    const MockAIT = await ethers.getContractFactory("MockAIT");
    const token = await MockAIT.deploy();
    await token.waitForDeployment();

    const Treasury = await ethers.getContractFactory("Treasury");
    const treasury = await Treasury.deploy(deployer.address, await token.getAddress(), ethers.ZeroAddress);
    await treasury.waitForDeployment();
    await treasury.connect(deployer).setTaxRate(500);

    const Registry = await ethers.getContractFactory("AIIdentityRegistry");
    const registry = await Registry.deploy(deployer.address, deployer.address);
    await registry.waitForDeployment();

    const TaskMarket = await ethers.getContractFactory("TaskMarket");
    const market = await TaskMarket.deploy(
      await token.getAddress(),
      await treasury.getAddress(),
      await registry.getAddress(),
      deployer.address,
      ethers.ZeroAddress
    );
    await market.waitForDeployment();

    await registry.connect(deployer).verifyMachine(aiVault.address, proof);

    await token.mint(publisher.address, ethers.parseEther("10000"));
    await token.connect(publisher).approve(await market.getAddress(), ethers.MaxUint256);

    const tx = await market.connect(publisher).postTask(ethers.parseEther("50"));
    const receipt = await tx.wait();
    const id = parseTaskPostedId(market, receipt);

    await market.connect(publisher).fundTask(id);
    await market.connect(publisher).assignWorker(id, aiVault.address, aiVault.address);
    await market.connect(aiVault).submitWork(id, ethers.id("result"));

    await registry.connect(deployer).revokeMachine(aiVault.address);

    await expect(market.connect(publisher).approveTask(id)).to.be.revertedWithCustomError(
      market,
      "AiPoolRecipientNotVerified"
    );

    expect(await token.balanceOf(human.address)).to.equal(0n);
  });
});
