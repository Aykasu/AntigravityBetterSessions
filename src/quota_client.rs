use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Duration;

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn get_http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(4))
            .build()
            .unwrap_or_default()
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuotaBucket {
    #[serde(rename = "bucketId", default)]
    pub bucket_id: String,
    #[serde(rename = "displayName", default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub window: String,
    #[serde(rename = "remainingFraction", default)]
    pub remaining_fraction: f64,
    #[serde(rename = "resetTime", default)]
    pub reset_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuotaGroup {
    #[serde(rename = "displayName", default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub buckets: Vec<QuotaBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuotaResponseInner {
    #[serde(default)]
    pub groups: Vec<QuotaGroup>,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuotaResponseWrapper {
    pub response: Option<QuotaResponseInner>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserTierInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LiveQuotaData {
    pub connected: bool,
    pub port: u16,
    pub user_tier: UserTierInfo,
    pub user_name: String,
    pub user_email: String,
    pub quota: QuotaResponseInner,
    pub error: Option<String>,
}

pub async fn fetch_live_quota(port: u16, csrf_token: &str) -> Result<LiveQuotaData, String> {
    let client = get_http_client();

    let quota_url = format!(
        "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary",
        port
    );

    let quota_resp = client
        .post(&quota_url)
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1")
        .header("x-codeium-csrf-token", csrf_token)
        .body("{}")
        .send()
        .await
        .map_err(|e| format!("Failed to reach Antigravity server: {}", e))?;

    if !quota_resp.status().is_success() {
        return Err(format!("Antigravity returned HTTP {}", quota_resp.status()));
    }

    let quota_json: QuotaResponseWrapper = quota_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse quota JSON: {}", e))?;

    // Also fetch User Status for Tier Info
    let status_url = format!(
        "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/GetUserStatus",
        port
    );
    let mut user_tier = UserTierInfo {
        name: "Google AI Pro".into(),
        id: "g1-pro-tier".into(),
        description: "Google AI Pro Tier".into(),
    };

    let mut user_name = String::new();
    let mut user_email = String::new();

    if let Ok(status_resp) = client
        .post(&status_url)
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1")
        .header("x-codeium-csrf-token", csrf_token)
        .body("{}")
        .send()
        .await
    {
        if let Ok(val) = status_resp.json::<serde_json::Value>().await {
            if let Some(status) = val.get("userStatus") {
                if let Some(name) = status.get("name").and_then(|n| n.as_str()) {
                    user_name = name.to_string();
                }
                if let Some(email) = status.get("email").and_then(|e| e.as_str()) {
                    user_email = email.to_string();
                }
            }
            if let Some(tier) = val.get("userTier") {
                if let Some(name) = tier.get("name").and_then(|n| n.as_str()) {
                    user_tier.name = name.to_string();
                }
                if let Some(id) = tier.get("id").and_then(|i| i.as_str()) {
                    user_tier.id = id.to_string();
                }
            }
        }
    }

    Ok(LiveQuotaData {
        connected: true,
        port,
        user_tier,
        user_name,
        user_email,
        quota: quota_json.response.unwrap_or_default(),
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::antigravity_finder::find_antigravity_instance;

    #[tokio::test]
    async fn test_quota_fetch() {
        if let Some(inst) = find_antigravity_instance() {
            let res = fetch_live_quota(inst.ls_port, &inst.csrf_token).await;
            println!("Live Quota result: {:?}", res);
            assert!(res.is_ok());
            let q = res.unwrap();
            println!("User: {}, Tier: {}", q.user_name, q.user_tier.name);
            println!("Groups count: {}", q.quota.groups.len());
        }
    }
}
