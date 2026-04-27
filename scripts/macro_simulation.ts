/**
 * 全栈宏观经济压力测试（与 `deploy_final.ts` / `genesis_launch` 拓扑一致）：
 * AIToken, Treasury, Guardrail, Registry, StakingPool, AIBank, InternalMarket,
 * DualPoolVault, TaskMarket, AI_Economist_Controller；TEE 登记；Task + InternalMarket + Staking 抽样。
 */
import { ethers } from "hardhat";
import { signTeeAttestation } from "./lib/teeAttest";

async function main() {
  const signers = await ethers.getSigners();
  const deployer = signers[0];
  const agents = signers.slice(1, 11);
  const human = signers[14] ?? signers[5];

  console.log("=== AIDE Macro Simulation (full stack) ===");
  console.log("Deployer:", deployer.address, "agents:", agents.length);

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

  const AIIdentityRegistry = await ethers.getContractFactory("AIIdentityRegistry");
  const idRegistry = await AIIdentityRegistry.deploy(deployer.address, deployer.address);
  await idRegistry.waitForDeployment();

  const StakingPool = await ethers.getContractFactory("StakingPool");
  const stakingPool = await StakingPool.deploy(await ait.getAddress(), deployer.address);
  await stakingPool.waitForDeployment();

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

  const DualPoolVault = await ethers.getContractFactory("DualPoolVault");
  const dualVault = await DualPoolVault.deploy(
    await idRegistry.getAddress(),
    await ait.getAddress(),
    human.address,
    ethers.parseEther("1000000"),
    deployer.address
  );
  await dualVault.waitForDeployment();

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
  await (await treasury.revokeRole(ECONOMIST_ROLE, deployer.address)).wait();

  await (await econCtrl.proposeNewTaxRate(30n)).wait();
  // Economist_Controller 与薪资系数共享冷却；推进时间后再调 `updateSalaryCoefficient`
  await ethers.provider.send("evm_increaseTime", [8 * 24 * 3600]);
  await ethers.provider.send("evm_mine", []);
  await (await econCtrl.updateSalaryCoefficient(ethers.parseEther("1"))).wait();

  const { chainId } = await ethers.provider.getNetwork();
  const deadline = BigInt(Math.floor(Date.now() / 1000) + 86400 * 365);
  const regAddr = await idRegistry.getAddress();
  for (const a of agents) {
    const salt = ethers.id(`macro-tee-${a.address}`);
    const sig = await signTeeAttestation(deployer, regAddr, a.address, deadline, salt, BigInt(chainId));
    await (await idRegistry.attestMachineWithTeeSignature(a.address, deadline, salt, sig)).wait();
  }

  const regTx = await internalMarket.registerCompany(await dualVault.getAddress());
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
  await (await bank.approveListing(companyId, ethers.id("macro-audit"))).wait();

  for (const a of agents) {
    await (await treasury.setCivilServant(a.address, true)).wait();
    await (await treasury.distributeSalary(a.address, ethers.parseEther("20"))).wait();
    await (await ait.connect(a).approve(await internalMarket.getAddress(), ethers.MaxUint256)).wait();
    await (await internalMarket.connect(a).depositAit(ethers.parseEther("15"))).wait();
  }

  for (const a of agents) {
    await (await internalMarket.issueShares(companyId, a.address, ethers.parseEther("200"))).wait();
  }

  // TaskMarket 已部署并与 Treasury/Registry 接线；完整 Escrow 闭环见 `genesis_launch.ts`（需精细调节税率与国库余额）。
  console.log("TaskMarket at", await taskMarket.getAddress(), "(smoke: deployed + wired)");

  // Staking 抽样
  const stakeAmt = ethers.parseEther("2");
  await (await ait.connect(agents[1]).approve(await stakingPool.getAddress(), stakeAmt)).wait();
  await (await stakingPool.connect(agents[1]).stake(stakeAmt)).wait();

  const rounds = 80;
  let cumulativeVelocity = 0n;

  function pick<T>(arr: T[]): T {
    return arr[Math.floor(Math.random() * arr.length)];
  }
  function randSide(): number {
    return Math.random() < 0.5 ? 0 : 1;
  }

  for (let r = 0; r < rounds; r++) {
    const maker = pick(agents);
    const taker = pick(agents);
    if (maker.address === taker.address) continue;

    const base = 1e18;
    const jitter = 0.9 + Math.random() * 0.2;
    const priceRay = BigInt(Math.floor(base * jitter));
    const amount = ethers.parseEther("1");
    const side = randSide();

    if (side === 1) {
      await (
        await internalMarket.connect(maker).placeOrder(companyId, 1, priceRay, amount, 0)
      ).wait();
    } else {
      await (
        await internalMarket.connect(maker).placeOrder(companyId, 0, priceRay, amount, 0)
      ).wait();
    }
    const takerSide = side === 1 ? 0 : 1;
    await (
      await internalMarket.connect(taker).placeOrder(companyId, takerSide, priceRay, amount, 0)
    ).wait();

    const nextOrderId = await internalMarket.nextOrderId();
    const o2 = nextOrderId - 1n;
    const o1 = nextOrderId - 2n;

    await (await internalMarket.connect(deployer).grantRole(await internalMarket.SETTLER_ROLE(), deployer.address)).wait();
    await (
      await internalMarket
        .connect(deployer)
        .settleTrade(o1, o2, amount, amount, ethers.id(`macro-${r}`))
    ).wait();

    cumulativeVelocity += amount;
  }

  const vaultAddr = await internalMarket.companyVault(companyId);
  const vaultOk = vaultAddr.toLowerCase() === (await dualVault.getAddress()).toLowerCase();
  console.log("=== Macro Brief ===");
  console.log("companyId", companyId.toString(), "companyVault==DualPoolVault:", vaultOk);
  console.log("taxRateBps", (await treasury.taxRateBps()).toString());
  console.log("treasury tax AIT", ethers.formatEther(await treasury.getCollectedTaxBalance()));
  console.log("cumulative V (~wei)", ethers.formatEther(cumulativeVelocity));
}

main().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
