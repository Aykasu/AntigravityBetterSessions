use std::env;
use std::fs;
use std::process::Command;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use serde::{Deserialize, Serialize};

pub const GITHUB_OWNER: &str = "Aykasu";
pub const GITHUB_REPO: &str = "AntigravityBetterSessions";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_name: String,
    pub release_notes: String,
    pub download_url: Option<String>,
    pub published_at: String,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    published_at: Option<String>,
    assets: Vec<GitHubAsset>,
}

pub fn cleanup_old_binary() {
    if let Ok(current_exe) = env::current_exe() {
        let old_exe = current_exe.with_extension("exe.old");
        if old_exe.exists() {
            let _ = fs::remove_file(old_exe);
        }
        let new_exe = current_exe.with_extension("exe.new");
        if new_exe.exists() {
            let _ = fs::remove_file(new_exe);
        }
    }
}

fn parse_semver(v: &str) -> (u64, u64, u64) {
    let clean = v.trim().trim_start_matches('v').trim_start_matches('V');
    let parts: Vec<&str> = clean.split('.').collect();
    let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.get(2).and_then(|s| s.split('-').next()).and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

fn is_newer(current: &str, candidate: &str) -> bool {
    let c = parse_semver(current);
    let n = parse_semver(candidate);
    n > c
}

pub async fn check_for_updates() -> Result<UpdateInfo, String> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        GITHUB_OWNER, GITHUB_REPO
    );

    let client = reqwest::Client::builder()
        .user_agent(format!("AntigravityBetterSessions/{}", CURRENT_VERSION))
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .map_err(|e| e.to_string())?;

    let res = client.get(&url).send().await.map_err(|e| format!("Не удалось проверить обновления: {}", e))?;

    if res.status() == reqwest::StatusCode::NOT_FOUND {
        // Repo or releases not published yet
        return Ok(UpdateInfo {
            has_update: false,
            current_version: CURRENT_VERSION.to_string(),
            latest_version: CURRENT_VERSION.to_string(),
            release_name: "Версия актуальна".to_string(),
            release_notes: "Вы используете последнюю версию программы.".to_string(),
            download_url: None,
            published_at: "".to_string(),
        });
    }

    if !res.status().is_success() {
        return Err(format!("GitHub API вернул статус: {}", res.status()));
    }

    let release: GitHubRelease = res.json().await.map_err(|e| format!("Ошибка парсинга ответа GitHub: {}", e))?;
    let latest_tag = release.tag_name.trim().to_string();
    let has_update = is_newer(CURRENT_VERSION, &latest_tag);

    // Look for Windows .exe asset
    let download_url = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(".exe") || a.name.contains("antigravity"))
        .or_else(|| release.assets.first())
        .map(|a| a.browser_download_url.clone());

    Ok(UpdateInfo {
        has_update,
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest_tag,
        release_name: release.name.unwrap_or_else(|| "Обновление Antigravity BetterSessions".into()),
        release_notes: release.body.unwrap_or_else(|| "Список изменений не указан.".into()),
        download_url,
        published_at: release.published_at.unwrap_or_default(),
    })
}

pub async fn download_and_hot_swap(download_url: &str) -> Result<(), String> {
    let current_exe = env::current_exe().map_err(|e| format!("Не удалось определить путь exe: {}", e))?;
    let new_exe = current_exe.with_extension("exe.new");
    let old_exe = current_exe.with_extension("exe.old");

    // 1. Download file bytes
    let client = reqwest::Client::builder()
        .user_agent(format!("AntigravityBetterSessions/{}", CURRENT_VERSION))
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let res = client
        .get(download_url)
        .send()
        .await
        .map_err(|e| format!("Ошибка скачивания обновления: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Сервер вернул ошибку скачивания: {}", res.status()));
    }

    let bytes = res.bytes().await.map_err(|e| format!("Ошибка загрузки данных: {}", e))?;

    // 2. Write to .exe.new
    fs::write(&new_exe, &bytes).map_err(|e| format!("Не удалось сохранить временный файл {}: {}", new_exe.display(), e))?;

    // 3. Hot Swap: rename current_exe -> .exe.old
    if old_exe.exists() {
        let _ = fs::remove_file(&old_exe);
    }
    fs::rename(&current_exe, &old_exe).map_err(|e| format!("Не удалось переместить текущий файл: {}", e))?;

    // 4. Rename .exe.new -> current_exe
    if let Err(e) = fs::rename(&new_exe, &current_exe) {
        // Rollback
        let _ = fs::rename(&old_exe, &current_exe);
        return Err(format!("Не удалось применить новый бинарник: {}", e));
    }

    // 5. Spawn new process independently
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new(&current_exe);
        // CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS
        cmd.creation_flags(0x00000200 | 0x00000008);
        cmd.spawn().map_err(|e| format!("Не удалось перезапустить процесс: {}", e))?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new(&current_exe)
            .spawn()
            .map_err(|e| format!("Не удалось перезапустить процесс: {}", e))?;
    }

    // 6. Exit old process
    std::process::exit(0);
}
