use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;


#[derive(Debug, Clone, Default)]
pub struct AntigravityInstance {
    pub ls_port: u16,
    pub csrf_token: String,
}

fn extract_csrf_from_cmdline(cmdline: &str) -> Option<String> {
    if let Some(pos) = cmdline.find("--csrf_token ") {
        let after = &cmdline[pos + 13..];
        let token: String = after.split_whitespace().next()?.to_string();
        if token.len() > 10 { return Some(token); }
    }
    if let Some(pos) = cmdline.find("--csrf_token=") {
        let after = &cmdline[pos + 13..];
        let token: String = after.split_whitespace().next().unwrap_or("").trim_end_matches('"').to_string();
        if token.len() > 10 { return Some(token); }
    }
    None
}

fn find_csrf_via_wmic() -> Option<String> {
    // Use cmd /c wmic — reliable in GUI processes without a console window
    let output = std::process::Command::new("cmd")
        .args(["/c", "wmic process where \"name='language_server.exe'\" get CommandLine /value"])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if let Some(token) = extract_csrf_from_cmdline(&text) {
        return Some(token);
    }

    // Fallback: powershell hidden window
    let output = std::process::Command::new("powershell")
        .args(["-WindowStyle", "Hidden", "-NonInteractive", "-Command",
            "(Get-CimInstance Win32_Process -Filter \"name = 'language_server.exe'\").CommandLine"
        ])
        .creation_flags(0x08000000)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    extract_csrf_from_cmdline(&text)
}

pub fn find_antigravity_instance() -> Option<AntigravityInstance> {
    // 1. First inspect running processes for language_server.exe via sysinfo
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new().with_cmd(UpdateKind::Always),
        ),
    );
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All);

    let mut csrf_from_cmd = None;

    for (_pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_lowercase();
        if name.contains("language_server") {
            let cmd = process.cmd();
            for i in 0..cmd.len() {
                let arg = cmd[i].to_string_lossy();
                if arg == "--csrf_token" && i + 1 < cmd.len() {
                    csrf_from_cmd = Some(cmd[i + 1].to_string_lossy().to_string());
                    break;
                } else if arg.starts_with("--csrf_token=") {
                    csrf_from_cmd = Some(arg.trim_start_matches("--csrf_token=").to_string());
                    break;
                }
            }
        }
    }

    // 2. If sysinfo couldn't read cmd (integrity level issue), try WMI via powershell
    if csrf_from_cmd.is_none() {
        csrf_from_cmd = find_csrf_via_wmic();
    }

    // 3. Discover the language server port from DevToolsActivePort or fallback
    let ls_port = find_ls_port();

    if let (Some(port), Some(csrf)) = (ls_port, csrf_from_cmd) {
        return Some(AntigravityInstance {
            ls_port: port,
            csrf_token: csrf,
        });
    }

    None
}

fn find_ls_port() -> Option<u16> {
    // Check DevToolsActivePort in %APPDATA%\Antigravity\DevToolsActivePort
    let appdata = dirs::data_dir()?; // e.g. AppData/Roaming
    let devtools_path = appdata.join("Antigravity").join("DevToolsActivePort");

    if devtools_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&devtools_path) {
            let lines: Vec<&str> = content.lines().collect();
            if let Some(first_line) = lines.first() {
                if let Ok(dt_port) = first_line.trim().parse::<u16>() {
                    // Query DevTools to find the page URL
                    if let Some(port) = query_devtools_for_ls_port(dt_port) {
                        return Some(port);
                    }
                }
            }
        }
    }

    // Fallback: check language_server.log in Roaming/Antigravity/logs/
    let log_path = appdata.join("Antigravity").join("logs").join("language_server.log");
    if log_path.exists() {
        if let Ok(content) = std::fs::read_to_string(log_path) {
            for line in content.lines().rev().take(50) {
                if let Some(port) = extract_port_from_log_line(line) {
                    return Some(port);
                }
            }
        }
    }

    None
}

fn query_devtools_for_ls_port(dt_port: u16) -> Option<u16> {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let addr = SocketAddr::from(([127, 0, 0, 1], dt_port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(800)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(800)));

    let req = format!(
        "GET /json/list HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        dt_port
    );
    stream.write_all(req.as_bytes()).ok()?;

    let mut buf = [0u8; 8192];
    match stream.read(&mut buf) {
        Ok(n) if n > 0 => {
            let text = String::from_utf8_lossy(&buf[..n]);
            for part in text.split("127.0.0.1:") {
                let port_str: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(p) = port_str.parse::<u16>() {
                    if p != dt_port && p > 1024 {
                        return Some(p);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("DevTools read error: {:?}", e);
        }
        _ => {}
    }

    None
}

fn extract_port_from_log_line(line: &str) -> Option<u16> {
    // "listening on ... port at 61484 for HTTP"
    if line.contains("port at ") {
        let parts: Vec<&str> = line.split("port at ").collect();
        if parts.len() > 1 {
            let port_str: String = parts[1].chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(p) = port_str.parse::<u16>() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finder() {
        let ls_port = find_ls_port();
        println!("find_ls_port result: {:?}", ls_port);
        let csrf_wmic = find_csrf_via_wmic();
        println!("find_csrf_via_wmic result: {:?}", csrf_wmic);
        let inst = find_antigravity_instance();
        println!("Discovered instance: {:?}", inst);
    }
}
