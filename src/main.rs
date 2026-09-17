#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod antigravity_finder;
mod api;
mod cleaner;
mod db_scanner;
mod launcher;
mod protobuf_parser;
mod profile_store;
mod quota_client;
mod updater;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use tao::{
    dpi::LogicalSize,
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

#[derive(Debug, Clone)]
pub enum UserWindowEvent {
    SetAlwaysOnTop(bool),
    SetWindowSize(f64, f64),
    SetMiniMode(bool),
    Show,
    Hide,
    UpdateTray(String),
}

fn create_default_tray_icon() -> tray_icon::Icon {
    let width = 32u32;
    let height = 32u32;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let cx = x as f32 - 16.0;
            let cy = y as f32 - 16.0;
            let dist = (cx * cx + cy * cy).sqrt();
            if dist <= 14.0 && dist >= 8.0 {
                // Neon teal/cyan outer ring
                rgba[idx] = 20;
                rgba[idx + 1] = 184;
                rgba[idx + 2] = 166;
                rgba[idx + 3] = 255;
            } else if dist < 8.0 && dist >= 2.0 {
                // Purple inner core
                rgba[idx] = 139;
                rgba[idx + 1] = 92;
                rgba[idx + 2] = 246;
                rgba[idx + 3] = 240;
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, width, height).expect("Failed to create tray icon")
}

use antigravity_finder::find_antigravity_instance;
use api::{create_router, AppState};
use db_scanner::{scan_antigravity_data_filtered, AggregatedStats};
use profile_store::ProfileStore;
use quota_client::{fetch_live_quota, LiveQuotaData};

fn main() {
    updater::cleanup_old_binary();

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let _ = std::env::set_current_dir(parent);
        }
    }

    std::panic::set_hook(Box::new(|info| {
        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "Unknown panic payload",
            },
        };
        let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let log_msg = format!("Panic at {}:\n{}\nFull info: {:?}", location, msg, info);
        let _ = std::fs::write("sentinel_error.log", log_msg);
    }));

    let log_file = |msg: &str| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("sentinel_debug.log") {
            let _ = writeln!(f, "[{}] {}", chrono::Local::now().format("%H:%M:%S%.3f"), msg);
            let _ = f.flush();
        }
    };

    log_file("1. main started");

    // 1. Initialize EventLoop and Window on STA Main Thread FIRST
    log_file("1.1. About to call EventLoopBuilder::with_user_event().build()");
    let event_loop: EventLoop<UserWindowEvent> = EventLoopBuilder::<UserWindowEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    log_file("2. EventLoop created");

    log_file("2.1. About to build Window");
    let window = match WindowBuilder::new()
        .with_title("Antigravity BetterSessions • Supercharged Sentinel")
        .with_inner_size(LogicalSize::new(1220.0, 840.0))
        .with_min_inner_size(LogicalSize::new(320.0, 180.0))
        .build(&event_loop)
    {
        Ok(w) => {
            log_file("3. Window built successfully");
            w
        }
        Err(e) => {
            let err_text = format!("Window build error: {:?}", e);
            log_file(&err_text);
            panic!("{}", err_text);
        }
    };

    // 1.2. Build System Tray
    let tray_menu = tray_icon::menu::Menu::new();
    let show_item = tray_icon::menu::MenuItem::new("Открыть BetterSessions", true, None);
    let mini_item = tray_icon::menu::MenuItem::new("Мини-виджет (HUD)", true, None);
    let sep = tray_icon::menu::PredefinedMenuItem::separator();
    let quit_item = tray_icon::menu::MenuItem::new("Выход", true, None);

    let show_id = show_item.id().clone();
    let mini_id = mini_item.id().clone();
    let quit_id = quit_item.id().clone();

    let _ = tray_menu.append_items(&[&show_item, &mini_item, &sep, &quit_item]);

    let mut tray_icon = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Antigravity BetterSessions • Token Tracker")
        .with_icon(create_default_tray_icon())
        .build()
        .ok();

    // 2. Bind local std TCP listener
    let std_listener = match std::net::TcpListener::bind("127.0.0.1:4848") {
        Ok(l) => l,
        Err(_) => std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind any port"),
    };
    let port = std_listener.local_addr().map(|a| a.port()).unwrap_or(4848);
    std_listener.set_nonblocking(true).expect("Failed to set nonblocking");
    log_file(&format!("4. Server bound to port {}", port));

    // 3. Initialize WebView with dedicated user data directory
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("C:\\Users\\Default\\AppData\\Local")))
        .join("AntigravitySentinel")
        .join("webview");
    let _ = std::fs::create_dir_all(&data_dir);
    log_file(&format!("5. Data directory: {:?}", data_dir));

    let url = format!("http://127.0.0.1:{}", port);

    // Try primary context (persistent), then fallback to unique temp dir per session
    let primary_dir = data_dir;
    let fallback_dir = std::env::temp_dir().join(format!("agy_sentinel_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&fallback_dir);

    let mut primary_ctx = wry::WebContext::new(Some(primary_dir));
    let mut fallback_ctx = wry::WebContext::new(Some(fallback_dir));

    let webview = match WebViewBuilder::new_with_web_context(&mut primary_ctx)
        .with_url(&url)
        .build(&window)
    {
        Ok(wv) => {
            log_file("6. WebView built OK with primary context");
            wv
        }
        Err(e) => {
            log_file(&format!("6. Primary WebView locked ({:?}), trying fallback...", e));
            match WebViewBuilder::new_with_web_context(&mut fallback_ctx)
                .with_url(&url)
                .build(&window)
            {
                Ok(wv) => {
                    log_file("6. WebView built OK with fallback context");
                    wv
                }
                Err(e2) => {
                    let err_text = format!("WebView build error (fallback): {:?}", e2);
                    log_file(&err_text);
                    let _ = std::fs::write("sentinel_error.log", &err_text);
                    panic!("{}", err_text);
                }
            }
        }
    };

    // 4. Start Tokio runtime in background for Axum server & pollers
    let profile_store = Arc::new(ProfileStore::new());
    let proxy_arc = Arc::new(proxy.clone());
    let state = AppState {
        stats: Arc::new(RwLock::new(AggregatedStats::default())),
        live_quota: Arc::new(RwLock::new(LiveQuotaData::default())),
        instance: Arc::new(RwLock::new(None)),
        profiles: profile_store.clone(),
        proxy: Some(proxy_arc.clone()),
    };

    let rt = Box::leak(Box::new(tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime")));
    log_file("7. Tokio runtime created");

    let poller_state = state.clone();
    let poller_proxy = proxy.clone();
    rt.spawn(async move {
        // Initial quick load
        let inst_opt = find_antigravity_instance();
        *poller_state.instance.write().await = inst_opt.clone();

        if let Some(ref inst) = inst_opt {
            if let Ok(quota) = fetch_live_quota(inst.ls_port, &inst.csrf_token).await {
                poller_state.profiles.upsert_active_account(
                    &quota.user_email,
                    &quota.user_name,
                    &quota.user_tier.name,
                    Some(quota.quota.clone()),
                );
                *poller_state.live_quota.write().await = quota;
            }
        }

        let active_email = poller_state.live_quota.read().await.user_email.clone();
        let known: Vec<String> = poller_state
            .profiles
            .get_all()
            .into_iter()
            .map(|p| p.email)
            .filter(|e| !e.is_empty())
            .collect();
        if let Ok(new_stats) = tokio::task::spawn_blocking(move || {
            let def = if !active_email.is_empty() { active_email } else { "Default".to_string() };
            scan_antigravity_data_filtered(None, &def, &known)
        }).await {
            *poller_state.stats.write().await = new_stats;
        }

        // Ongoing periodic poller
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;

            let inst_opt = find_antigravity_instance();
            *poller_state.instance.write().await = inst_opt.clone();

            if let Some(inst) = inst_opt {
                if let Ok(quota) = fetch_live_quota(inst.ls_port, &inst.csrf_token).await {
                    poller_state.profiles.upsert_active_account(
                        &quota.user_email,
                        &quota.user_name,
                        &quota.user_tier.name,
                        Some(quota.quota.clone()),
                    );
                    *poller_state.live_quota.write().await = quota;
                }
            } else {
                poller_state.profiles.mark_all_inactive();
                let mut q = poller_state.live_quota.write().await;
                q.connected = false;
            }

            // Update tray tooltip with live percentages
            {
                let q = poller_state.live_quota.read().await;
                let gem_pct = q.quota.groups.iter()
                    .find(|g| g.display_name.to_lowercase().contains("gemini"))
                    .and_then(|g| g.buckets.first())
                    .map(|b| (b.remaining_fraction * 100.0).round() as i64)
                    .unwrap_or(100);
                let cld_pct = q.quota.groups.iter()
                    .find(|g| g.display_name.to_lowercase().contains("claude"))
                    .and_then(|g| g.buckets.first())
                    .map(|b| (b.remaining_fraction * 100.0).round() as i64)
                    .unwrap_or(100);
                drop(q);
                let tip = format!("Gemini: {}% | Claude: {}% • Antigravity BetterSessions", gem_pct, cld_pct);
                let _ = poller_proxy.send_event(UserWindowEvent::UpdateTray(tip));
            }

            let active_email = poller_state.live_quota.read().await.user_email.clone();
            let known: Vec<String> = poller_state
                .profiles
                .get_all()
                .into_iter()
                .map(|p| p.email)
                .filter(|e| !e.is_empty())
                .collect();
            if let Ok(new_stats) = tokio::task::spawn_blocking(move || {
                let def = if !active_email.is_empty() { active_email } else { "Default".to_string() };
                scan_antigravity_data_filtered(None, &def, &known)
            }).await {
                *poller_state.stats.write().await = new_stats;
            }
        }
    });

    let app = create_router(state);
    rt.spawn(async move {
        if let Ok(tokio_listener) = tokio::net::TcpListener::from_std(std_listener) {
            let _ = axum::serve(tokio_listener, app).await;
        }
    });

    log_file("8. Entering GUI event loop");
    event_loop.run(move |event, _, control_flow| {
        let _ = (&webview, &primary_ctx, &fallback_ctx);
        *control_flow = ControlFlow::Wait;

        // Tray click event
        if let Ok(tray_event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click { button: tray_icon::MouseButton::Left, .. } = tray_event {
                let is_visible = window.is_visible();
                window.set_visible(!is_visible);
                if !is_visible {
                    window.set_focus();
                }
            }
        }

        // Tray menu items
        if let Ok(menu_event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if menu_event.id == show_id {
                window.set_visible(true);
                window.set_always_on_top(false);
                window.set_inner_size(LogicalSize::new(1220.0, 840.0));
                window.set_focus();
            } else if menu_event.id == mini_id {
                window.set_visible(true);
                window.set_always_on_top(true);
                window.set_inner_size(LogicalSize::new(380.0, 240.0));
                window.set_focus();
            } else if menu_event.id == quit_id {
                *control_flow = ControlFlow::Exit;
            }
        }

        match event {
            Event::NewEvents(StartCause::Init) => {
                log_file("Event: Init");
            }
            Event::UserEvent(user_evt) => match user_evt {
                UserWindowEvent::SetAlwaysOnTop(on_top) => {
                    window.set_always_on_top(on_top);
                }
                UserWindowEvent::SetWindowSize(w, h) => {
                    window.set_inner_size(LogicalSize::new(w, h));
                }
                UserWindowEvent::SetMiniMode(is_mini) => {
                    if is_mini {
                        window.set_always_on_top(true);
                        window.set_inner_size(LogicalSize::new(380.0, 240.0));
                    } else {
                        window.set_always_on_top(false);
                        window.set_inner_size(LogicalSize::new(1220.0, 840.0));
                    }
                }
                UserWindowEvent::Show => {
                    window.set_visible(true);
                    window.set_focus();
                }
                UserWindowEvent::Hide => {
                    window.set_visible(false);
                }
                UserWindowEvent::UpdateTray(tip) => {
                    if let Some(ref mut t) = tray_icon {
                        let _ = t.set_tooltip(Some(&tip));
                    }
                }
            },
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                log_file("Event: CloseRequested -> hiding to tray");
                if tray_icon.is_some() {
                    window.set_visible(false);
                } else {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::LoopDestroyed => {
                log_file("Event: LoopDestroyed");
            }
            _ => (),
        }
    });
}
