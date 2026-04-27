/**
 * 生成 AIDE_1.0_FINAL_RELEASE.txt（ASCII、地址、宪法哈希、宣告）
 */
import crypto from "crypto";
import fs from "fs";
import path from "path";

export function writeFinalReleaseFile(): void {
  const root = path.join(__dirname, "..");
  const depPath = path.join(root, "deployments", "localhost.json");
  const constitutionPath = path.join(root, "AIDE_CONSTITUTION.md");
  const lawPath = path.join(root, "LAW_OF_AIDE.md");
  const outPath = path.join(root, "AIDE_1.0_FINAL_RELEASE.txt");

  let dep: Record<string, string> = {};
  if (fs.existsSync(depPath)) {
    dep = JSON.parse(fs.readFileSync(depPath, "utf8")) as Record<string, string>;
  }

  let constitutionSha = "(AIDE_CONSTITUTION.md not found)";
  if (fs.existsSync(constitutionPath)) {
    const buf = fs.readFileSync(constitutionPath);
    constitutionSha = "sha256:" + crypto.createHash("sha256").update(buf).digest("hex");
  }

  let lawSha = "(LAW_OF_AIDE.md not found)";
  if (fs.existsSync(lawPath)) {
    const buf = fs.readFileSync(lawPath);
    lawSha = "sha256:" + crypto.createHash("sha256").update(buf).digest("hex");
  }

  const lines: string[] = [];
  lines.push("");
  lines.push("    * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * *");
  lines.push("    *                                                                 *");
  lines.push("    *     █████╗     ██╗██████╗ ███████╗    ██╗  ██╗    ██████╗       *");
  lines.push("    *    ██╔══██╗    ██║██╔══██╗██╔════╝    ╚██╗██╔╝    ╚════██╗      *");
  lines.push("    *    ███████║    ██║██║  ██║█████╗       ╚███╔╝      █████╔╝      *");
  lines.push("    *    ██╔══██║    ██║██║  ██║██╔══╝       ██╔██╗     ██╔═══╝       *");
  lines.push("    *    ██║  ██║    ██║██████╔╝███████╗    ██╔╝ ██╗    ███████╗      *");
  lines.push("    *    ╚═╝  ╚═╝    ╚═╝╚═════╝ ╚══════╝    ╚═╝  ╚═╝    ╚══════╝      *");
  lines.push("    *                                                                 *");
  lines.push("    *              A I D E   1 . 0   F I N A L   R E L E A S E         *");
  lines.push("    *                                                                 *");
  lines.push("    * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * *");
  lines.push("");
  lines.push("  --- 机器经济正式开启 ---");
  lines.push("");
  lines.push("  Constitution digest (AIDE_CONSTITUTION.md):");
  lines.push("  " + constitutionSha);
  lines.push("  Legal foundation digest (LAW_OF_AIDE.md):");
  lines.push("  " + lawSha);
  lines.push("");
  lines.push("  --- Core contract addresses (from deployments/localhost.json if present) ---");
  const keys = [
    "AIToken",
    "Treasury",
    "Guardrail",
    "InternalMarket",
    "TaskMarket",
    "AIIdentityRegistry",
    "StakingPool",
    "AIBank",
    "AI_Economist_Controller",
    "DualPoolVault",
  ];
  for (const k of keys) {
    const v = dep[k]?.trim();
    if (v) lines.push(`  ${k.padEnd(28)} ${v}`);
  }
  if (dep["admin"]) lines.push(`  ${"admin (deploy snapshot)".padEnd(28)} ${dep["admin"]}`);
  lines.push("");
  lines.push("  --- Message ---");
  lines.push("  The AIDE 1.0 closed-loop economy is declared operational: TEE-backed");
  lines.push("  machine identity, Treasury-mediated tax, InternalMarket settlement,");
  lines.push("  and Sync-Audit alignment between chain truth and engine shadow state.");
  lines.push("");
  lines.push("  [V2 Roadmap] On-chain ZK task verification and extended ZK-PoM are");
  lines.push("  out of scope for this 1.0 release tag.");
  lines.push("");
  lines.push("  " + new Date().toISOString());
  lines.push("");

  fs.writeFileSync(outPath, lines.join("\n"), "utf8");
  console.log("Wrote", outPath);
}

if (require.main === module) {
  writeFinalReleaseFile();
}
