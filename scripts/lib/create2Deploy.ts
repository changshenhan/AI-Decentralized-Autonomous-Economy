/**
 * CREATE2 确定性部署：依赖 `AideCreate2Factory`（见 contracts/AideCreate2Factory.sol）。
 * 多链同址：工厂合约地址须一致（专用 EOA 在各链以相同 nonce 部署工厂，或使用 `CREATE2_FACTORY_ADDRESS`）。
 */
import { ethers } from "ethers";
import type { BaseContract, ContractFactory, ContractTransactionResponse } from "ethers";

export const C2_SALT = {
  guardrail: ethers.id("AIDE_C2_GUARD_V1"),
  aitoken: ethers.id("AIDE_C2_AITOKEN_V1"),
  bank: ethers.id("AIDE_C2_BANK_V1"),
  treasury: ethers.id("AIDE_C2_TREASURY_V1"),
  internalMarket: ethers.id("AIDE_C2_INTERNAL_MARKET_V1"),
} as const;

const DEPLOYED_IFACE = new ethers.Interface([
  "event Deployed(address indexed addr, bytes32 indexed salt)",
]);

/** 使用已连接的 `AideCreate2Factory` 实例部署，返回新合约地址 */
export async function deployViaCreate2(
  c2Factory: BaseContract,
  artifact: ContractFactory,
  salt: string,
  constructorArgs: unknown[]
): Promise<string> {
  const txReq = await artifact.getDeployTransaction(...constructorArgs);
  const data = txReq.data;
  if (!data) throw new Error("create2: empty creation bytecode");
  const sent: ContractTransactionResponse = await (c2Factory as any).deploy(salt, data);
  const rc = await sent.wait();
  if (!rc) throw new Error("create2: no receipt");
  for (const log of rc.logs) {
    try {
      const p = DEPLOYED_IFACE.parseLog(log);
      if (p?.name === "Deployed") {
        return ethers.getAddress(p.args[0] as string);
      }
    } catch {
      /* next log */
    }
  }
  throw new Error("create2: Deployed event not found");
}
