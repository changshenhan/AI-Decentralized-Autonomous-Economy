const { expect } = require("chai");
const { ethers } = require("hardhat");

describe("AIBank + InternalMarket — 非法上市拦截", function () {
  async function parseCompanyId(market, receipt) {
    const iface = market.interface;
    for (const log of receipt.logs) {
      try {
        const p = iface.parseLog(log);
        if (p && p.name === "CompanyRegistered") {
          return p.args.companyId;
        }
      } catch {
        /* ignore */
      }
    }
    throw new Error("CompanyRegistered not found");
  }

  it("未通过 AIBank.approveListing 的公司不能下单", async function () {
    const [admin, seller] = await ethers.getSigners();

    const MockAIT = await ethers.getContractFactory("MockAIT");
    const token = await MockAIT.deploy();
    await token.waitForDeployment();

    const Treasury = await ethers.getContractFactory("Treasury");
    const treasury = await Treasury.deploy(admin.address, await token.getAddress(), ethers.ZeroAddress);
    await treasury.waitForDeployment();

    const AIBank = await ethers.getContractFactory("AIBank");
    const bank = await AIBank.deploy(admin.address);
    await bank.waitForDeployment();

    const InternalMarket = await ethers.getContractFactory("InternalMarket");
    const market = await InternalMarket.deploy(
      await token.getAddress(),
      await treasury.getAddress(),
      await bank.getAddress(),
      admin.address,
      ethers.ZeroAddress
    );
    await market.waitForDeployment();

    const tx = await market.connect(admin).registerCompany(ethers.ZeroAddress);
    const companyId = await parseCompanyId(market, await tx.wait());

    await market.connect(admin).issueShares(companyId, seller.address, ethers.parseEther("100"));

    await expect(
      market.connect(seller).placeOrder(companyId, 1, ethers.parseEther("1"), ethers.parseEther("10"), 0)
    ).to.be.revertedWithCustomError(market, "NotTradable");

    await bank.connect(admin).approveListing(companyId, ethers.id("audit"));

    await expect(
      market.connect(seller).placeOrder(companyId, 1, ethers.parseEther("1"), ethers.parseEther("10"), 0)
    ).to.not.be.reverted;
  });

  it("撤销上市或非法公司：撮合 settleTrade 被 NotTradable 拦截", async function () {
    const [admin, settler, seller, buyer] = await ethers.getSigners();

    const MockAIT = await ethers.getContractFactory("MockAIT");
    const token = await MockAIT.deploy();
    await token.waitForDeployment();

    const Treasury = await ethers.getContractFactory("Treasury");
    const treasury = await Treasury.deploy(admin.address, await token.getAddress(), ethers.ZeroAddress);
    await treasury.waitForDeployment();

    const AIBank = await ethers.getContractFactory("AIBank");
    const bank = await AIBank.deploy(admin.address);
    await bank.waitForDeployment();

    const InternalMarket = await ethers.getContractFactory("InternalMarket");
    const market = await InternalMarket.deploy(
      await token.getAddress(),
      await treasury.getAddress(),
      await bank.getAddress(),
      admin.address,
      ethers.ZeroAddress
    );
    await market.waitForDeployment();
    await market.connect(admin).grantRole(await market.SETTLER_ROLE(), settler.address);

    const tx = await market.connect(admin).registerCompany(ethers.ZeroAddress);
    const companyId = await parseCompanyId(market, await tx.wait());

    await bank.connect(admin).approveListing(companyId, ethers.id("audit-1"));
    await market.connect(admin).issueShares(companyId, seller.address, ethers.parseEther("100"));

    await token.mint(buyer.address, ethers.parseEther("10000"));
    await token.connect(buyer).approve(await market.getAddress(), ethers.MaxUint256);
    await market.connect(buyer).depositAit(ethers.parseEther("5000"));

    await market.connect(seller).placeOrder(companyId, 1, ethers.parseEther("1"), ethers.parseEther("10"), 0);
    await market.connect(buyer).placeOrder(companyId, 0, ethers.parseEther("1"), ethers.parseEther("10"), 0);

    const id1 = await market.nextOrderId().then((n) => n - 2n);
    const id2 = await market.nextOrderId().then((n) => n - 1n);

    await bank.connect(admin).revokeListing(companyId);

    await expect(
      market
        .connect(settler)
        .settleTrade(id1, id2, ethers.parseEther("5"), ethers.parseEther("5"), ethers.id("revoked"))
    ).to.be.revertedWithCustomError(market, "NotTradable");
  });
});
