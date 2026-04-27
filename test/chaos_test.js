const { expect } = require("chai");
const { ethers } = require("hardhat");
const { time } = require("@nomicfoundation/hardhat-network-helpers");

const proof = ethers.id("pom");

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

describe("AIDE Phase 9 — chaos & guardrail", function () {
  describe("场景 1：TaskMarket 向非 PoM 地址结算（物理隔离）", function () {
    it("拦截率 100%：任意次尝试 assignWorker(human AiPool) 均 revert", async function () {
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

      await token.mint(publisher.address, ethers.parseEther("100000"));
      await token.connect(publisher).approve(await market.getAddress(), ethers.MaxUint256);

      const attempts = 50;
      for (let i = 0; i < attempts; i++) {
        const tx = await market.connect(publisher).postTask(ethers.parseEther("1"));
        const receipt = await tx.wait();
        const id = parseTaskPostedId(market, receipt);
        await market.connect(publisher).fundTask(id);
        await expect(
          market.connect(publisher).assignWorker(id, aiWorker.address, human.address)
        ).to.be.revertedWithCustomError(market, "AiPoolRecipientNotVerified");
      }

      const rate = 100;
      console.log(`[场景1] TaskMarket 非 PoM 拦截: ${attempts}/${attempts} => ${rate}%`);
    });
  });

  describe("场景 2：治理冷却期攻击（Economist_Controller）", function () {
    it("冷却不可绕过：连发与多账户均无法突破 7 天间隔", async function () {
      const [admin, botOwner, attacker] = await ethers.getSigners();

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

      await controller.connect(botOwner).proposeNewTaxRate(100);
      expect(await treasury.taxRateBps()).to.equal(100n);

      for (let i = 0; i < 40; i++) {
        await expect(controller.connect(botOwner).proposeNewTaxRate(120)).to.be.revertedWithCustomError(
          controller,
          "EconomistCooldownActive"
        );
      }

      await expect(controller.connect(botOwner).updateSalaryCoefficient(ethers.parseEther("1"))).to.be.revertedWithCustomError(
        controller,
        "EconomistCooldownActive"
      );

      await expect(treasury.connect(attacker).setTaxRate(200)).to.be.reverted;

      const lastTs = await controller.lastEconomistActionAt();
      // 下一笔交易会再挖一块，timestamp >= 父块 +1；故父块须为 last+168h-2，子块才可能是 last+168h-1（仍锁）
      await time.increaseTo(lastTs + 168n * 3600n - 2n);
      await expect(controller.connect(botOwner).proposeNewTaxRate(120)).to.be.revertedWithCustomError(
        controller,
        "EconomistCooldownActive"
      );

      await time.increaseTo(lastTs + 168n * 3600n);
      await controller.connect(botOwner).proposeNewTaxRate(120);
      expect(await treasury.taxRateBps()).to.equal(120n);

      console.log("[场景2] 治理冷却：连发 40 次均 EconomistCooldownActive；increaseTo 边界仍锁定；满 7 天后可调");
    });
  });

  describe("场景 3：Guardrail 熔断（国库异常流出 → PAUSE）", function () {
    it("单笔流出 >30% 余额时同交易内进入 PAUSE，后续结算路径 revert", async function () {
      const [deployer, ops, human] = await ethers.getSigners();

      const MockAIT = await ethers.getContractFactory("MockAIT");
      const token = await MockAIT.deploy();
      await token.waitForDeployment();

      const Guardrail = await ethers.getContractFactory("Guardrail");
      const guardrail = await Guardrail.deploy(deployer.address);
      await guardrail.waitForDeployment();

      const Treasury = await ethers.getContractFactory("Treasury");
      const treasury = await Treasury.deploy(deployer.address, await token.getAddress(), await guardrail.getAddress());
      await treasury.waitForDeployment();

      const AIBank = await ethers.getContractFactory("AIBank");
      const bank = await AIBank.deploy(deployer.address);
      await bank.waitForDeployment();

      const InternalMarket = await ethers.getContractFactory("InternalMarket");
      const internalMarket = await InternalMarket.deploy(
        await token.getAddress(),
        await treasury.getAddress(),
        await bank.getAddress(),
        deployer.address,
        await guardrail.getAddress()
      );
      await internalMarket.waitForDeployment();

      await (await guardrail.setPeers(await treasury.getAddress(), await internalMarket.getAddress())).wait();

      const fund = ethers.parseEther("1000");
      await token.mint(await treasury.getAddress(), fund);

      await treasury.connect(deployer).setOperatingExpenseRecipient(ops.address, true);
      await treasury.connect(deployer).grantRole(await treasury.TREASURY_OPS_ROLE(), ops.address);

      const balBefore = await token.balanceOf(await treasury.getAddress());
      const badOut = (balBefore * 40n) / 100n;
      await treasury.connect(ops).withdrawOperatingExpenses(badOut, ops.address);

      expect(await guardrail.paused()).to.equal(true);
      await expect(guardrail.requireNotPaused()).to.be.revertedWithCustomError(guardrail, "SystemPaused");

      await treasury.connect(deployer).setCivilServant(human.address, true);
      await treasury.connect(deployer).setWeeklySalaryCoefficient(ethers.parseEther("1"));

      await expect(
        treasury.connect(deployer).distributeSalary(human.address, ethers.parseEther("10"))
      ).to.be.revertedWithCustomError(guardrail, "SystemPaused");

      const small = ethers.parseEther("1");
      await expect(treasury.connect(ops).withdrawOperatingExpenses(small, ops.address)).to.be.revertedWithCustomError(
        guardrail,
        "SystemPaused"
      );

      await guardrail.connect(deployer).unpause();
      expect(await guardrail.paused()).to.equal(false);

      await treasury.connect(ops).withdrawOperatingExpenses(small, ops.address);
      console.log("[场景3] 熔断：40% 流出触发 PAUSE；distributeSalary / withdraw 在 PAUSE 下 revert；unpause 后恢复");
    });
  });
});
