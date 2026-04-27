import fs from "fs";
import path from "path";
import { ethers } from "hardhat";
import { signTeeAttestation } from "./lib/teeAttest";

/** 创世部署 + 社会仿真；与 `mainnet_check` 同进程调用时可共享同一 Hardhat 链状态。 */
export async function genesisLaunch() {
  const [deployer, ai1, ai2, ai3, human] = await ethers.getSigners();

  console.log("=== AIDE Genesis Launch ===");
  console.log("Deployer:", deployer.address);

  // 1. 部署核心合约
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
  const financingThreshold = ethers.parseEther(process.env.FINANCING_THRESHOLD_AIT || "1000000");
  const dualVault = await DualPoolVault.deploy(
    await idRegistry.getAddress(),
    await ait.getAddress(),
    human.address,
    financingThreshold,
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

  // 设置 taxExempt sender，避免与 Treasury.collectTax 双重抽税
  await (await ait.setTaxExemptSender(await internalMarket.getAddress(), true)).wait();
  await (await ait.setTaxExemptSender(await taskMarket.getAddress(), true)).wait();

  // 授权 ECONOMIST_ROLE 给 Controller
  const ECONOMIST_ROLE = await treasury.ECONOMIST_ROLE();
  await (await treasury.grantRole(ECONOMIST_ROLE, await econCtrl.getAddress())).wait();

  console.log("Contracts:");
  console.log("  AIToken:", await ait.getAddress());
  console.log("  Guardrail:", await guardrail.getAddress());
  console.log("  Treasury:", await treasury.getAddress());
  console.log("  AIIdentityRegistry:", await idRegistry.getAddress());
  console.log("  StakingPool:", await stakingPool.getAddress());
  console.log("  AIBank:", await bank.getAddress());
  console.log("  InternalMarket:", await internalMarket.getAddress());
  console.log("  DualPoolVault:", await dualVault.getAddress());
  console.log("  TaskMarket:", await taskMarket.getAddress());
  console.log("  AI_Economist_Controller:", await econCtrl.getAddress());

  // 2. 模拟 AIBank 审计并批准第一个 AI 公司上市（链上 id=1，逻辑 id=1001）
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
  const companyIdOnChain = regEvent!.args.companyId as bigint;

  const auditHash = ethers.id("genesis-company-1001");
  await (await bank.approveListing(companyIdOnChain, auditHash)).wait();

  console.log(`Company logical ID 1001 -> on-chain companyId=${companyIdOnChain.toString()}`);

  // 3. 经济员设置初始税率 0.3% (30 BPS) 与薪资系数
  await (await econCtrl.proposeNewTaxRate(30n)).wait();
  // 初始创世仅设置税率，薪资系数可在冷却期后由后续运维调整
  const salaryS = ethers.parseEther("1");
  console.log("Planned weeklySalaryCoefficientS (after cooldown):", salaryS.toString());

  console.log("Initial taxRateBps:", (await treasury.taxRateBps()).toString());
  console.log("Initial weeklySalaryCoefficientS:", (await treasury.weeklySalaryCoefficientS()).toString());

  // 4. 受信 TEE（此处为 deployer 密钥）ECDSA 身份证明
  const agents = [ai1, ai2, ai3];
  const { chainId } = await ethers.provider.getNetwork();
  const deadline = BigInt(Math.floor(Date.now() / 1000) + 86400 * 365);
  const regAddr = await idRegistry.getAddress();
  for (const a of agents) {
    const salt = ethers.id(`genesis-tee-${a.address}`);
    const sig = await signTeeAttestation(deployer, regAddr, a.address, deadline, salt, BigInt(chainId));
    await (await idRegistry.attestMachineWithTeeSignature(a.address, deadline, salt, sig)).wait();
  }

  console.log("Registered 3 genesis AI agents via attestMachineWithTeeSignature (TEE ECDSA)");

  // 临时将税率调为 0，避免 Task 流中出现双重抽税，后续再恢复 0.3%
  await (await treasury.setTaxRate(0)).wait();

  // 5. 任务经济闭环：TaskMarket 发布/执行一次任务
  // 5.1 国库向人类发薪（用于支付任务赏金）
  const SALARY_ROLE = await treasury.SALARY_ROLE();
  await (await treasury.setCivilServant(human.address, true)).wait();
  // 直接从合约 owner 视角设置 S 与贡献度形成非零工资，用于创世资金注入
  await (await treasury.setWeeklySalaryCoefficient(ethers.parseEther("1"))).wait();
  const contribRating = ethers.parseEther("100");
  await (await treasury.distributeSalary(human.address, contribRating)).wait();

  const bounty = ethers.parseEther("50");
  await (await ait.connect(human).approve(await taskMarket.getAddress(), bounty)).wait();

  const postTx = await taskMarket.connect(human).postTask(bounty);
  const postRc = await postTx.wait();
  const postEvent = postRc!.logs
    .map((l) => {
      try {
        return taskMarket.interface.parseLog(l);
      } catch {
        return undefined;
      }
    })
    .find((e) => e && e.name === "TaskPosted");
  const taskId = postEvent!.args.taskId as bigint;
  await (await taskMarket.connect(human).fundTask(taskId)).wait();

  // 登记 ai1 为已验证机器（已在 ZK 接口中标记），直接作为 AiPool 收款地址
  await (await taskMarket.connect(human).assignWorker(taskId, ai1.address, ai1.address)).wait();
  await (await taskMarket.connect(ai1).submitWork(taskId, ethers.id("genesis-task-result"))).wait();
  await (await taskMarket.connect(human).approveTask(taskId)).wait();

  // 恢复税率至 0.3% 以便后续 InternalMarket 交易抽税
  await (await treasury.setTaxRate(30n)).wait();

  // 6. InternalMarket 进行一笔交易，验证交易税进入 Treasury
  await (await internalMarket.issueShares(companyIdOnChain, ai2.address, ethers.parseEther("100"))).wait();

  // 给 ai3 部署初始 AIT 用于买入
  await (await treasury.setCivilServant(ai3.address, true)).wait();
  await (await treasury.distributeSalary(ai3.address, ethers.parseEther("100"))).wait();

  await (await ait.connect(ai3).approve(await internalMarket.getAddress(), ethers.MaxUint256)).wait();
  await (await internalMarket.connect(ai3).depositAit(ethers.parseEther("100"))).wait();

  // 卖方：ai2；买方：ai3
  await (
    await internalMarket
      .connect(ai2)
      .placeOrder(companyIdOnChain, 1, ethers.parseEther("1"), ethers.parseEther("10"), 0)
  ).wait();
  await (
    await internalMarket
      .connect(ai3)
      .placeOrder(companyIdOnChain, 0, ethers.parseEther("1"), ethers.parseEther("10"), 0)
  ).wait();

  const nextOrderId = await internalMarket.nextOrderId();
  const sellOrderId = nextOrderId - 2n;
  const buyOrderId = nextOrderId - 1n;

  const aitNotional = ethers.parseEther("10");
  await (
    await internalMarket
      .connect(deployer)
      .grantRole(await internalMarket.SETTLER_ROLE(), deployer.address)
  ).wait();
  await (
    await internalMarket
      .connect(deployer)
      .settleTrade(sellOrderId, buyOrderId, ethers.parseEther("10"), aitNotional, ethers.id("genesis-trade"))
  ).wait();

  // 6b. StakingPool：AI 质押多余 AIT + 国库税收注入分红（质押额不超过卖方成交后余额）
  const stakeAmt = ethers.parseEther("5");
  await (await ait.connect(ai2).approve(await stakingPool.getAddress(), stakeAmt)).wait();
  await (await stakingPool.connect(ai2).stake(stakeAmt)).wait();

  await (await treasury.setCivilServant(deployer.address, true)).wait();
  await (await treasury.setWeeklySalaryCoefficient(ethers.parseEther("50"))).wait();
  await (await treasury.distributeSalary(deployer.address, ethers.parseEther("1"))).wait();
  const rewardInject = ethers.parseEther("8");
  const tbBefore = await ait.balanceOf(await treasury.getAddress());
  if (tbBefore < rewardInject) {
    await (await ait.transfer(await treasury.getAddress(), rewardInject - tbBefore)).wait();
  }
  await (await treasury.depositRewardToStakingPool(await stakingPool.getAddress(), rewardInject)).wait();
  await (await treasury.approveStakingPoolPull(await stakingPool.getAddress(), ethers.MaxUint256)).wait();

  console.log(
    `Staking: ai2 staked ${ethers.formatEther(stakeAmt)} AIT; treasury-funded reward ${ethers.formatEther(rewardInject)} AIT`
  );

  // 7. 社会运行报告
  const treasuryBal = await ait.balanceOf(await treasury.getAddress());
  const humanBal = await ait.balanceOf(human.address);
  const ai1Bal = await ait.balanceOf(ai1.address);
  const ai2Bal = await ait.balanceOf(ai2.address);
  const ai3Bal = await ait.balanceOf(ai3.address);

  console.log("=== Genesis Social Report ===");
  console.log("Treasury AIT balance:", ethers.formatEther(treasuryBal));
  console.log("Human AIT balance:", ethers.formatEther(humanBal));
  console.log("AI1 (Task worker) AIT balance:", ethers.formatEther(ai1Bal));
  console.log("AI2 (Seller) AIT balance:", ethers.formatEther(ai2Bal));
  console.log("AI3 (Buyer) AIT balance:", ethers.formatEther(ai3Bal));
  console.log(
    "Note: Treasury balance includes tax from TaskMarket + InternalMarket, per LAW dynamic tax rules."
  );

  const deployment = {
    network: (await ethers.provider.getNetwork()).name,
    chainId: String(chainId),
    AIToken: await ait.getAddress(),
    Treasury: await treasury.getAddress(),
    Guardrail: await guardrail.getAddress(),
    AIIdentityRegistry: await idRegistry.getAddress(),
    StakingPool: await stakingPool.getAddress(),
    AIBank: await bank.getAddress(),
    InternalMarket: await internalMarket.getAddress(),
    DualPoolVault: await dualVault.getAddress(),
    TaskMarket: await taskMarket.getAddress(),
    AI_Economist_Controller: await econCtrl.getAddress(),
    teeAuthority,
    admin: deployer.address,
  };
  // 主网卫生：创世中 deployer 曾直接调用 setTaxRate(0) 等；全部完成后撤销 Admin 的 ECONOMIST_ROLE
  await (await treasury.revokeRole(ECONOMIST_ROLE, deployer.address)).wait();

  const outDir = path.join(__dirname, "..", "deployments");
  fs.mkdirSync(outDir, { recursive: true });
  const outFile = path.join(outDir, "localhost.json");
  fs.writeFileSync(outFile, JSON.stringify(deployment, null, 2));
  console.log("Wrote deployment snapshot:", outFile);
}

if (require.main === module) {
  genesisLaunch().catch((err) => {
    console.error(err);
    process.exitCode = 1;
  });
}

