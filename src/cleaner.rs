use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub size_bytes: u64,
    pub formatted_size: String,
    pub exists: bool,
    pub can_clean: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanerScanResult {
    pub items: Vec<CacheItem>,
    pub total_size_bytes: u64,
    pub total_formatted_size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanerCleanResult {
    pub success: bool,
    pub freed_bytes: u64,
    pub formatted_freed: String,
    pub cleaned_items: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanRequest {
    pub item_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenFolderRequest {
    pub path: String,
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} ГБ", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} МБ", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} КБ", bytes as f64 / 1024.0)
    } else {
        format!("{} Б", bytes)
    }
}

fn get_dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut total: u64 = 0;
    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

struct CacheTarget {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    path: PathBuf,
}

fn get_cache_targets() -> Vec<CacheTarget> {
    let mut targets = Vec::new();

    let roaming = dirs::data_dir();
    let local = dirs::data_local_dir();
    let home = dirs::home_dir();

    if let Some(ref r) = roaming {
        let agy = r.join("Antigravity");
        targets.push(CacheTarget {
            id: "agy_cache",
            name: "Кэш Antigravity (HTTP/Сеть)",
            description: "Временный сетевой кэш Chromium и запросов интерфейса",
            path: agy.join("Cache"),
        });
        targets.push(CacheTarget {
            id: "agy_code_cache",
            name: "V8 Code Cache (Скомпилированный JS)",
            description: "Байткод V8 и кэш расширений редактора",
            path: agy.join("Code Cache"),
        });
        targets.push(CacheTarget {
            id: "agy_gpu_cache",
            name: "GPU & Шейдерный кэш",
            description: "Кэш видеокарты и графических пайплайнов",
            path: agy.join("GPUCache"),
        });
        targets.push(CacheTarget {
            id: "agy_dawn_cache",
            name: "Dawn WebGPU кэш",
            description: "Кэш аппаратного ускорения интерфейса",
            path: agy.join("DawnCache"),
        });
        targets.push(CacheTarget {
            id: "agy_logs",
            name: "Логи сессий Antigravity",
            description: "Исторические текстовые журналы работы редактора",
            path: agy.join("logs"),
        });
        targets.push(CacheTarget {
            id: "agy_crashpad",
            name: "Краш-дампы Antigravity",
            description: "Автоматические отчеты об ошибках Electron Crashpad",
            path: agy.join("Crashpad"),
        });
    }

    if let Some(ref h) = home {
        let gemini_agy = h.join(".gemini").join("antigravity");
        targets.push(CacheTarget {
            id: "gemini_crashes",
            name: "Дампы падений (.gemini)",
            description: "Архив дампов падений сессий агента",
            path: gemini_agy.join("crashes"),
        });
        targets.push(CacheTarget {
            id: "gemini_logs",
            name: "Логи агента (.gemini)",
            description: "Журналы отладки фоновых задач и агента",
            path: gemini_agy.join("logs"),
        });
    }

    if let Some(ref l) = local {
        let sentinel_wv = l.join("AntigravitySentinel").join("webview").join("EBWebView");
        targets.push(CacheTarget {
            id: "sentinel_cache",
            name: "Кэш WebView Sentinel",
            description: "Временный кэш встроенного браузера Sentinel",
            path: sentinel_wv.join("Default").join("Cache"),
        });
        targets.push(CacheTarget {
            id: "sentinel_code_cache",
            name: "Code Cache WebView Sentinel",
            description: "Кэш скриптов WebView Sentinel",
            path: sentinel_wv.join("Default").join("Code Cache"),
        });
    }

    targets
}

pub fn scan_caches() -> CleanerScanResult {
    let targets = get_cache_targets();
    let mut items = Vec::new();
    let mut total_bytes: u64 = 0;

    for target in targets {
        let exists = target.path.exists();
        let size = if exists { get_dir_size(&target.path) } else { 0 };
        total_bytes += size;

        items.push(CacheItem {
            id: target.id.to_string(),
            name: target.name.to_string(),
            description: target.description.to_string(),
            path: target.path.to_string_lossy().to_string(),
            size_bytes: size,
            formatted_size: format_bytes(size),
            exists,
            can_clean: true,
        });
    }

    CleanerScanResult {
        total_size_bytes: total_bytes,
        total_formatted_size: format_bytes(total_bytes),
        items,
    }
}

pub fn clean_cache_items(item_ids: Option<Vec<String>>) -> CleanerCleanResult {
    let targets = get_cache_targets();
    let mut freed_bytes: u64 = 0;
    let mut cleaned_items = Vec::new();
    let mut errors = Vec::new();

    for target in targets {
        if let Some(ref filter) = item_ids {
            if !filter.iter().any(|id| id == target.id) {
                continue;
            }
        }

        if !target.path.exists() {
            continue;
        }

        let size_before = get_dir_size(&target.path);

        // Clean directory contents safely
        let clean_res = (|| -> Result<u64, std::io::Error> {
            let mut freed: u64 = 0;
            if let Ok(entries) = fs::read_dir(&target.path) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_file() {
                        if let Ok(meta) = entry_path.metadata() {
                            freed += meta.len();
                        }
                        let _ = fs::remove_file(&entry_path);
                    } else if entry_path.is_dir() {
                        freed += get_dir_size(&entry_path);
                        let _ = fs::remove_dir_all(&entry_path);
                    }
                }
            }
            Ok(freed)
        })();

        match clean_res {
            Ok(f) => {
                let actual_freed = if f > 0 { f } else { size_before };
                freed_bytes += actual_freed;
                cleaned_items.push(target.id.to_string());
            }
            Err(e) => {
                errors.push(format!("Ошибка очистки {}: {}", target.name, e));
            }
        }
    }

    CleanerCleanResult {
        success: errors.is_empty(),
        freed_bytes,
        formatted_freed: format_bytes(freed_bytes),
        cleaned_items,
        errors,
    }
}

pub fn open_folder_in_explorer(path_str: &str) -> Result<(), String> {
    let path = Path::new(path_str);
    let target = if path.exists() {
        path.to_path_buf()
    } else if let Some(parent) = path.parent() {
        if parent.exists() {
            parent.to_path_buf()
        } else {
            return Err("Папка еще не создана приложением".into());
        }
    } else {
        return Err("Указанный путь не найден".into());
    };

    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("explorer.exe");
        cmd.arg(&target);
        cmd.spawn()
            .map_err(|e| format!("Не удалось открыть проводник: {}", e))?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("Поддерживается только на Windows".into())
    }
}
