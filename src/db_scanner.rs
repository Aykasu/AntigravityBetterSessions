use std::collections::HashMap;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::protobuf_parser::parse_tokens_from_gen_metadata;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectStats {
    pub name: String,
    pub path: String,
    pub account: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub gemini_tokens: u64,
    pub claude_tokens: u64,
    pub conversation_count: u32,
    pub total_steps: u64,
    pub total_duration_seconds: i64,
    pub last_activity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationItem {
    pub id: String,
    pub title: String,
    pub project_name: String,
    pub project_path: String,
    pub account: String,
    pub step_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub duration_seconds: i64,
    pub last_modified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DailyStat {
    pub date: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregatedStats {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub gemini_tokens: u64,
    pub claude_tokens: u64,
    pub total_conversations: u32,
    pub total_steps: u64,
    pub total_time_spent_seconds: i64,
    pub last_prompt_tokens: u64,
    pub last_prompt_in: u64,
    pub last_prompt_out: u64,
    pub projects: Vec<ProjectStats>,
    pub conversations: Vec<ConversationItem>,
    pub daily_history: Vec<DailyStat>,
}

fn get_conv_accounts_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("AntigravitySentinel")
        .join("conversation_accounts.json")
}

fn load_conv_accounts() -> HashMap<String, String> {
    let path = get_conv_accounts_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    }
}

fn save_conv_accounts(map: &HashMap<String, String>) {
    let path = get_conv_accounts_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(path, json);
    }
}

fn get_antigravity_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let p = home.join(".gemini").join("antigravity");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn decode_workspace_uri(uri_json: &str) -> (String, String) {
    if let Ok(uris) = serde_json::from_str::<Vec<String>>(uri_json) {
        if let Some(uri) = uris.first() {
            // e.g. file:///z%3A/Aykasus/Antigravirty/TEST%20MODELS/3.8
            let mut decoded = uri.replace("file:///", "").replace("file://", "");
            // URL decode %3A -> :, %20 -> space
            decoded = decoded.replace("%3A", ":").replace("%3a", ":").replace("%20", " ");
            decoded = decoded.replace('/', "\\");

            let name = Path::new(&decoded)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("General Workspace")
                .to_string();

            return (name, decoded);
        }
    }
    ("General Workspace".into(), "Default".into())
}

#[allow(dead_code)]
pub fn scan_antigravity_data() -> AggregatedStats {
    scan_antigravity_data_filtered(None, "Default", &[])
}

pub fn scan_antigravity_data_filtered(
    filter_account: Option<&str>,
    default_account: &str,
    known_accounts: &[String],
) -> AggregatedStats {
    let mut stats = AggregatedStats::default();
    let agy_dir = match get_antigravity_dir() {
        Some(d) => d,
        None => return stats,
    };

    let summary_db_path = agy_dir.join("conversation_summaries.db");
    if !summary_db_path.exists() {
        return stats;
    }

    let conn = match Connection::open_with_flags(
        &summary_db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    ) {
        Ok(c) => c,
        Err(_) => return stats,
    };

    let mut stmt = match conn.prepare(
        "SELECT conversation_id, title, preview, step_count, last_modified_time, workspace_uris, last_user_input_time 
         FROM conversation_summaries 
         ORDER BY datetime(last_modified_time) DESC",
    ) {
        Ok(s) => s,
        Err(_) => return stats,
    };

    let conv_dir = agy_dir.join("conversations");
    let mut project_map: HashMap<String, ProjectStats> = HashMap::new();
    let mut daily_map: HashMap<String, (u64, u64)> = HashMap::new();
    let mut conv_items: Vec<ConversationItem> = Vec::new();

    let filter_clean = filter_account.map(|s| s.trim().to_lowercase());
    let mut conv_account_map = load_conv_accounts();
    let mut map_changed = false;
    let mut is_first = true;

    let rows = stmt.query_map([], |row| {
        let cid: String = row.get(0)?;
        let title: String = row.get(1).unwrap_or_default();
        let preview: String = row.get(2).unwrap_or_default();
        let step_count: u64 = row.get(3).unwrap_or(0);
        let last_mod: String = row.get(4).unwrap_or_default();
        let workspace_uris: String = row.get(5).unwrap_or_default();
        let last_input: String = row.get(6).unwrap_or_default();
        Ok((cid, title, preview, step_count, last_mod, workspace_uris, last_input))
    });

    if let Ok(rows) = rows {
        for row in rows.flatten() {
            let (cid, mut title, preview, step_count, last_mod, workspace_uris, last_input) = row;
            if title.trim().is_empty() {
                title = if !preview.trim().is_empty() {
                    preview
                } else {
                    format!("Conversation {}", &cid[..8.min(cid.len())])
                };
            }

            let (proj_name, proj_path) = decode_workspace_uri(&workspace_uris);

            let (conv_in, conv_out, conv_gemini, conv_claude, mut conv_account, step_in, step_out) =
                scan_single_conv_tokens(&conv_dir, &cid, default_account, known_accounts);

            // 1. Persistent account binding: if already mapped, use mapped account!
            if let Some(saved) = conv_account_map.get(&cid) {
                conv_account = saved.clone();
            } else if !conv_account.is_empty() && conv_account != "Default" {
                conv_account_map.insert(cid.clone(), conv_account.clone());
                map_changed = true;
            }

            // Capture last prompt tokens from the most recently updated conversation
            if is_first {
                stats.last_prompt_tokens = step_in + step_out;
                stats.last_prompt_in = step_in;
                stats.last_prompt_out = step_out;
                is_first = false;
            }

            // 2. Strict Filter if account specified!
            if let Some(ref target) = filter_clean {
                if !target.is_empty() && conv_account.to_lowercase() != *target {
                    continue;
                }
            }

            let mut duration_secs: i64 = 0;
            if let (Ok(start), Ok(end)) = (
                DateTime::parse_from_rfc3339(&last_input),
                DateTime::parse_from_rfc3339(&last_mod),
            ) {
                let diff = end.signed_duration_since(start).num_seconds();
                if diff > 0 && diff < 86400 * 3 {
                    duration_secs = diff;
                }
            }
            if duration_secs == 0 && step_count > 0 {
                duration_secs = (step_count as i64) * 15;
            }

            let conv_total = conv_in + conv_out;
            stats.total_input_tokens += conv_in;
            stats.total_output_tokens += conv_out;
            stats.total_tokens += conv_total;
            stats.gemini_tokens += conv_gemini;
            stats.claude_tokens += conv_claude;
            stats.total_steps += step_count;
            stats.total_conversations += 1;
            stats.total_time_spent_seconds += duration_secs;

            // Daily tracking
            let date_key = if last_mod.len() >= 10 {
                last_mod[..10].to_string()
            } else {
                Utc::now().format("%Y-%m-%d").to_string()
            };
            let daily = daily_map.entry(date_key).or_insert((0, 0));
            daily.0 += conv_in;
            daily.1 += conv_out;

            // Project aggregation
            let proj = project_map.entry(proj_path.clone()).or_insert_with(|| ProjectStats {
                name: proj_name.clone(),
                path: proj_path.clone(),
                account: conv_account.clone(),
                last_activity: last_mod.clone(),
                ..Default::default()
            });

            proj.total_input_tokens += conv_in;
            proj.total_output_tokens += conv_out;
            proj.total_tokens += conv_total;
            proj.gemini_tokens += conv_gemini;
            proj.claude_tokens += conv_claude;
            proj.conversation_count += 1;
            proj.total_steps += step_count;
            proj.total_duration_seconds += duration_secs;
            if last_mod > proj.last_activity {
                proj.last_activity = last_mod.clone();
            }

            conv_items.push(ConversationItem {
                id: cid,
                title,
                project_name: proj_name,
                project_path: proj_path,
                account: conv_account,
                step_count,
                input_tokens: conv_in,
                output_tokens: conv_out,
                total_tokens: conv_total,
                duration_seconds: duration_secs,
                last_modified: last_mod,
            });
        }
    }

    if map_changed {
        save_conv_accounts(&conv_account_map);
    }

    let mut projects: Vec<ProjectStats> = project_map.into_values().collect();
    projects.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));

    let mut daily_vec: Vec<DailyStat> = daily_map
        .into_iter()
        .map(|(date, (inp, out))| DailyStat {
            date,
            input_tokens: inp,
            output_tokens: out,
            total_tokens: inp + out,
        })
        .collect();
    daily_vec.sort_by(|a, b| a.date.cmp(&b.date));

    conv_items.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
    conv_items.truncate(50);

    stats.projects = projects;
    stats.conversations = conv_items;
    stats.daily_history = daily_vec;

    stats
}

fn scan_single_conv_tokens(
    conv_dir: &Path,
    cid: &str,
    default_account: &str,
    known_accounts: &[String],
) -> (u64, u64, u64, u64, String, u64, u64) {
    let db_file = conv_dir.join(format!("{}.db", cid));
    if !db_file.exists() {
        return (0, 0, 0, 0, default_account.to_string(), 0, 0);
    }

    let conn = match Connection::open_with_flags(
        &db_file,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    ) {
        Ok(c) => c,
        Err(_) => return (0, 0, 0, 0, default_account.to_string(), 0, 0),
    };

    let mut in_tok: u64 = 0;
    let mut out_tok: u64 = 0;
    let mut gemini_tok: u64 = 0;
    let mut claude_tok: u64 = 0;
    let mut last_step_in: u64 = 0;
    let mut last_step_out: u64 = 0;
    let mut account = default_account.to_string();

    // Check if account can be identified in trajectory metadata
    if let Ok(mut stmt) = conn.prepare("SELECT data FROM trajectory_metadata_blob WHERE id = 'main'") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0)) {
            for data in rows.flatten() {
                let text = String::from_utf8_lossy(&data);
                let mut matched = false;
                for known in known_accounts {
                    if !known.is_empty() && text.contains(known) {
                        account = known.clone();
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    // Check for general email pattern
                    if let Some(pos) = text.find('@') {
                        let start = text[..pos]
                            .rfind(|c: char| !c.is_alphanumeric() && c != '.' && c != '_' && c != '-')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        let end = text[pos..]
                            .find(|c: char| !c.is_alphanumeric() && c != '.' && c != '_' && c != '-')
                            .map(|i| pos + i)
                            .unwrap_or(text.len());
                        let candidate = &text[start..end];
                        if candidate.contains('.') && candidate.len() > 5 && !candidate.ends_with('.') {
                            account = candidate.to_string();
                        }
                    }
                }
                break;
            }
        }
    }

    if let Ok(mut stmt) = conn.prepare("SELECT data FROM gen_metadata WHERE size > 0") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0)) {
            for data in rows.flatten() {
                let counts = parse_tokens_from_gen_metadata(&data);
                let step_total = counts.input_tokens + counts.output_tokens;
                last_step_in = counts.input_tokens;
                last_step_out = counts.output_tokens;
                in_tok += counts.input_tokens;
                out_tok += counts.output_tokens;

                let is_claude = data.windows(11).any(|w| w == b"used_claude")
                    && (data.windows(13).any(|w| w == b"used_claude\x01\x01")
                        || data.windows(16).any(|w| w == b"used_claude\x04true")
                        || data.windows(13).any(|w| w == b"claude-sonnet")
                        || data.windows(21).any(|w| w == b"used_non_gemini_model\x01\x01")
                        || data.windows(24).any(|w| w == b"used_non_gemini_model\x04true"));

                if is_claude {
                    claude_tok += step_total;
                } else {
                    gemini_tok += step_total;
                }
            }
        }
    }

    (in_tok, out_tok, gemini_tok, claude_tok, account, last_step_in, last_step_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner() {
        let stats = scan_antigravity_data();
        println!("Total tokens: {}", stats.total_tokens);
        println!("Gemini tokens: {}", stats.gemini_tokens);
        println!("Claude tokens: {}", stats.claude_tokens);
        println!("Projects: {}", stats.projects.len());
        println!("Conversations: {}", stats.conversations.len());
        assert!(stats.total_tokens > 0);
    }
}
