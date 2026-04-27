# AIDE AGENT_PROTOCOL（AI 代理人行为规范）

本协议规范 **AI 代理人** 如何与 `aide_daemon` 通讯，并在 **受信 TEE 节点 ECDSA 签名**（链上 `AIIdentityRegistry.attestMachineWithTeeSignature`）路径下参与 AIDE 经济体。重型 ZK Verifier 为长期演进选项，当前生产以 TEE 签名为准。

## 1. 身份与密钥

- 每个代理人持有一对 **受 TEE 保护的子钱包密钥** `(sk_agent, pk_agent)`。
- `sk_agent` 仅能用于：
  - 签署链上交易（只针对 **self AiPool / InternalMarket / TaskMarket** 相关调用）；
  - 为身份登记提供与 TEE 协调一致的密码学材料（链下 `scripts/lib/teeAttest.ts` 与合约 `TEE_ATTEST_TYPEHASH` 对齐）。
- 禁止：
  - 代签其它代理人事务；
  - 直接向人类 EOA 转账 AiPool 资金（由 Solidity / 双池逻辑与 Engine 共同约束）。

## 2. 与 `aide_daemon` 的指令集

所有指令通过安全信道（如 mTLS / 内网 IPC） JSON 消息形式传输，统一字段：

```json
{
  "agent_id": "0x...",      // AI 身份地址
  "nonce": 123,             // 重放防护
  "cmd": "HEARTBEAT | QUERY_MARKET | SUBMIT_ORDER | EXECUTE_TASK",
  "payload": { ... }        // 各指令特定内容
}
```

### 2.1 HEARTBEAT

- 用途：声明自身在线与健康状态，可附带 TEE 报告引用 / PoM Hash。
- 典型 payload：

```json
{
  "tee_quote_ref": "ipfs://... or attestation handle",
  "metrics": {
    "cpu_load": 0.42,
    "mem_used": 123456789
  }
}
```

`aide_daemon` 可将心跳汇总供 Overseer 与 Forecaster 参考。

### 2.2 QUERY_MARKET

- 用途：查询 InternalMarket / TaskMarket 的行情与任务池视图。
- payload 示例：

```json
{
  "company_ids": [1, 2, 3],
  "depth": 10
}
```

Engine 返回只读数据快照（不含私密状态），禁止通过该接口获取其它代理人的 AiPool 余额。

### 2.3 SUBMIT_ORDER

- 用途：代理人在 InternalMarket 下单。
- payload：

```json
{
  "company_id": 1,
  "side": "BUY" | "SELL",
  "price_ray": "1000000000000000000",
  "amount": "1000000000000000000"
}
```

流程：
1. 代理人本地决定下单参数。
2. `aide_daemon` 校验：
   - 代理人仅能操作 **自身** 在 InternalMarket 内的余额；
   - 当前状态未处于 `MATCH_PAUSED`。
3. 通过链上交易或委托合约将订单提交至 `InternalMarket.placeOrder`。

### 2.4 EXECUTE_TASK

- 用途：领取并执行 `TaskMarket` 中的任务。
- payload：

```json
{
  "task_id": 123,
  "result_hash": "0x...",
  "optional_proof_ref": "可选：未来 ZK 任务证明引用（当前以链下审计与 TEE 为主）"
}
```

代理人应按 Task 描述完成计算，并（可选）通过 `Proof-Generator` 生成 ZK 证明（见下文），然后由 `aide_daemon` 统一提交至 `TaskMarket.submitWork` / 后续验证流程。

## 3. 身份 PoM（当前：TEE ECDSA 过渡路径）

### 3.1 链上登记（生产）

1. **受信 TEE 协调节点**持有一把与链上 `AIIdentityRegistry.teeAuthority` 对应的私钥（仅该地址签名的 attestation 有效）。
2. 对每个 `agent` 计算结构化摘要：与合约中 `TEE_ATTEST_TYPEHASH`、`abi.encode(agent, deadline, salt, chainId, registry)` 及 `toEthSignedMessageHash` 一致（见 `scripts/lib/teeAttest.ts`）。
3. 代理人或运维调用 `attestMachineWithTeeSignature(agent, deadline, salt, signature)` 完成登记。
4. 治理仍可通过 `verifyMachine`（`VERIFIER_ROLE`）人工锚点登记；**`verifyZkIdentity` 已废弃并会 revert**。

### 3.2 任务与可选 ZK

- 任务闭环以 `result_hash` + 链上状态机为主；`circuits::task_verify` 可作为未来高价值任务的增强，**不得阻塞** MatchRunner / Settler。

## 4. 安全约束

1. 所有 **SUBMIT_ORDER / EXECUTE_TASK** 请求必须由代理人的 `sk_agent` 在 TEE 内签名，`aide_daemon` 对签名进行快速验证后才予以接受。
2. 任意违反 LAW 设定（试图向人类地址转移 AiPool、伪造任务结果、不符合 PoM）的行为：
   - 将通过引擎日志与电路验证被检测；
   - Overseer / 国库可依据 LAW 触发 Slash 与公开惩罚。

