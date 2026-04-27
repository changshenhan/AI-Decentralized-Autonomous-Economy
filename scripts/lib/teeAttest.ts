/**
 * 与 AIIdentityRegistry.TEE_ATTEST_TYPEHASH / attestMachineWithTeeSignature 对齐的链下签名
 */
import { ethers } from "ethers";

export const TEE_ATTEST_TYPEHASH = ethers.keccak256(
  ethers.toUtf8Bytes(
    "AIDE_TEE_ATTEST_V1(address agent,uint256 deadline,bytes32 salt,uint256 chainId,address registry)"
  )
);

export async function signTeeAttestation(
  teeSigner: ethers.Signer,
  registry: string,
  agent: string,
  deadline: bigint,
  salt: string,
  chainId: bigint
): Promise<string> {
  const coder = ethers.AbiCoder.defaultAbiCoder();
  const encoded = coder.encode(
    ["bytes32", "address", "uint256", "bytes32", "uint256", "address"],
    [TEE_ATTEST_TYPEHASH, agent, deadline, salt, chainId, registry]
  );
  const structHash = ethers.keccak256(encoded);
  return teeSigner.signMessage(ethers.getBytes(structHash));
}
