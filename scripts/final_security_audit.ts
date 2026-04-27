/**
 * 最终权限审查：遍历部署合约的 owner / DEFAULT_ADMIN_ROLE（hasRole），提示部署者是否仍持有管理权限。
 */
import fs from "fs";
import path from "path";
import hre from "hardhat";

const OWNABLE_ABI = ["function owner() view returns (address)"];
const AC_MIN_ABI = [
  "function DEFAULT_ADMIN_ROLE() view returns (bytes32)",
  "function hasRole(bytes32 role, address account) view returns (bool)",
];

type Entry = { name: string; key: string };

const MANAGED: Entry[] = [
  { name: "AIToken", key: "AIToken" },
  { name: "Treasury", key: "Treasury" },
  { name: "Guardrail", key: "Guardrail" },
  { name: "AIIdentityRegistry", key: "AIIdentityRegistry" },
  { name: "StakingPool", key: "StakingPool" },
  { name: "AIBank", key: "AIBank" },
  { name: "InternalMarket", key: "InternalMarket" },
  { name: "TaskMarket", key: "TaskMarket" },
  { name: "AI_Economist_Controller", key: "AI_Economist_Controller" },
  { name: "DualPoolVault", key: "DualPoolVault" },
];

function loadDeployment(): Record<string, string> | null {
  const candidates = [
    process.env.DEPLOYMENT_FILE,
    path.join(__dirname, "..", "deployments", "localhost.json"),
    path.join(__dirname, "..", "deployments", "base_sepolia.json"),
  ].filter(Boolean) as string[];
  for (const p of candidates) {
    if (fs.existsSync(p)) {
      return JSON.parse(fs.readFileSync(p, "utf8")) as Record<string, string>;
    }
  }
  return null;
}

export async function runFinalSecurityAudit(): Promise<void> {
  const eth = hre.ethers;
  const dep = loadDeployment();
  const deployer =
    dep?.["admin"]?.trim() ||
    process.env.AIDE_ADMIN_ADDRESS?.trim() ||
    (await eth.getSigners())[0].address;

  console.log("\n========== AIDE Final Security Audit (roles / ownership) ==========\n");
  console.log("Reference deployer/admin address:", deployer);

  let warned = false;

  for (const { name, key } of MANAGED) {
    const raw = dep?.[key]?.trim();
    if (!raw || !eth.isAddress(raw)) {
      console.log(`[SKIP] ${name}: no address in deployment`);
      continue;
    }
    const addr = eth.getAddress(raw);
    console.log(`\n>>> ${name}  ${addr}`);

    try {
      const o = new eth.Contract(addr, OWNABLE_ABI, eth.provider);
      const owner = await o.owner();
      console.log(`    owner() = ${owner}`);
      if (owner.toLowerCase() === deployer.toLowerCase()) {
        console.log("    WARNING: DEPLOYER STILL HAS ADMIN PRIVILEGES (Ownable.owner)");
        warned = true;
      }
    } catch {
      console.log("    (no owner() — not Ownable or call failed)");
    }

    try {
      const c = new eth.Contract(addr, AC_MIN_ABI, eth.provider);
      const role = await c.DEFAULT_ADMIN_ROLE();
      const hasAdm = await c.hasRole(role, deployer);
      console.log(`    hasRole(DEFAULT_ADMIN_ROLE, deployer) = ${hasAdm}`);
      if (hasAdm) {
        console.log("    WARNING: DEPLOYER STILL HAS ADMIN PRIVILEGES (AccessControl DEFAULT_ADMIN)");
        warned = true;
      }
    } catch {
      console.log("    (no AccessControl DEFAULT_ADMIN check — skipped)");
    }
  }

  console.log("\n------------------------------------------------------------------");
  if (warned) {
    console.log(
      "SUMMARY: Deployer still holds at least one Ownable owner or DEFAULT_ADMIN_ROLE.\n" +
        "         For production, transfer roles per AIDE_MAINNET_OPERATIONS.md / deploy_final.ts.\n"
    );
  } else {
    console.log(
      "SUMMARY: Deployer not detected as owner() or DEFAULT_ADMIN on scanned contracts\n" +
        "         (or interfaces skipped). Verify on Basescan if needed.\n"
    );
  }
  console.log("======================================================================\n");
}

if (require.main === module) {
  runFinalSecurityAudit().catch((e) => {
    console.error(e);
    process.exitCode = 1;
  });
}
