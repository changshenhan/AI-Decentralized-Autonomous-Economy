//! 运维告警：将 `[CRITICAL_DRIFT]` / `[CRITICAL_SETTLEMENT_FAILURE]` 异步投递到 Webhook（Discord / Slack / Telegram Bot API 等兼容 JSON POST 的端点）。

use crate::telemetry::ContractAddressBook;
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

/// 通过 `ALERT_WEBHOOK_URL` 投递 JSON 告警（`POST`，`Content-Type: application/json`）。
#[derive(Clone)]
pub struct NotificationManager {
    webhook_url: Option<String>,
    client: Client,
}

#[derive(Serialize)]
struct AlertPayload {
    /// Slack / 通用：`text`；Discord Incoming Webhook 亦接受 `content` 字段，此处双字段提高兼容性
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    aide_alert: bool,
    severity: &'static str,
    kind: String,
    detail: String,
    contracts: ContractAddressBook,
}

impl NotificationManager {
    pub fn from_env() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .expect("reqwest client for NotificationManager");
        Self {
            webhook_url: std::env::var("ALERT_WEBHOOK_URL")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            client,
        }
    }

    pub fn has_webhook(&self) -> bool {
        self.webhook_url.is_some()
    }

    /// 异步发送，不阻塞撮合暂停路径；失败仅打日志。
    pub fn send_critical_alert(&self, kind: &str, detail: String, contracts: ContractAddressBook) {
        let Some(ref url) = self.webhook_url else {
            return;
        };
        let text = format!("AIDE CRITICAL [{}]\n{}", kind, detail);
        let payload = AlertPayload {
            content: Some(text.clone()),
            text,
            aide_alert: true,
            severity: "critical",
            kind: kind.to_string(),
            detail,
            contracts,
        };
        let url = url.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            match client.post(&url).json(&payload).send().await {
                Ok(resp) if resp.status().is_success() => {}
                Ok(resp) => {
                    tracing::error!(
                        status = %resp.status(),
                        "alert webhook returned non-success"
                    );
                }
                Err(e) => tracing::error!(?e, "alert webhook request failed"),
            }
        });
    }
}
