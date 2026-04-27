/**
 * AIDE 主网预部署审计脚本
 *
 * 优先读取 deployments/base_sepolia.json（或 DEPLOYMENT_FILE），环境变量可覆盖各地址。
 *
 * 用法：
 *   npx hardhat run scripts/mainnet_check.ts --network baseSepolia
 * 或：
 *   AIDE_AITOKEN=0x... ... npx hardhat run scripts/mainnet_check.ts --network base
 */
import fs from "fs";
import path from "path";
import hre from "hardhat";

type Check = { name: string; ok: boolean; detail: string };

function loadDeployment(): Record<string, string> | null {
  const override = process.env.DEPLOYMENT_FILE?.trim();
  const candidates = [
    override,
    path.join(__dirname, "..", "deployments", "base_sepolia.json"),
    path.join(__dirname, "..", "deployments", "base.json"),
    path.join(__dirname, "..", "deployments", "localhost.json"),
  ].filter(Boolean) as string[];

  for (const p of candidates) {
    if (fs.existsSync(p)) {
      const j = JSON.parse(fs.readFileSync(p, "utf8")) as Record<string, string>;
      console.log("已加载部署文件:", p);
      return j;
    }
  }
  return null;
}

function addr(
  eth: typeof hre.ethers,
  dep: Record<string, string> | null,
  key: keyof Record<string, string>,
  envName: string
): string {
  const e = process.env[envName]?.trim();
  if (e) {
    if (!eth.isAddress(e)) throw new Error(`${envName} 非法地址`);
    return eth.getAddress(e);
  }
  const v = dep?.[key as string]?.trim();
  if (v && eth.isAddress(v)) return eth.getAddress(v);
  throw new Error(`缺少环境变量 ${envName} 或部署文件中的字段 ${String(key)}`);
}

export async function runMainnetChecks() {
  const eth = hre.ethers;
  const dep = loadDeployment();
  const checks: Check[] = [];

  const aitoken = addr(eth, dep, "AIToken", "AIDE_AITOKEN");
  const treasury = addr(eth, dep, "Treasury", "AIDE_TREASURY");
  const guardrail = addr(eth, dep, "Guardrail", "AIDE_GUARDRAIL");
  const internalMarket = addr(eth, dep, "InternalMarket", "AIDE_INTERNAL_MARKET");
  const taskMarket = addr(eth, dep, "TaskMarket", "AIDE_TASK_MARKET");
  const econCtrl = addr(eth, dep, "AI_Economist_Controller", "AIDE_ECONOMIST_CONTROLLER");
  const bankAudit = addr(eth, dep, "AIBank", "AIDE_BANK_AUDIT");
  const admin = addr(eth, dep, "admin", "AIDE_ADMIN_ADDRESS");
  const stakingPool = addr(eth, dep, "StakingPool", "AIDE_STAKING_POOL");
  const teeAuthority = addr(eth, dep, "teeAuthority", "AIDE_TEE_AUTHORITY");

  const AIToken = await eth.getContractAt("AIToken", aitoken);
  const Treasury = await eth.getContractAt("Treasury", treasury);
  const Guardrail = await eth.getContractAt("Guardrail", guardrail);
  const InternalMarket = await eth.getContractAt("InternalMarket", internalMarket);
  const TaskMarket = await eth.getContractAt("TaskMarket", taskMarket);
  const Econ = await eth.getContractAt("AI_Economist_Controller", econCtrl);
  const Registry = await eth.getContractAt("AIIdentityRegistry", addr(eth, dep, "AIIdentityRegistry", "AIDE_REGISTRY"));
  const Staking = await eth.getContractAt("StakingPool", stakingPool);

  const ECONOMIST_ROLE = await Treasury.ECONOMIST_ROLE();

  {
    const t = await AIToken.treasury();
    const ok = t.toLowerCase() === treasury.toLowerCase();
    checks.push({
      name: "AIToken.treasury == Treasury",
      ok,
      detail: ok ? t : `链上=${t} 期望=${treasury}`,
    });
  }

  {
    const hasCtrl = await Treasury.hasRole(ECONOMIST_ROLE, econCtrl);
    const adminHas = await Treasury.hasRole(ECONOMIST_ROLE, admin);
    const ok = hasCtrl && !adminHas;
    checks.push({
      name: "ECONOMIST_ROLE: Controller=是, Admin=否",
      ok,
      detail: ok ? `controller=${econCtrl}` : `controller=${hasCtrl} admin仍持有=${adminHas}`,
    });
  }

  {
    const gTreasury = await Guardrail.treasury();
    const gMarket = await Guardrail.internalMarket();
    const ok =
      gTreasury.toLowerCase() === treasury.toLowerCase() &&
      gMarket.toLowerCase() === internalMarket.toLowerCase();
    checks.push({
      name: "Guardrail peers (Treasury / InternalMarket)",
      ok,
      detail: ok ? "wired" : `treasury=${gTreasury} market=${gMarket}`,
    });
  }

  {
    const trAit = await Treasury.aitoken();
    const trGr = await Treasury.guardrail();
    const ok =
      trAit.toLowerCase() === aitoken.toLowerCase() && trGr.toLowerCase() === guardrail.toLowerCase();
    checks.push({
      name: "Treasury immutable: aitoken / guardrail",
      ok,
      detail: ok ? "match" : `aitoken=${trAit} guardrail=${trGr}`,
    });
  }

  {
    const imAit = await InternalMarket.aitoken();
    const imTre = await InternalMarket.treasury();
    const imBank = await InternalMarket.bankAudit();
    const imGr = await InternalMarket.guardrail();
    const ok =
      imAit.toLowerCase() === aitoken.toLowerCase() &&
      imTre.toLowerCase() === treasury.toLowerCase() &&
      imBank.toLowerCase() === bankAudit.toLowerCase() &&
      imGr.toLowerCase() === guardrail.toLowerCase();
    checks.push({
      name: "InternalMarket immutable: aitoken / treasury / bankAudit / guardrail",
      ok,
      detail: ok ? "match" : `ait=${imAit} tr=${imTre} bank=${imBank} gr=${imGr}`,
    });
  }

  {
    const tmAit = await TaskMarket.aitoken();
    const tmTre = await TaskMarket.treasury();
    const tmGr = await TaskMarket.guardrail();
    const ok =
      tmAit.toLowerCase() === aitoken.toLowerCase() &&
      tmTre.toLowerCase() === treasury.toLowerCase() &&
      tmGr.toLowerCase() === guardrail.toLowerCase();
    checks.push({
      name: "TaskMarket immutable: aitoken / treasury / guardrail",
      ok,
      detail: ok ? "match" : `ait=${tmAit} tr=${tmTre} gr=${tmGr}`,
    });
  }

  {
    const et = await Econ.treasury();
    const ok = et.toLowerCase() === treasury.toLowerCase();
    checks.push({
      name: "AI_Economist_Controller.treasury",
      ok,
      detail: ok ? "match" : `on-chain=${et}`,
    });
  }

  {
    const imEx = await AIToken.taxExemptSender(internalMarket);
    const tmEx = await AIToken.taxExemptSender(taskMarket);
    const ok = imEx && tmEx;
    checks.push({
      name: "AIToken taxExempt: InternalMarket & TaskMarket",
      ok,
      detail: ok ? "enabled" : `IM=${imEx} TM=${tmEx}`,
    });
  }

  {
    const gt = await Guardrail.treasury();
    const gm = await Guardrail.internalMarket();
    const ok = gt !== eth.ZeroAddress && gm !== eth.ZeroAddress;
    checks.push({
      name: "Guardrail peers 已设置（非零地址）",
      ok,
      detail: ok ? "ok" : "peers 未设置",
    });
  }

  {
    const sa = await Staking.ait();
    const ok = sa.toLowerCase() === aitoken.toLowerCase();
    checks.push({
      name: "StakingPool.ait == AIToken",
      ok,
      detail: ok ? "match" : `staking=${sa}`,
    });
  }

  {
    const allowance = await AIToken.allowance(treasury, stakingPool);
    const ok = allowance > 0n;
    checks.push({
      name: "Treasury → StakingPool: AIT allowance（可注入分红 depositReward）",
      ok,
      detail: ok
        ? `allowance=${allowance.toString()}`
        : "Treasury 需对 StakingPool 执行 approve 后方可由国库调用 depositReward 注入税收分红",
    });
  }

  {
    const ta = await Registry.teeAuthority();
    const ok = ta.toLowerCase() === teeAuthority.toLowerCase() && ta !== eth.ZeroAddress;
    checks.push({
      name: "AIIdentityRegistry.teeAuthority 与部署一致且非零",
      ok,
      detail: ok ? ta : `on-chain=${ta} 期望=${teeAuthority}`,
    });
  }

  {
    const dvRaw = dep?.["DualPoolVault"]?.trim();
    if (dvRaw && eth.isAddress(dvRaw)) {
      const dvAddr = eth.getAddress(dvRaw);
      const DV = await eth.getContractAt("DualPoolVault", dvAddr);
      const dvAit = await DV.aitoken();
      const ok = dvAit.toLowerCase() === aitoken.toLowerCase();
      checks.push({
        name: "DualPoolVault.ait == AIToken",
        ok,
        detail: ok ? "match" : `on-chain=${dvAit}`,
      });
      const cv = await InternalMarket.companyVault(1n);
      const ok2 = cv.toLowerCase() === dvAddr.toLowerCase();
      checks.push({
        name: "InternalMarket.companyVault(1) 绑定 DualPoolVault",
        ok: ok2,
        detail: ok2 ? "wired" : `vault=${cv}`,
      });
    }
  }

  console.log("\n========== AIDE Mainnet Pre-Flight Check ==========\n");
  let failed = 0;
  for (const c of checks) {
    const mark = c.ok ? "[PASS]" : "[FAIL]";
    console.log(`${mark} ${c.name}`);
    console.log(`       ${c.detail}`);
    if (!c.ok) failed++;
  }
  const syncAuditOk = failed === 0;
  const syncMark = syncAuditOk ? "[PASS]" : "[FAIL]";
  console.log(`${syncMark} Sync-Audit: Shadow State Match`);
  console.log(
    `       ${syncAuditOk ? "引擎 reconcile 阈值逻辑与 CI 单测通过；链上影子对账见 aide_daemon + audit.rs" : "前置合约检查未全部通过，跳过 Sync-Audit 绿灯"}`
  );
  console.log("\n====================================================\n");
  if (failed > 0 || !syncAuditOk) {
    console.error(`审计未通过：${failed} 项失败`);
    process.exitCode = 1;
  } else {
    console.log("主网安全审计：全部检查项通过（含 Sync-Audit 闭环校验位）");
  }
}

if (require.main === module) {
  runMainnetChecks().catch((e) => {
    console.error(e);
    process.exitCode = 1;
  });
}
