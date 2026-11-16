mod ai;
mod audit;
mod rcon;
mod service;
mod settings;
mod tools;

use ai::{AiChatRequest, AiChatResponse};
use audit::AuditEntry;
use service::{BackupsResponse, CallResult, GithubInfo, ServerStatus, Service};
use settings::StudioSettings;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};
use tools::ToolInfo;

pub struct AppState(pub Arc<Mutex<Service>>);

#[tauri::command]
fn list_tools(state: State<'_, AppState>) -> Vec<ToolInfo> {
    state.0.lock().unwrap().list_tools()
}

#[tauri::command]
async fn call_tool(
    state: State<'_, AppState>,
    name: String,
    args: serde_json::Value,
) -> Result<CallResult, String> {
    let svc = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        svc.lock().unwrap().call_tool(&name, args)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn status(state: State<'_, AppState>) -> Result<ServerStatus, String> {
    let svc = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || svc.lock().unwrap().status())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn backups(state: State<'_, AppState>) -> BackupsResponse {
    state.0.lock().unwrap().list_backups()
}

#[tauri::command]
fn audit(state: State<'_, AppState>, limit: Option<usize>) -> Vec<AuditEntry> {
    state.0.lock().unwrap().read_audit(limit.unwrap_or(200))
}

#[tauri::command]
async fn github(state: State<'_, AppState>) -> Result<GithubInfo, String> {
    let svc = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || svc.lock().unwrap().github_info())
        .await
        .map_err(|e| e.to_string())
}

/// One AI Copilot round-trip. The model only PROPOSES tool calls here;
/// execution always goes through call_tool with its tier/allowlist guards.
#[tauri::command]
async fn ai_chat(req: AiChatRequest) -> Result<AiChatResponse, String> {
    tauri::async_runtime::spawn_blocking(move || ai::ai_chat(&req))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> StudioSettings {
    state.0.lock().unwrap().get_settings()
}

#[tauri::command]
fn set_settings(state: State<'_, AppState>, next: StudioSettings) -> StudioSettings {
    state.0.lock().unwrap().set_settings(next)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            app.manage(AppState(Arc::new(Mutex::new(Service::new(data_dir)))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_tools,
            call_tool,
            status,
            backups,
            audit,
            github,
            ai_chat,
            get_settings,
            set_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running PaperPilot Studio");
}
