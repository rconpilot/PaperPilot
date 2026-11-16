use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// A local Studio user. Roles map onto the PaperPilot core's tiers:
/// owner/admin → admin, operator → operator, viewer → readonly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub id: String,
    pub name: String,
    pub role: String,
}

/// Studio's own settings — stored locally in the app data dir, never in the repo.
/// Field names match the Electron edition's studio-settings.json exactly, so a
/// settings file written by either edition loads in the other.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioSettings {
    pub rcon_host: String,
    pub rcon_port: u16,
    /// Kept only in this local settings file.
    pub rcon_password: String,
    /// Minecraft server directory — used to resolve logs/latest.log and backups/.
    pub server_dir: String,
    /// run_command allowlist (enforced by the PaperPilot core).
    pub allowlist: Vec<String>,
    /// Require an explicit confirmation before admin-tier tools run from the UI.
    pub require_approval: bool,
    pub theme: String,
    pub accent: String,
    pub density: String,
    /// Local team: the owner plus any admins the owner added with limited roles.
    pub team: Vec<TeamMember>,
    /// Id of the team member currently using Studio (drives tool permissions).
    pub active_user_id: String,
}

impl Default for StudioSettings {
    fn default() -> Self {
        StudioSettings {
            rcon_host: "127.0.0.1".to_string(),
            rcon_port: 25575,
            rcon_password: String::new(),
            server_dir: String::new(),
            allowlist: [
                "say",
                "list",
                "tps",
                "save-all",
                "whitelist",
                "weather",
                "time set day",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            require_approval: true,
            theme: "claude".to_string(),
            accent: "#D97757".to_string(),
            density: "comfortable".to_string(),
            team: vec![TeamMember {
                id: "owner".to_string(),
                name: "Owner".to_string(),
                role: "owner".to_string(),
            }],
            active_user_id: "owner".to_string(),
        }
    }
}

/// A settings file may be partial (older editions wrote fewer fields):
/// start from the defaults and overlay whatever the file carries.
fn overlay(raw: &serde_json::Value) -> StudioSettings {
    let mut settings = StudioSettings::default();
    if let Ok(json) = serde_json::to_string(&settings) {
        if let Ok(mut base) = serde_json::from_str::<serde_json::Value>(&json) {
            if let (Some(base_obj), Some(raw_obj)) = (base.as_object_mut(), raw.as_object()) {
                for (k, v) in raw_obj {
                    base_obj.insert(k.clone(), v.clone());
                }
            }
            if let Ok(parsed) = serde_json::from_value::<StudioSettings>(base) {
                settings = parsed;
            }
        }
    }
    settings
}

/// Load settings from the primary path; when it does not exist yet, fall back
/// to the Electron edition's settings file (one-time import of the user's
/// server, team and theme when they switch editions).
pub fn load_settings(primary: &Path, legacy_candidates: &[PathBuf]) -> StudioSettings {
    if let Some(settings) = try_load(primary) {
        return settings;
    }
    for candidate in legacy_candidates {
        if let Some(settings) = try_load(candidate) {
            return settings;
        }
    }
    StudioSettings::default()
}

fn try_load(path: &Path) -> Option<StudioSettings> {
    let text = fs::read_to_string(path).ok()?;
    let raw: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(overlay(&raw))
}

pub fn save_settings(path: &Path, settings: &StudioSettings) -> std::io::Result<StudioSettings> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(settings).unwrap_or_else(|_| "{}".to_string());
    fs::write(path, text)?;
    Ok(settings.clone())
}

/// Where the Electron edition kept its settings, per OS — used once to import
/// them into the Tauri edition.
pub fn legacy_settings_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            out.push(
                PathBuf::from(appdata)
                    .join("paperpilot-studio")
                    .join("studio-settings.json"),
            );
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            out.push(
                PathBuf::from(home)
                    .join("Library/Application Support/paperpilot-studio/studio-settings.json"),
            );
        }
    } else if let Ok(home) = std::env::var("HOME") {
        out.push(
            PathBuf::from(home)
                .join(".config/paperpilot-studio/studio-settings.json"),
        );
    }
    out
}
