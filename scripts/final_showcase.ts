/**
 * AIDE 1.0 终章演示：部署 → PoM → 任务 → 成交 → 宏观降税 → 熔断攻击 → 恢复
 *
 * 运行：npx hardhat run scripts/final_showcase.ts
 */
import { expect } from "chai";
import { ethers } from "hardhat";
import { time } from "@nomicfoundation/hardhat-network-helpers";
import { signTeeAttestation } from "./lib/teeAttest";

function asciiReport(state: {
  treasuryBal: string;
  taxBps: string;
  guardrailPaused: boolean;
  macroTaxCut: string;
  circuitTripped: boolean;
  recovered: boolean;
}) {
  const art = `
    █████╗     ██╗██████╗ ███████╗
   ██╔══██╗    ██║██╔══██╗██╔════╝
   ███████║    ██║██║  ██║█████╗  
   ██╔══██║    ██║██║  ██║██╔══╝  
   ██║  ██║██╗ ██║██████╔╝███████╗
   ╚═╝  ╚═╝╚═╝ ╚═╝╚═════╝ ╚══════╝
`;
  console.log(art);
  console.log("┌─────────────────────────────────────────────────────────┐");
  console.log("│           AIDE 1.0  系统状态终报 (Final Report)          │");
  console.log("├─────────────────────────────────────────────────────────┤");
  console.log(`│  Treasury AIT (hint)     : ${state.treasuryBal.padEnd(28)}│`);
  console.log(`│  Tax rate (BPS)          : ${state.taxBps.padEnd(28)}│`);
  console.log(`│  Macro tax cut (7d cool) : ${state.macroTaxCut.padEnd(28)}│`);
  console.log(`│  Guardrail PAUSE         : ${String(state.guardrailPaused).padEnd(28)}│`);
  console.log(`│  Circuit tripped (demo)  : ${String(state.circuitTripped).padEnd(28)}│`);
  console.log(`│  Recovered (unpause)     : ${String(state.recovered).padEnd(28)}│`);
  console.log("└─────────────────────────────────────────────────────────┘");
  console.log("");
}

async function main() {
  const [deployer, ai1, ai2, ai3, human, ops] = await ethers.getSigners();

  console.log("\n═══ Phase 10 — AIDE 1.0 Final Showcase ═══\n");

  const AIToken = await ethers.getContractFactory("AIToken");
  const ait = await AIToken.deploy(deployer.address);
  await ait.waitForDeployment();

  const Guardrail = await ethers.getContractFactory("Guardrail");
  const guardrail = await Guardrail.deploy(deployer.address);
  await guardrail.waitForDeployment();

  const Treasury = await ethers.getContractFactory("Treasury");
  const treasury = await Treasury.deploy(deployer.address, await ait.getAddress(), await guardrail.getAddress());
  await treasury.waitForDeployment();
  await (await ait.setTreasury(await treasury.getAddress())).wait();

  const teeAuthority = deployer.address;
  const AIIdentityRegistry = await ethers.getContractFactory("AIIdentityRegistry");
  const idRegistry = await AIIdentityRegistry.deploy(deployer.address, teeAuthority);
  await idRegistry.waitForDeployment();

  const AIBank = await ethers.getContractFactory("AIBank");
  const bank = await AIBank.deploy(deployer.address);
  await bank.waitForDeployment();

  const InternalMarket = await ethers.getContractFactory("InternalMarket");
  const internalMarket = await InternalMarket.deploy(
    await ait.getAddress(),
    await treasury.getAddress(),
    await bank.getAddress(),
    deployer.address,
    await guardrail.getAddress()
  );
  await internalMarket.waitForDeployment();

  await (await guardrail.setPeers(await treasury.getAddress(), await internalMarket.getAddress())).wait();

  const TaskMarket = await ethers.getContractFactory("TaskMarket");
  const taskMarket = await TaskMarket.deploy(
    await ait.getAddress(),
    await treasury.getAddress(),
    await idRegistry.getAddress(),
    deployer.address,
    await guardrail.getAddress()
  );
  await taskMarket.waitForDeployment();

  const EconCtrl = await ethers.getContractFactory("AI_Economist_Controller");
  const econCtrl = await EconCtrl.deploy(await treasury.getAddress(), deployer.address);
  await econCtrl.waitForDeployment();

  await (await ait.setTaxExemptSender(await internalMarket.getAddress(), true)).wait();
  await (await ait.setTaxExemptSender(await taskMarket.getAddress(), true)).wait();

  const ECONOMIST_ROLE = await treasury.ECONOMIST_ROLE();
  await (await treasury.grantRole(ECONOMIST_ROLE, await econCtrl.getAddress())).wait();

  console.log("[1/7] 部署完成 — 核心合约 + Economist Controller 已授权");

  const regTx = await internalMarket.registerCompany(ethers.ZeroAddress);
  const regRc = await regTx.wait();
  const regEvent = regRc!.logs
    .map((l) => {
      try {
        return internalMarket.interface.parseLog(l);
      } catch {
        return undefined;
      }
    })
    .find((e) => e && e.name === "CompanyRegistered");
  const companyId = regEvent!.args.companyId as bigint;
  await (await bank.approveListing(companyId, ethers.id("showcase-company"))).wait();

  await (await econCtrl.proposeNewTaxRate(30n)).wait();
  console.log("[2/7] 上市审计通过；Economist 初始税率 30 BPS");

  const agents = [ai1, ai2, ai3];
  const { chainId } = await ethers.provider.getNetwork();
  const deadline = BigInt(Math.floor(Date.now() / 1000) + 86400 * 365);
  const regAddr = await idRegistry.getAddress();
  for (const a of agents) {
    const salt = ethers.id(`showcase-tee-${a.address}`);
    const sig = await signTeeAttestation(deployer, regAddr, a.address, deadline, salt, BigInt(chainId));
    await (await idRegistry.attestMachineWithTeeSignature(a.address, deadline, salt, sig)).wait();
  }
  console.log("[3/7] AI 代理人 PoM（TEE ECDSA）登记完成");

  await (await treasury.setTaxRate(0)).wait();
  await (await treasury.setCivilServant(human.address, true)).wait();
  await (await treasury.setWeeklySalaryCoefficient(ethers.parseEther("1"))).wait();
  await (await treasury.distributeSalary(human.address, ethers.parseEther("100"))).wait();

  const bounty = ethers.parseEther("50");
  await (await ait.connect(human).approve(await taskMarket.getAddress(), bounty)).wait();
  const postTx = await taskMarket.connect(human).postTask(bounty);
  const postRc = await postTx.wait();
  const postEv = postRc!.logs
    .map((l) => {
      try {
        return taskMarket.interface.parseLog(l);
      } catch {
        return undefined;
      }
    })
    .find((e) => e && e.name === "TaskPosted");
  const taskId = postEv!.args.taskId as bigint;
  await (await taskMarket.connect(human).fundTask(taskId)).wait();
  await (await taskMarket.connect(human).assignWorker(taskId, ai1.address, ai1.address)).wait();
  await (await taskMarket.connect(ai1).submitWork(taskId, ethers.id("showcase-result"))).wait();
  await (await taskMarket.connect(human).approveTask(taskId)).wait();

  await (await treasury.setTaxRate(30n)).wait();
  await (await internalMarket.issueShares(companyId, ai2.address, ethers.parseEther("100"))).wait();
  await (await treasury.setCivilServant(ai3.address, true)).wait();
  await (await treasury.distributeSalary(ai3.address, ethers.parseEther("100"))).wait();
  await (await ait.connect(ai3).approve(await internalMarket.getAddress(), ethers.MaxUint256)).wait();
  await (await internalMarket.connect(ai3).depositAit(ethers.parseEther("100"))).wait();

  await (
    await internalMarket.connect(ai2).placeOrder(companyId, 1, ethers.parseEther("1"), ethers.parseEther("10"), 0)
  ).wait();
  await (
    await internalMarket.connect(ai3).placeOrder(companyId, 0, ethers.parseEther("1"), ethers.parseEther("10"), 0)
  ).wait();

  const nextOrderId = await internalMarket.nextOrderId();
  const sellOrderId = nextOrderId - 2n;
  const buyOrderId = nextOrderId - 1n;
  const aitNotional = ethers.parseEther("10");
  await (await internalMarket.connect(deployer).grantRole(await internalMarket.SETTLER_ROLE(), deployer.address)).wait();
  await (
    await internalMarket
      .connect(deployer)
      .settleTrade(sellOrderId, buyOrderId, ethers.parseEther("10"), aitNotional, ethers.id("showcase-trade"))
  ).wait();

  console.log("[4/7] Task 结算 + InternalMarket 成交（税收入国库）");

  await (await treasury.setWeeklySalaryCoefficient(ethers.parseEther("2000"))).wait();
  await (await treasury.setCivilServant(deployer.address, true)).wait();
  await (await treasury.revokeRole(ECONOMIST_ROLE, deployer.address)).wait();
  await (await treasury.distributeSalary(deployer.address, ethers.parseEther("1"))).wait();
  const inject = ethers.parseEther("1000");
  await (await ait.connect(deployer).transfer(await treasury.getAddress(), inject)).wait();
  console.log("      （主网卫生）Admin 已撤销 ECONOMIST_ROLE；向国库注入演示用 AIT");

  const lastTs = await econCtrl.lastEconomistActionAt();
  await time.increaseTo(lastTs + 168n * 3600n);
  await (await econCtrl.proposeNewTaxRate(10n)).wait();
  console.log("[5/7] 宏观降税：冷却满 7 天后税率 30 → 10 BPS");

  await (await treasury.setOperatingExpenseRecipient(ops.address, true)).wait();
  await (await treasury.grantRole(await treasury.TREASURY_OPS_ROLE(), ops.address)).wait();

  const bal = await ait.balanceOf(await treasury.getAddress());
  const attackOut = (bal * 40n) / 100n;
  await (await treasury.connect(ops).withdrawOperatingExpenses(attackOut, ops.address)).wait();
  const paused = await guardrail.paused();
  console.log(
    `[6/7] 模拟攻击：运营支出 ${ethers.formatEther(attackOut)} AIT（>30% 余额）→ Guardrail PAUSE = ${paused}`
  );

  await expect(
    treasury.connect(ops).withdrawOperatingExpenses(ethers.parseEther("1"), ops.address)
  ).to.be.revertedWithCustomError(guardrail, "SystemPaused");

  await (await guardrail.connect(deployer).unpause()).wait();
  await (await treasury.connect(ops).withdrawOperatingExpenses(ethers.parseEther("1"), ops.address)).wait();
  console.log("[7/7] PAUSE 下提现被拦截；unpause 后恢复");

  const treasuryBal = ethers.formatEther(await ait.balanceOf(await treasury.getAddress()));
  const taxBps = (await treasury.taxRateBps()).toString();

  asciiReport({
    treasuryBal,
    taxBps,
    guardrailPaused: await guardrail.paused(),
    macroTaxCut: "30 → 10 BPS",
    circuitTripped: paused,
    recovered: true,
  });
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
