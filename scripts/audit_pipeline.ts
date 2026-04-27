/**
 * 单次 Hardhat 会话内：先创世部署再跑主网预检。
 * 解决分两次 `hardhat run` 时每次都是新链、localhost.json 地址无代码的问题。
 */
import { execSync } from "child_process";
import path from "path";
import { genesisLaunch } from "./genesis_launch";
import { runFinalSecurityAudit } from "./final_security_audit";
import { runMainnetChecks } from "./mainnet_check";
import { writeFinalReleaseFile } from "./write_final_release";

function runEngineAuditTests() {
  const engineDir = path.join(__dirname, "..", "engine");
  console.log("\n--- Rust: Sync-Audit (audit.rs) unit tests ---\n");
  execSync("cargo test audit:: --quiet -- --nocapture", {
    cwd: engineDir,
    stdio: "inherit",
  });
}

async function main() {
  runEngineAuditTests();
  await genesisLaunch();
  await runMainnetChecks();
  await runFinalSecurityAudit();
  writeFinalReleaseFile();
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
