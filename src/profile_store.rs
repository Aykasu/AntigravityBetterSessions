use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::quota_client::QuotaResponseInner;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfile {
    pub id: String,
    pub email: String,
    pub name: String,
    pub alias: Option<String>,
    pub user_tier: String,
    pub last_seen: String,
    pub last_quota: Option<QuotaResponseInner>,
    pub is_active: bool,
    pub data_dir: Option<String>,
}

#[allow(dead_code)]
impl AccountProfile {
    pub fn display_name(&self) -> String {
        if let Some(ref a) = self.alias {
            if !a.trim().is_empty() {
                return a.clone();
            }
        }
        if !self.name.is_empty() {
            return self.name.clone();
        }
        if !self.email.is_empty() {
            return self.email.clone();
        }
        "Default".to_string()
    }
}

pub struct ProfileStore {
    path: PathBuf,
    profiles: Mutex<HashMap<String, AccountProfile>>,
}

impl ProfileStore {
    pub fn new() -> Self {
        let base_dir = dirs::data_local_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
            .join("AntigravitySentinel");
        let _ = fs::create_dir_all(&base_dir);
        let path = base_dir.join("profiles.json");

        let mut map = HashMap::new();
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(loaded) = serde_json::from_str::<Vec<AccountProfile>>(&data) {
                    for p in loaded {
                        map.insert(p.id.clone(), p);
                    }
                }
            }
        }

        Self {
            path,
            profiles: Mutex::new(map),
        }
    }

    pub fn get_all(&self) -> Vec<AccountProfile> {
        let guard = self.profiles.lock().unwrap();
        let mut list: Vec<AccountProfile> = guard.values().cloned().collect();
        list.sort_by(|a, b| {
            b.is_active
                .cmp(&a.is_active)
                .then_with(|| b.last_seen.cmp(&a.last_seen))
        });
        list
    }

    #[allow(dead_code)]
    pub fn get(&self, id: &str) -> Option<AccountProfile> {
        let guard = self.profiles.lock().unwrap();
        guard.get(id).cloned()
    }

    pub fn upsert_active_account(
        &self,
        email: &str,
        name: &str,
        user_tier: &str,
        quota: Option<QuotaResponseInner>,
    ) -> AccountProfile {
        let mut guard = self.profiles.lock().unwrap();

        let id = if !email.is_empty() {
            email.to_lowercase()
        } else if !name.is_empty() {
            name.to_lowercase()
        } else {
            "default".to_string()
        };

        for p in guard.values_mut() {
            p.is_active = false;
        }

        let now = Utc::now().to_rfc3339();
        let profile = if let Some(existing) = guard.get_mut(&id) {
            existing.email = if !email.is_empty() { email.to_string() } else { existing.email.clone() };
            existing.name = if !name.is_empty() { name.to_string() } else { existing.name.clone() };
            existing.user_tier = if !user_tier.is_empty() { user_tier.to_string() } else { existing.user_tier.clone() };
            existing.last_seen = now;
            existing.is_active = true;
            if quota.is_some() {
                existing.last_quota = quota;
            }
            existing.clone()
        } else {
            let p = AccountProfile {
                id: id.clone(),
                email: email.to_string(),
                name: name.to_string(),
                alias: None,
                user_tier: user_tier.to_string(),
                last_seen: now,
                last_quota: quota,
                is_active: true,
                data_dir: None,
            };
            guard.insert(id, p.clone());
            p
        };

        drop(guard);
        self.save();
        profile
    }

    pub fn mark_all_inactive(&self) {
        let mut guard = self.profiles.lock().unwrap();
        for p in guard.values_mut() {
            p.is_active = false;
        }
        drop(guard);
        self.save();
    }

    pub fn create_custom_profile(&self, alias: &str, data_dir: Option<String>) -> AccountProfile {
        let mut guard = self.profiles.lock().unwrap();
        let timestamp = Utc::now().timestamp_millis();
        let safe_name = alias.trim().replace(' ', "_").to_lowercase();
        let id = format!("prof_{}_{}", safe_name, timestamp);

        let p = AccountProfile {
            id: id.clone(),
            email: String::new(),
            name: alias.trim().to_string(),
            alias: Some(alias.trim().to_string()),
            user_tier: "Не авторизован".to_string(),
            last_seen: Utc::now().to_rfc3339(),
            last_quota: None,
            is_active: false,
            data_dir,
        };

        guard.insert(id, p.clone());
        drop(guard);
        self.save();
        p
    }

    pub fn delete_profile(&self, id: &str) -> bool {
        let mut guard = self.profiles.lock().unwrap();
        let removed = guard.remove(id).is_some();
        drop(guard);
        if removed {
            self.save();
        }
        removed
    }

    pub fn rename_account(&self, id: &str, alias: &str) -> bool {
        let mut guard = self.profiles.lock().unwrap();
        if let Some(p) = guard.get_mut(id) {
            let clean_alias = alias.trim();
            if clean_alias.is_empty() {
                p.alias = None;
            } else {
                p.alias = Some(clean_alias.to_string());
            }
            drop(guard);
            self.save();
            true
        } else {
            false
        }
    }

    fn save(&self) {
        let guard = self.profiles.lock().unwrap();
        let list: Vec<&AccountProfile> = guard.values().collect();
        if let Ok(json) = serde_json::to_string_pretty(&list) {
            let _ = fs::write(&self.path, json);
        }
    }
}
