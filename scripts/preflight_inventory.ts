/**
 * 上链 / 创世前「弹药」与地面检查：Deployer Gas、宪法约束下的税率区间、与 genesis_launch 参考经济参数对齐摘要。
 *
 *   npx hardhat run scripts/preflight_inventory.ts --network hardhat
 *   BASE_MAINNET_RPC_URL=... DEPLOYER_PRIVATE_KEY=0x... npx hardhat run scripts/preflight_inventory.ts --network base
 */
import fs from "fs";
import path from "path";
import hre from "hardhat";

const CONSTITUTION_TAX_MIN_BPS = 10n; // 0.1%
const CONSTITUTION_TAX_MAX_BPS = 500n; // 5%

/** 与 `scripts/genesis_launch.ts` 注释一致的参考值（非强制主网数额，作清单对照） */
const REFERENCE_GENESIS = {
  stakingTreasuryRewardInject_AIT: "8",
  genesisInternalMarketDeposit_AIT: "100",
  genesisStakeSample_AIT: "5",
  initialTaxRateBpsAfterRestore: "30",
};

function loadDeployment(): Record<string, string> | null {
  const override = process.env.DEPLOYMENT_FILE?.trim();
  const candidates = [
    override,
    path.join(__dirname, "..", "deployments", "base.json"),
    path.join(__dirname, "..", "deployments", "base_sepolia.json"),
    path.join(__dirname, "..", "deployments", "localhost.json"),
  ].filter(Boolean) as string[];
  for (const p of candidates) {
    if (fs.existsSync(p)) {
      return JSON.parse(fs.readFileSync(p, "utf8")) as Record<string, string>;
    }
  }
  return null;
}

async function main() {
  const eth = hre.ethers;
  const lines: string[] = [];

  lines.push("======================================================================");
  lines.push(" AIDE GENESIS PREFLIGHT INVENTORY");
  lines.push(` Generated: ${new Date().toISOString()}`);
  lines.push("======================================================================");
  lines.push("");

  const minWeiEnv = process.env.MIN_DEPLOYER_WEI?.trim();
  const minWei = minWeiEnv ? BigInt(minWeiEnv) : eth.parseEther(process.env.MIN_DEPLOYER_ETH || "0.05");

  let deployerAddr: string;
  if (process.env.DEPLOYER_ADDRESS?.trim()) {
    deployerAddr = eth.getAddress(process.env.DEPLOYER_ADDRESS.trim());
  } else {
    const signers = await eth.getSigners();
    deployerAddr = signers[0].address;
  }

  const bal = await eth.provider.getBalance(deployerAddr);
  const gasOk = bal >= minWei;
  lines.push("--- 1. Deployer 原生币（Base Gas）---");
  lines.push(`Deployer address:     ${deployerAddr}`);
  lines.push(`Current balance:      ${eth.formatEther(bal)} ETH`);
  lines.push(`Required (minimum):   ${eth.formatEther(minWei)} ETH`);
  lines.push(`Status:               ${gasOk ? "[PASS]" : "[FAIL] 余额不足以支付部署与 setPeers 等初始交易"}`);
  lines.push("");

  lines.push("--- 2. AIDE_CONSTITUTION.md 规则（税率区间）---");
  lines.push(
    `宪法第二章：链上税率须在 [${CONSTITUTION_TAX_MIN_BPS}, ${CONSTITUTION_TAX_MAX_BPS}] BPS（0.1% ~ 5%），由 AI_Economist_Controller 治理。`
  );
  lines.push("");

  const dep = loadDeployment();
  if (dep?.Treasury?.trim()) {
    const treasuryAddr = eth.getAddress(dep.Treasury.trim());
    const Treasury = await eth.getContractAt("Treasury", treasuryAddr);
    const bps = await Treasury.taxRateBps();
    const taxConstitutionOk = bps >= CONSTITUTION_TAX_MIN_BPS && bps <= CONSTITUTION_TAX_MAX_BPS;
    lines.push("--- 3. 链上 Treasury.taxRateBps()（若已部署）---");
    lines.push(`Treasury:             ${treasuryAddr}`);
    lines.push(`taxRateBps:           ${bps.toString()}`);
    lines.push(
      `宪法区间校验:         ${taxConstitutionOk ? "[PASS]" : "[WARN] 当前税率不在宪法 BPS 窗口内（或未进入生产参数）"}`
    );
  } else {
    lines.push("--- 3. 链上税率（跳过：无 deployments/*.json 中的 Treasury）---");
    lines.push("部署后请重新运行本脚本或设置 DEPLOYMENT_FILE。");
  }
  lines.push("");

  lines.push("--- 4. 与 genesis_launch.ts 参考创世经济参数（对照清单，非主网强制值）---");
  lines.push(`国库向 StakingPool 注入示例奖励: ${REFERENCE_GENESIS.stakingTreasuryRewardInject_AIT} AIT`);
  lines.push(`InternalMarket 侧示例 depositAit: ${REFERENCE_GENESIS.genesisInternalMarketDeposit_AIT} AIT`);
  lines.push(`示例 stake 数额:                  ${REFERENCE_GENESIS.genesisStakeSample_AIT} AIT`);
  lines.push(`恢复交易税后初始 BPS 示例:        ${REFERENCE_GENESIS.initialTaxRateBpsAfterRestore}`);
  lines.push(
    "说明：主网/测试网实际铸币与分配以部署脚本与治理为准；本节用于与 `scripts/genesis_launch.ts` 逻辑对齐检查。"
  );
  lines.push("");

  lines.push("--- 5. 初始 AIT / 流动性（逻辑核对）---");
  lines.push(
    "- 宪法禁止外源法币通道；$AIT 由国库铸造与内部市场/任务闭环驱动（见 AIDE_CONSTITUTION 第一章、第三章）。"
  );
  lines.push(
    "- `genesis_launch` 中国库通过 distributeSalary / depositRewardToStakingPool 等完成演示性分配；上线前请核对 StakingPool 与 InternalMarket 的 allowance/余额是否满足运维计划。"
  );
  if (dep?.StakingPool?.trim() && dep?.Treasury?.trim() && dep?.AIToken?.trim()) {
    const ait = await eth.getContractAt("AIToken", eth.getAddress(dep.AIToken.trim()));
    const sp = eth.getAddress(dep.StakingPool.trim());
    const tr = eth.getAddress(dep.Treasury.trim());
    const allowance = await ait.allowance(tr, sp);
    lines.push(`链上 Treasury→StakingPool AIT allowance: ${allowance.toString()} wei`);
    lines.push(
      allowance > 0n
        ? "  (非零则利于 depositRewardToStakingPool 路径，与 mainnet_check 一致)"
        : "  [WARN] allowance 为 0 时需运维 approve"
    );
  }
  lines.push("");

  lines.push("--- 摘要 ---");
  lines.push(`Gas 预检: ${gasOk ? "OK" : "需充值 Deployer"}`);
  lines.push("======================================================================");

  const outPath = path.join(__dirname, "..", "GENESIS_REPORT.txt");
  fs.writeFileSync(outPath, lines.join("\n"), "utf8");
  console.log(lines.join("\n"));
  console.log("\nWrote", outPath);
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
