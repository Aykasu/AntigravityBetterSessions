use std::fs;
use std::path::PathBuf;
use std::process::Command;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};

pub fn find_antigravity_exe() -> Result<PathBuf, String> {
    // 1. Check standard Windows LocalAppData installation path
    if let Some(local_dir) = dirs::data_local_dir() {
        let standard_path = local_dir
            .join("Programs")
            .join("antigravity")
            .join("Antigravity.exe");
        if standard_path.exists() {
            return Ok(standard_path);
        }
    }

    // 2. Check running processes via sysinfo
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new().with_cmd(UpdateKind::Always),
        ),
    );
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All);

    for (_pid, proc) in sys.processes() {
        let name = proc.name().to_string_lossy().to_lowercase();
        if name == "antigravity.exe" || name == "antigravity" {
            if let Some(exe) = proc.exe() {
                if exe.exists() {
                    return Ok(exe.to_path_buf());
                }
            }
        }
    }

    // 3. Fallback to hardcoded standard Windows default
    let default_win = PathBuf::from("C:\\Users")
        .join(std::env::var("USERNAME").unwrap_or_else(|_| "Default".into()))
        .join("AppData")
        .join("Local")
        .join("Programs")
        .join("antigravity")
        .join("Antigravity.exe");
    if default_win.exists() {
        return Ok(default_win);
    }

    Err("Не удалось найти Antigravity.exe на компьютере".into())
}

pub fn get_profiles_base_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("AntigravityProfiles")
}

pub fn setup_profile_dir(profile_id: &str) -> Result<PathBuf, String> {
    let base = get_profiles_base_dir();
    let profile_dir = base.join(profile_id);
    fs::create_dir_all(&profile_dir)
        .map_err(|e| format!("Не удалось создать папку профиля: {}", e))?;

    // Copy user settings from default Antigravity profile if available for great UX
    if let Some(roaming) = dirs::data_dir() {
        let src_user = roaming.join("Antigravity").join("User");
        let dst_user = profile_dir.join("User");
        if src_user.exists() && !dst_user.exists() {
            let _ = fs::create_dir_all(&dst_user);
            let settings_file = src_user.join("settings.json");
            if settings_file.exists() {
                let _ = fs::copy(&settings_file, dst_user.join("settings.json"));
            }
            let keybindings_file = src_user.join("keybindings.json");
            if keybindings_file.exists() {
                let _ = fs::copy(&keybindings_file, dst_user.join("keybindings.json"));
            }
        }
    }

    Ok(profile_dir)
}

pub fn launch_antigravity_profile(data_dir: Option<&str>) -> Result<(), String> {
    let exe = find_antigravity_exe()?;
    let mut cmd = Command::new(&exe);

    if let Some(dir) = data_dir {
        if !dir.trim().is_empty() {
            cmd.arg(format!("--user-data-dir={}", dir));
        }
    }

    #[cfg(target_os = "windows")]
    {
        // CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS so it lives independently
        cmd.creation_flags(0x00000200 | 0x00000008);
    }

    cmd.spawn()
        .map_err(|e| format!("Ошибка запуска Antigravity: {}", e))?;

    Ok(())
}

pub fn close_running_antigravity() -> Result<usize, String> {
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new().with_cmd(UpdateKind::Always),
        ),
    );
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All);

    let mut killed = 0;
    for (_pid, proc) in sys.processes() {
        let name = proc.name().to_string_lossy().to_lowercase();
        if name == "antigravity.exe" || name == "antigravity" {
            proc.kill();
            killed += 1;
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Also call taskkill /F /T /IM Antigravity.exe to guarantee full tree shutdown
        let mut cmd = Command::new("taskkill");
        cmd.args(["/F", "/T", "/IM", "Antigravity.exe"]);
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        let _ = cmd.output();
    }

    Ok(killed)
}
