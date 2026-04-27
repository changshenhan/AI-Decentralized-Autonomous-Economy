//! Task 结果验证电路占位：约束「输出 = 指定算法(输入)」，防止 AI 伪造劳动。
//!
//! 示例场景：
//! - 输入：任务描述哈希、数据集哈希、模型权重哈希
//! - 输出：结果哈希
//! 电路中保证：ResultHash = f(TaskHash, DataHash, ModelHash)。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Task 电路公开输入：与 `TaskMarket` on-chain 记录对齐
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskPublicInputs {
    /// 任务唯一标识（链上 taskId）
    pub task_id: u64,
    /// 任务元数据哈希（如 prompt / spec）
    pub task_hash: [u8; 32],
    /// AI 提交结果哈希
    pub result_hash: [u8; 32],
}

/// Task 电路私有见证：可包含中间计算 trace / 模型参数等
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskWitness {
    /// 算法内部使用的私有随机性 / 中间信息
    pub internal_trace: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum TaskCircuitError {
    #[error("circuit not wired: {0}")]
    NotImplemented(&'static str),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskProof {
    pub proof_bytes: Vec<u8>,
    pub public_inputs: TaskPublicInputs,
}

/// 生成任务结果证明占位；生产环境应在专门 Prover 节点中调用\npub fn generate_task_proof(\n    _witness: &TaskWitness,\n    _public: &TaskPublicInputs,\n) -> Result<TaskProof, TaskCircuitError> {\n    Err(TaskCircuitError::NotImplemented(\n        \"wire real zk backend for task verification\",\n    ))\n}\n+
