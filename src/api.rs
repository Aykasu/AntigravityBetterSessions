use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tao::event_loop::EventLoopProxy;

use crate::antigravity_finder::{find_antigravity_instance, AntigravityInstance};
use crate::cleaner::{
    clean_cache_items, open_folder_in_explorer, scan_caches, CleanRequest, CleanerCleanResult,
    CleanerScanResult, OpenFolderRequest,
};
use crate::db_scanner::{scan_antigravity_data_filtered, AggregatedStats, ConversationItem, ProjectStats};
use crate::launcher::{close_running_antigravity, launch_antigravity_profile, setup_profile_dir};
use crate::profile_store::{AccountProfile, ProfileStore};
use crate::quota_client::{fetch_live_quota, LiveQuotaData};
use crate::UserWindowEvent;

#[derive(Clone)]
pub struct AppState {
    pub stats: Arc<RwLock<AggregatedStats>>,
    pub live_quota: Arc<RwLock<LiveQuotaData>>,
    pub instance: Arc<RwLock<Option<AntigravityInstance>>>,
    pub profiles: Arc<ProfileStore>,
    pub proxy: Option<Arc<EventLoopProxy<UserWindowEvent>>>,
}

#[derive(serde::Deserialize)]
pub struct AccountFilterQuery {
    pub account: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct RenameAccountRequest {
    pub id: String,
    pub alias: String,
}

#[derive(serde::Deserialize)]
pub struct CreateProfileRequest {
    pub alias: String,
}

#[derive(serde::Deserialize)]
pub struct LaunchProfileRequest {
    pub id: String,
}

#[derive(serde::Deserialize)]
pub struct DeleteProfileRequest {
    pub id: String,
}

#[derive(serde::Deserialize)]
pub struct PinWindowRequest {
    pub pinned: bool,
}

#[derive(serde::Deserialize)]
pub struct InstallUpdateRequest {
    pub download_url: String,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/index.html", get(serve_index))
        .route("/app.js", get(serve_js))
        .route("/style.css", get(serve_css))
        .route("/chart.umd.min.js", get(serve_chartjs))
        .route("/lucide.min.js", get(serve_lucide))
        .route("/api/live", get(get_live_quota))
        .route("/api/stats", get(get_stats))
        .route("/api/projects", get(get_projects))
        .route("/api/conversations", get(get_conversations))
        .route("/api/accounts", get(get_accounts))
        .route("/api/accounts/rename", post(rename_account))
        .route("/api/profiles/create", post(create_profile_handler))
        .route("/api/profiles/launch", post(launch_profile_handler))
        .route("/api/profiles/close", post(close_antigravity_handler))
        .route("/api/profiles/delete", post(delete_profile_handler))
        .route("/api/cleaner/scan", get(scan_cleaner_handler))
        .route("/api/cleaner/clean", post(clean_cleaner_handler))
        .route("/api/cleaner/open-folder", post(open_folder_handler))
        .route("/api/window/mini", post(window_mini_handler))
        .route("/api/window/restore", post(window_restore_handler))
        .route("/api/window/pin", post(window_pin_handler))
        .route("/api/window/hide", post(window_hide_handler))
        .route("/api/window/show", post(window_show_handler))
        .route("/api/update/check", get(check_update_handler))
        .route("/api/update/install", post(install_update_handler))
        .route("/api/refresh", post(refresh_data))
        .with_state(state)
}

async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn serve_js() -> Response {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        include_str!("../static/app.js"),
    )
        .into_response()
}

async fn serve_css() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../static/style.css"),
    )
        .into_response()
}

async fn serve_chartjs() -> Response {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        include_str!("../static/chart.umd.min.js"),
    )
        .into_response()
}

async fn serve_lucide() -> Response {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        include_str!("../static/lucide.min.js"),
    )
        .into_response()
}

async fn get_live_quota(State(state): State<AppState>) -> Json<LiveQuotaData> {
    let quota = state.live_quota.read().await.clone();
    Json(quota)
}

async fn get_stats(
    State(state): State<AppState>,
    Query(params): Query<AccountFilterQuery>,
) -> Json<AggregatedStats> {
    let base = state.stats.read().await.clone();
    let target = match params.account {
        Some(ref a) if !a.trim().is_empty() && a != "all" => a.trim().to_lowercase(),
        _ => return Json(base),
    };

    let mut filtered = AggregatedStats::default();
    for conv in &base.conversations {
        if conv.account.to_lowercase() == target {
            filtered.total_input_tokens += conv.input_tokens;
            filtered.total_output_tokens += conv.output_tokens;
            filtered.total_tokens += conv.total_tokens;
            filtered.total_steps += conv.step_count;
            filtered.total_conversations += 1;
            filtered.total_time_spent_seconds += conv.duration_seconds;
            filtered.conversations.push(conv.clone());
        }
    }

    for proj in &base.projects {
        if proj.account.to_lowercase() == target {
            filtered.projects.push(proj.clone());
            filtered.gemini_tokens += proj.gemini_tokens;
            filtered.claude_tokens += proj.claude_tokens;
        }
    }

    filtered.daily_history = base.daily_history.clone();
    Json(filtered)
}

async fn get_projects(
    State(state): State<AppState>,
    Query(params): Query<AccountFilterQuery>,
) -> Json<Vec<ProjectStats>> {
    let stats = state.stats.read().await;
    let target = match params.account {
        Some(ref a) if !a.trim().is_empty() && a != "all" => a.trim().to_lowercase(),
        _ => return Json(stats.projects.clone()),
    };
    let filtered: Vec<ProjectStats> = stats
        .projects
        .iter()
        .filter(|p| p.account.to_lowercase() == target)
        .cloned()
        .collect();
    Json(filtered)
}

async fn get_conversations(
    State(state): State<AppState>,
    Query(params): Query<AccountFilterQuery>,
) -> Json<Vec<ConversationItem>> {
    let stats = state.stats.read().await;
    let target = match params.account {
        Some(ref a) if !a.trim().is_empty() && a != "all" => a.trim().to_lowercase(),
        _ => return Json(stats.conversations.clone()),
    };
    let filtered: Vec<ConversationItem> = stats
        .conversations
        .iter()
        .filter(|c| c.account.to_lowercase() == target)
        .cloned()
        .collect();
    Json(filtered)
}

async fn get_accounts(State(state): State<AppState>) -> Json<Vec<AccountProfile>> {
    let list = state.profiles.get_all();
    Json(list)
}

async fn rename_account(
    State(state): State<AppState>,
    Json(payload): Json<RenameAccountRequest>,
) -> StatusCode {
    if state.profiles.rename_account(&payload.id, &payload.alias) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn refresh_data(State(state): State<AppState>) -> StatusCode {
    let active_email = state.live_quota.read().await.user_email.clone();
    let known_accounts: Vec<String> = state
        .profiles
        .get_all()
        .into_iter()
        .map(|p| p.email)
        .filter(|e| !e.is_empty())
        .collect();

    // 1. Refresh DB stats
    let new_stats = tokio::task::spawn_blocking(move || {
        let def = if !active_email.is_empty() { active_email } else { "Default".to_string() };
        scan_antigravity_data_filtered(None, &def, &known_accounts)
    })
    .await
    .unwrap_or_default();
    *state.stats.write().await = new_stats;

    // 2. Discover / poll live Antigravity
    let instance = find_antigravity_instance();
    *state.instance.write().await = instance.clone();

    if let Some(inst) = instance {
        if let Ok(quota) = fetch_live_quota(inst.ls_port, &inst.csrf_token).await {
            state.profiles.upsert_active_account(
                &quota.user_email,
                &quota.user_name,
                &quota.user_tier.name,
                Some(quota.quota.clone()),
            );
            *state.live_quota.write().await = quota;
        }
    } else {
        state.profiles.mark_all_inactive();
        let mut q = state.live_quota.write().await;
        q.connected = false;
    }

    StatusCode::OK
}

async fn create_profile_handler(
    State(state): State<AppState>,
    Json(payload): Json<CreateProfileRequest>,
) -> Result<Json<AccountProfile>, (StatusCode, String)> {
    let clean_alias = payload.alias.trim();
    if clean_alias.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Имя профиля не может быть пустым".into()));
    }

    let timestamp = chrono::Utc::now().timestamp_millis();
    let ascii_part: String = clean_alias
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();

    let safe_slug = if ascii_part.is_empty() {
        format!("profile_{}", timestamp)
    } else {
        format!("profile_{}_{}", ascii_part.to_lowercase(), timestamp)
    };

    let profile_dir = setup_profile_dir(&safe_slug)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let p = state.profiles.create_custom_profile(clean_alias, Some(profile_dir.to_string_lossy().to_string()));
    Ok(Json(p))
}

async fn launch_profile_handler(
    State(state): State<AppState>,
    Json(payload): Json<LaunchProfileRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let profile_opt = state.profiles.get_all().into_iter().find(|p| p.id == payload.id);
    let data_dir = profile_opt.as_ref().and_then(|p| p.data_dir.as_deref());

    // 1. Close running Antigravity instance so ProcessSingleton doesn't block the switch
    let _ = close_running_antigravity();
    tokio::time::sleep(tokio::time::Duration::from_millis(650)).await;

    // 2. Launch target profile
    launch_antigravity_profile(data_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "profileId": payload.id,
        "dataDir": data_dir
    })))
}

async fn close_antigravity_handler() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let killed = close_running_antigravity()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "killedCount": killed
    })))
}

async fn delete_profile_handler(
    State(state): State<AppState>,
    Json(payload): Json<DeleteProfileRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let removed = state.profiles.delete_profile(&payload.id);
    Ok(Json(serde_json::json!({
        "success": removed
    })))
}

async fn scan_cleaner_handler() -> Json<CleanerScanResult> {
    let res = tokio::task::spawn_blocking(scan_caches).await.unwrap_or(CleanerScanResult {
        items: vec![],
        total_size_bytes: 0,
        total_formatted_size: "0 Б".into(),
    });
    Json(res)
}

async fn clean_cleaner_handler(Json(payload): Json<CleanRequest>) -> Json<CleanerCleanResult> {
    let res = tokio::task::spawn_blocking(move || clean_cache_items(payload.item_ids))
        .await
        .unwrap_or(CleanerCleanResult {
            success: false,
            freed_bytes: 0,
            formatted_freed: "0 Б".into(),
            cleaned_items: vec![],
            errors: vec!["Task error".into()],
        });
    Json(res)
}

async fn open_folder_handler(Json(payload): Json<OpenFolderRequest>) -> Result<StatusCode, (StatusCode, String)> {
    open_folder_in_explorer(&payload.path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(StatusCode::OK)
}

async fn window_mini_handler(State(state): State<AppState>) -> StatusCode {
    if let Some(ref p) = state.proxy {
        let _ = p.send_event(UserWindowEvent::SetMiniMode(true));
    }
    StatusCode::OK
}

async fn window_restore_handler(State(state): State<AppState>) -> StatusCode {
    if let Some(ref p) = state.proxy {
        let _ = p.send_event(UserWindowEvent::SetMiniMode(false));
    }
    StatusCode::OK
}

async fn window_pin_handler(
    State(state): State<AppState>,
    Json(payload): Json<PinWindowRequest>,
) -> StatusCode {
    if let Some(ref p) = state.proxy {
        let _ = p.send_event(UserWindowEvent::SetAlwaysOnTop(payload.pinned));
    }
    StatusCode::OK
}

async fn window_hide_handler(State(state): State<AppState>) -> StatusCode {
    if let Some(ref p) = state.proxy {
        let _ = p.send_event(UserWindowEvent::Hide);
    }
    StatusCode::OK
}

async fn window_show_handler(State(state): State<AppState>) -> StatusCode {
    if let Some(ref p) = state.proxy {
        let _ = p.send_event(UserWindowEvent::Show);
    }
    StatusCode::OK
}

async fn check_update_handler() -> Result<Json<crate::updater::UpdateInfo>, (StatusCode, String)> {
    let info = crate::updater::check_for_updates()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(info))
}

async fn install_update_handler(
    Json(payload): Json<InstallUpdateRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let url = payload.download_url.clone();
    tokio::task::spawn(async move {
        let _ = crate::updater::download_and_hot_swap(&url).await;
    });
    Ok(StatusCode::OK)
}
