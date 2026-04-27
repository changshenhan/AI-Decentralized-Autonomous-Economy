/**
 * 生产顺序部署 + Basescan 验证 + 可选权限移交（PRODUCTION_ADMIN_ADDRESS）。
 * 输出 deployments/base_sepolia.json（或 base.json / <network>.json）
 *
 * 用法：
 *   BASE_SEPOLIA_RPC_URL=... DEPLOYER_PRIVATE_KEY=0x... BASESCAN_API_KEY=... \
 *   npx hardhat run scripts/deploy_final.ts --network baseSepolia
 */
import fs from "fs";
import path from "path";
import hre, { ethers, network } from "hardhat";
import { C2_SALT, deployViaCreate2 } from "./lib/create2Deploy";
import { signTeeAttestation } from "./lib/teeAttest";

async function safeVerify(name: string, address: string, constructorArguments: unknown[]) {
  try {
    await hre.run("verify:verify", { address, constructorArguments });
    console.log("[verify:ok]", name, address);
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    console.warn("[verify:skip]", name, msg);
  }
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function transferProductionRoles(deployer: { address: string }, prod: string, contracts: Record<string, any>) {
  const prodAddr = ethers.getAddress(prod);
  if (prodAddr.toLowerCase() === deployer.address.toLowerCase()) {
    console.log("PRODUCTION_ADMIN_ADDRESS equals deployer; skip role transfer");
    return;
  }

  const ait = contracts.ait;
  const treasury = contracts.treasury;
  const guardrail = contracts.guardrail;
  const idRegistry = contracts.idRegistry;
  const staking = contracts.staking;
  const bank = contracts.bank;
  const internalMarket = contracts.internalMarket;
  const taskMarket = contracts.taskMarket;
  const econCtrl = contracts.econCtrl;
  const dualVault = contracts.dualVault;

  console.log("=== Transferring admin roles to production address", prodAddr);

  await (await ait.transferOwnership(prodAddr)).wait();

  const TR_ADMIN = await treasury.DEFAULT_ADMIN_ROLE();
  const TR_REG = await treasury.REGISTRY_ROLE();
  const TR_SAL = await treasury.SALARY_ROLE();
  const TR_OPS = await treasury.TREASURY_OPS_ROLE();
  await (await treasury.grantRole(TR_ADMIN, prodAddr)).wait();
  await (await treasury.grantRole(TR_REG, prodAddr)).wait();
  await (await treasury.grantRole(TR_SAL, prodAddr)).wait();
  await (await treasury.grantRole(TR_OPS, prodAddr)).wait();
  await (await treasury.revokeRole(TR_ADMIN, deployer.address)).wait();
  await (await treasury.revokeRole(TR_REG, deployer.address)).wait();
  await (await treasury.revokeRole(TR_SAL, deployer.address)).wait();
  await (await treasury.revokeRole(TR_OPS, deployer.address)).wait();

  const GR_ADM = await guardrail.DEFAULT_ADMIN_ROLE();
  const GR_EM = await guardrail.EMERGENCY_COMMITTEE_ROLE();
  await (await guardrail.grantRole(GR_ADM, prodAddr)).wait();
  await (await guardrail.grantRole(GR_EM, prodAddr)).wait();
  await (await guardrail.revokeRole(GR_ADM, deployer.address)).wait();
  await (await guardrail.revokeRole(GR_EM, deployer.address)).wait();

  const IR_ADM = await idRegistry.DEFAULT_ADMIN_ROLE();
  const IR_VER = await idRegistry.VERIFIER_ROLE();
  await (await idRegistry.grantRole(IR_ADM, prodAddr)).wait();
  await (await idRegistry.grantRole(IR_VER, prodAddr)).wait();
  await (await idRegistry.revokeRole(IR_ADM, deployer.address)).wait();
  await (await idRegistry.revokeRole(IR_VER, deployer.address)).wait();

  await (await staking.transferOwnership(prodAddr)).wait();

  const BK_ADM = await bank.DEFAULT_ADMIN_ROLE();
  const BK_AUD = await bank.AUDITOR_ROLE();
  await (await bank.grantRole(BK_ADM, prodAddr)).wait();
  await (await bank.grantRole(BK_AUD, prodAddr)).wait();
  await (await bank.revokeRole(BK_ADM, deployer.address)).wait();
  await (await bank.revokeRole(BK_AUD, deployer.address)).wait();

  const IM_ADM = await internalMarket.DEFAULT_ADMIN_ROLE();
  const IM_SET = await internalMarket.SETTLER_ROLE();
  const IM_CO = await internalMarket.COMPANY_ADMIN_ROLE();
  await (await internalMarket.grantRole(IM_ADM, prodAddr)).wait();
  await (await internalMarket.grantRole(IM_SET, prodAddr)).wait();
  await (await internalMarket.grantRole(IM_CO, prodAddr)).wait();
  await (await internalMarket.revokeRole(IM_ADM, deployer.address)).wait();
  await (await internalMarket.revokeRole(IM_SET, deployer.address)).wait();
  await (await internalMarket.revokeRole(IM_CO, deployer.address)).wait();

  await (await taskMarket.transferOwnership(prodAddr)).wait();
  await (await econCtrl.transferOwnership(prodAddr)).wait();
  await (await dualVault.transferOwnership(prodAddr)).wait();

  console.log("=== Production role transfer done");
}

async function main() {
  const [deployer] = await ethers.getSigners();
  const net = await ethers.provider.getNetwork();
  const chainId = net.chainId;

  console.log("=== deploy_final === network", network.name, "chainId", chainId.toString());

  const useC2 = process.env.AIDE_USE_CREATE2 === "1" || process.env.AIDE_USE_CREATE2 === "true";

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let ait: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let guardrail: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let treasury: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let bank: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let internalMarket: any;

  if (useC2) {
    console.log("=== AIDE_USE_CREATE2: salted deployment (Guardrail, AIToken, AIBank, Treasury, InternalMarket) ===");
    const C2F = await ethers.getContractFactory("AideCreate2Factory");
    const existingFactory = process.env.CREATE2_FACTORY_ADDRESS?.trim();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    let c2: any;
    if (existingFactory && ethers.isAddress(existingFactory)) {
      c2 = C2F.attach(existingFactory);
    } else {
      c2 = await C2F.deploy();
      await c2.waitForDeployment();
      console.log("AideCreate2Factory:", await c2.getAddress());
    }

    const AITokenF = await ethers.getContractFactory("AIToken");
    const GuardrailF = await ethers.getContractFactory("Guardrail");
    const TreasuryF = await ethers.getContractFactory("Treasury");
    const AIBankF = await ethers.getContractFactory("AIBank");
    const InternalMarketF = await ethers.getContractFactory("InternalMarket");

    const grAddr = await deployViaCreate2(c2, GuardrailF, C2_SALT.guardrail, [deployer.address]);
    const aitAddr = await deployViaCreate2(c2, AITokenF, C2_SALT.aitoken, [deployer.address]);
    const bankAddr = await deployViaCreate2(c2, AIBankF, C2_SALT.bank, [deployer.address]);
    const treasuryAddr = await deployViaCreate2(c2, TreasuryF, C2_SALT.treasury, [
      deployer.address,
      aitAddr,
      grAddr,
    ]);
    const imAddr = await deployViaCreate2(c2, InternalMarketF, C2_SALT.internalMarket, [
      aitAddr,
      treasuryAddr,
      bankAddr,
      deployer.address,
      grAddr,
    ]);

    ait = await ethers.getContractAt("AIToken", aitAddr);
    guardrail = await ethers.getContractAt("Guardrail", grAddr);
    treasury = await ethers.getContractAt("Treasury", treasuryAddr);
    bank = await ethers.getContractAt("AIBank", bankAddr);
    internalMarket = await ethers.getContractAt("InternalMarket", imAddr);

    await safeVerify("AIToken", aitAddr, [deployer.address]);
    await safeVerify("Guardrail", grAddr, [deployer.address]);
    await safeVerify("Treasury", treasuryAddr, [deployer.address, aitAddr, grAddr]);
    await safeVerify("AIBank", bankAddr, [deployer.address]);
    await safeVerify("InternalMarket", imAddr, [aitAddr, treasuryAddr, bankAddr, deployer.address, grAddr]);
  } else {
    const AIToken = await ethers.getContractFactory("AIToken");
    ait = await AIToken.deploy(deployer.address);
    await ait.waitForDeployment();
    await safeVerify("AIToken", await ait.getAddress(), [deployer.address]);

    const Guardrail = await ethers.getContractFactory("Guardrail");
    guardrail = await Guardrail.deploy(deployer.address);
    await guardrail.waitForDeployment();
    await safeVerify("Guardrail", await guardrail.getAddress(), [deployer.address]);

    const Treasury = await ethers.getContractFactory("Treasury");
    treasury = await Treasury.deploy(deployer.address, await ait.getAddress(), await guardrail.getAddress());
    await treasury.waitForDeployment();
    await safeVerify("Treasury", await treasury.getAddress(), [
      deployer.address,
      await ait.getAddress(),
      await guardrail.getAddress(),
    ]);

    const AIBank = await ethers.getContractFactory("AIBank");
    bank = await AIBank.deploy(deployer.address);
    await bank.waitForDeployment();
    await safeVerify("AIBank", await bank.getAddress(), [deployer.address]);

    const InternalMarket = await ethers.getContractFactory("InternalMarket");
    internalMarket = await InternalMarket.deploy(
      await ait.getAddress(),
      await treasury.getAddress(),
      await bank.getAddress(),
      deployer.address,
      await guardrail.getAddress()
    );
    await internalMarket.waitForDeployment();
    await safeVerify("InternalMarket", await internalMarket.getAddress(), [
      await ait.getAddress(),
      await treasury.getAddress(),
      await bank.getAddress(),
      deployer.address,
      await guardrail.getAddress(),
    ]);
  }

  await (await ait.setTreasury(await treasury.getAddress())).wait();

  const teeAuthority = process.env.TEE_AUTHORITY_ADDRESS
    ? ethers.getAddress(process.env.TEE_AUTHORITY_ADDRESS)
    : deployer.address;

  const AIIdentityRegistry = await ethers.getContractFactory("AIIdentityRegistry");
  const idRegistry = await AIIdentityRegistry.deploy(deployer.address, teeAuthority);
  await idRegistry.waitForDeployment();
  await safeVerify("AIIdentityRegistry", await idRegistry.getAddress(), [deployer.address, teeAuthority]);

  const StakingPool = await ethers.getContractFactory("StakingPool");
  const stakingPool = await StakingPool.deploy(await ait.getAddress(), deployer.address);
  await stakingPool.waitForDeployment();
  await safeVerify("StakingPool", await stakingPool.getAddress(), [await ait.getAddress(), deployer.address]);

  const rawHuman = process.env.HUMAN_DIVIDEND_EOA?.trim();
  const humanDiv = rawHuman ? ethers.getAddress(rawHuman) : deployer.address;

  const DualPoolVault = await ethers.getContractFactory("DualPoolVault");
  const financingThreshold = ethers.parseEther(process.env.FINANCING_THRESHOLD_AIT || "1000000");
  const dualVault = await DualPoolVault.deploy(
    await idRegistry.getAddress(),
    await ait.getAddress(),
    humanDiv,
    financingThreshold,
    deployer.address
  );
  await dualVault.waitForDeployment();
  await safeVerify("DualPoolVault", await dualVault.getAddress(), [
    await idRegistry.getAddress(),
    await ait.getAddress(),
    humanDiv,
    financingThreshold,
    deployer.address,
  ]);

  await (await guardrail.setPeers(await treasury.getAddress(), await internalMarket.getAddress())).wait();

  await (await internalMarket.registerCompany(await dualVault.getAddress())).wait();

  const TaskMarket = await ethers.getContractFactory("TaskMarket");
  const taskMarket = await TaskMarket.deploy(
    await ait.getAddress(),
    await treasury.getAddress(),
    await idRegistry.getAddress(),
    deployer.address,
    await guardrail.getAddress()
  );
  await taskMarket.waitForDeployment();
  await safeVerify("TaskMarket", await taskMarket.getAddress(), [
    await ait.getAddress(),
    await treasury.getAddress(),
    await idRegistry.getAddress(),
    deployer.address,
    await guardrail.getAddress(),
  ]);

  const EconCtrl = await ethers.getContractFactory("AI_Economist_Controller");
  const econCtrl = await EconCtrl.deploy(await treasury.getAddress(), deployer.address);
  await econCtrl.waitForDeployment();
  await safeVerify("AI_Economist_Controller", await econCtrl.getAddress(), [
    await treasury.getAddress(),
    deployer.address,
  ]);

  await (await ait.setTaxExemptSender(await internalMarket.getAddress(), true)).wait();
  await (await ait.setTaxExemptSender(await taskMarket.getAddress(), true)).wait();

  const ECONOMIST_ROLE = await treasury.ECONOMIST_ROLE();
  await (await treasury.grantRole(ECONOMIST_ROLE, await econCtrl.getAddress())).wait();
  await (await treasury.revokeRole(ECONOMIST_ROLE, deployer.address)).wait();

  const deployment: Record<string, string> = {
    network: network.name,
    chainId: chainId.toString(),
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
    humanDividendRecipient: humanDiv,
  };

  const outDir = path.join(__dirname, "..", "deployments");
  fs.mkdirSync(outDir, { recursive: true });
  const fname =
    network.name === "baseSepolia"
      ? "base_sepolia.json"
      : network.name === "base"
        ? "base.json"
        : `${network.name}.json`;
  const outFile = path.join(outDir, fname);
  fs.writeFileSync(outFile, JSON.stringify(deployment, null, 2));
  console.log("Wrote", outFile);

  const prod = process.env.PRODUCTION_ADMIN_ADDRESS?.trim();
  if (prod && ethers.isAddress(prod)) {
    await transferProductionRoles(deployer, prod, {
      ait,
      treasury,
      guardrail,
      idRegistry,
      staking: stakingPool,
      bank,
      internalMarket,
      taskMarket,
      econCtrl,
      dualVault,
    });
  }

  const teePk = process.env.TEE_SIGNER_PRIVATE_KEY?.trim();
  if (teePk) {
    const teeSigner = new ethers.Wallet(teePk, ethers.provider);
    if (teeSigner.address.toLowerCase() !== teeAuthority.toLowerCase()) {
      throw new Error("TEE_SIGNER_PRIVATE_KEY 地址与 TEE_AUTHORITY_ADDRESS / deployer 不一致");
    }
    const regAddr = await idRegistry.getAddress();
    const deadline = BigInt(Math.floor(Date.now() / 1000) + 86400 * 365);
    const salt = ethers.id("deploy-final-bootstrap");
    const sig = await signTeeAttestation(teeSigner, regAddr, deployer.address, deadline, salt, BigInt(chainId));
    await (await idRegistry.attestMachineWithTeeSignature(deployer.address, deadline, salt, sig)).wait();
    console.log("Bootstrap attestMachineWithTeeSignature for deployer OK");
  }
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
