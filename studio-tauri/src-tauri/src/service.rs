use crate::audit::{iso_from_system_time, summarize_args, AuditEntry, AuditLogger};
use crate::rcon::RconClient;
use crate::settings::{load_settings, save_settings, legacy_settings_candidates, StudioSettings, TeamMember};
use crate::tools::{self, CoreConfig, ToolCtx, ToolInfo};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub fn role_tier(role: &str) -> &'static str {
    match role {
        "operator" => "operator",
        "viewer" => "readonly",
        _ => "admin",
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub players_online: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub players_max: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub players: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tps: Option<[f64; 3]>,
    /// Outer None = no TPS line seen (field omitted); inner None = TPS seen
    /// but the server reported no MSPT (serialized as null), like the TS core.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mspt: Option<Option<f64>>,
    pub raw: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub name: String,
    pub size_bytes: u64,
    pub modified: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupsResponse {
    pub entries: Vec<BackupInfo>,
    pub total_bytes: u64,
    pub dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GithubInfo {
    pub stars: Option<i64>,
    pub url: String,
}

/// Hosts the Rust port of the PaperPilot core in-process — the same tools,
/// tier checks, allowlist and audit log as the TypeScript core that ships in
/// the PaperPilot repo, so the UI can never do anything the MCP server
/// couldn't. Mirrors the Electron edition's PaperPilotService method for
/// method.
pub struct Service {
    settings: StudioSettings,
    settings_path: PathBuf,
    audit_path: PathBuf,
    rcon: RconClient,
    core: CoreConfig,
    audit: AuditLogger,
    user_cell: Arc<Mutex<String>>,
    gh_cache: (Option<i64>, Option<Instant>),
}

impl Service {
    pub fn new(data_dir: PathBuf) -> Self {
        let settings_path = data_dir.join("studio-settings.json");
        let audit_path = data_dir.join("paperpilot-audit.jsonl");
        let settings = load_settings(&settings_path, &legacy_settings_candidates());
        let user_cell = Arc::new(Mutex::new("Owner".to_string()));
        let cell = user_cell.clone();
        let audit = AuditLogger::new(audit_path.clone(), move || {
            cell.lock().map(|g| g.clone()).unwrap_or_else(|_| "Owner".to_string())
        });
        let mut service = Service {
            settings,
            settings_path,
            audit_path,
            rcon: RconClient::new("127.0.0.1", 25575, ""),
            core: CoreConfig {
                allowlist: vec![],
                log_path: PathBuf::from("logs/latest.log"),
                backup_dir: PathBuf::from("backups"),
            },
            audit,
            user_cell,
            gh_cache: (None, None),
        };
        service.rebuild();
        service
    }

    fn rebuild(&mut self) {
        let s = &self.settings;
        let server_dir = s.server_dir.trim();
        let password = std::env::var("PAPERPILOT_RCON_PASSWORD")
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| std::env::var("RCON_PASSWORD").ok().filter(|v| !v.is_empty()))
            .unwrap_or_else(|| s.rcon_password.clone());
        let (log_path, backup_dir) = if server_dir.is_empty() {
            (
                PathBuf::from("logs/latest.log"),
                PathBuf::from("backups"),
            )
        } else {
            (
                PathBuf::from(server_dir).join("logs").join("latest.log"),
                PathBuf::from(server_dir).join("backups"),
            )
        };
        self.core = CoreConfig {
            allowlist: s.allowlist.clone(),
            log_path,
            backup_dir,
        };
        self.rcon = RconClient::new(
            if s.rcon_host.trim().is_empty() { "127.0.0.1" } else { s.rcon_host.trim() },
            if s.rcon_port == 0 { 25575 } else { s.rcon_port },
            &password,
        );
        let name = self.active_user().name;
        if let Ok(mut cell) = self.user_cell.lock() {
            *cell = name;
        }
    }

    /// The team member currently using Studio (falls back to the owner).
    pub fn active_user(&self) -> TeamMember {
        let team = &self.settings.team;
        team.iter()
            .find(|m| m.id == self.settings.active_user_id)
            .cloned()
            .or_else(|| team.first().cloned())
            .unwrap_or(TeamMember {
                id: "owner".to_string(),
                name: "Owner".to_string(),
                role: "owner".to_string(),
            })
    }

    pub fn list_tools(&self) -> Vec<ToolInfo> {
        tools::tool_infos()
    }

    pub fn call_tool(&mut self, name: &str, args: Value) -> CallResult {
        let required = match tools::tool_tier(name) {
            Some(t) => t,
            None => {
                return CallResult {
                    ok: false,
                    text: None,
                    error: Some(format!("Unknown tool \"{name}\".")),
                }
            }
        };
        // Team gate: the core instance runs at admin tier, so Studio enforces
        // the ACTIVE USER's tier here — a viewer can never run an
        // operator/admin tool, and the denial lands in the audit log under
        // their name.
        let user = self.active_user();
        let tier = role_tier(&user.role);
        if !tools::tier_at_least(tier, required) {
            let error = format!(
                "\"{name}\" needs the {required} tier — {} is a {} ({tier}).",
                user.name, user.role
            );
            self.audit.log(AuditEntry {
                ts: crate::audit::now_iso(),
                tool: name.to_string(),
                args: summarize_args(&args),
                outcome: "denied".to_string(),
                tier: tier.to_string(),
                detail: Some(error.clone()),
                user: None,
            });
            return CallResult {
                ok: false,
                text: None,
                error: Some(error),
            };
        }
        let mut ctx = ToolCtx {
            config: &self.core,
            rcon: &mut self.rcon,
            audit: &self.audit,
        };
        match tools::call_tool(&mut ctx, name, &args) {
            Ok(text) => CallResult {
                ok: true,
                text: Some(text),
                error: None,
            },
            Err(msg) => CallResult {
                ok: false,
                text: None,
                error: Some(msg),
            },
        }
    }

    /// server_status, parsed into fields for the dashboard.
    pub fn status(&mut self) -> ServerStatus {
        let res = self.call_tool("server_status", Value::Object(Default::default()));
        if !res.ok {
            return ServerStatus {
                online: false,
                version: None,
                players_online: None,
                players_max: None,
                players: None,
                tps: None,
                mspt: None,
                raw: String::new(),
                error: res.error,
            };
        }
        let raw = res.text.unwrap_or_default();
        let mut out = ServerStatus {
            online: true,
            version: None,
            players_online: None,
            players_max: None,
            players: None,
            tps: None,
            mspt: None,
            raw: raw.clone(),
            error: None,
        };
        let players_count_re =
            Regex::new(r"Players:\s*(\d+|\?)\s*/\s*(\d+|\?)").unwrap();
        let num_re = Regex::new(r"[\d.]+").unwrap();
        let mspt_re = Regex::new(r"MSPT:\s*([\d.]+)").unwrap();
        for line in raw.split('\n') {
            if let Some(rest) = line.strip_prefix("Version:") {
                out.version = Some(rest.trim().to_string());
            }
            if line.starts_with("Players:") {
                if let Some(m) = players_count_re.captures(line) {
                    out.players_online = m[1].parse().ok();
                    out.players_max = m[2].parse().ok();
                }
                let names = line.split('—').nth(1).map(|s| s.trim().to_string());
                out.players = Some(match names {
                    Some(n) if !n.is_empty() => n
                        .split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect(),
                    _ => vec![],
                });
            }
            if line.starts_with("TPS") {
                // Parse only after the colon — the "(1m/5m/15m)" label itself
                // contains numbers that are not TPS values.
                if let Some(colon) = line.find(':') {
                    let after = &line[colon + 1..];
                    let nums: Vec<f64> = num_re
                        .find_iter(after)
                        .filter_map(|m| m.as_str().parse().ok())
                        .collect();
                    if nums.len() >= 3 {
                        out.tps = Some([nums[0], nums[1], nums[2]]);
                    }
                }
                out.mspt = Some(
                    mspt_re
                        .captures(line)
                        .and_then(|c| c[1].parse::<f64>().ok()),
                );
            }
        }
        // Some Paper builds report the player count in a format the core's
        // parser marks as "?". Fall back to the raw list_players text so the
        // dashboard still shows the real numbers.
        if out.players_online.is_none() {
            let lp = self.call_tool("list_players", Value::Object(Default::default()));
            if lp.ok {
                if let Some(text) = lp.text {
                    let clean = {
                        let mut s = String::with_capacity(text.len());
                        let mut chars = text.chars();
                        while let Some(c) = chars.next() {
                            if c == '§' {
                                let _ = chars.next();
                            } else {
                                s.push(c);
                            }
                        }
                        s
                    };
                    let fallback_re =
                        Regex::new(r"There are\s+(\d+)\s+out of maximum\s+(\d+)").unwrap();
                    if let Some(m) = fallback_re.captures(&clean) {
                        out.players_online = m[1].parse().ok();
                        out.players_max = m[2].parse().ok();
                    }
                    if out.players.as_ref().map(|p| p.is_empty()).unwrap_or(true) {
                        let names = clean
                            .split(':')
                            .skip(1)
                            .collect::<Vec<_>>()
                            .join(":")
                            .trim()
                            .to_string();
                        if !names.is_empty() && out.players_online.map(|n| n > 0).unwrap_or(false)
                        {
                            out.players = Some(
                                names
                                    .split(',')
                                    .map(|x| x.trim().to_string())
                                    .filter(|x| !x.is_empty())
                                    .collect(),
                            );
                        } else if out.players_online == Some(0) {
                            out.players = Some(vec![]);
                        }
                    }
                }
            }
        }
        out
    }

    /// Backup folders/files in the configured backup dir — read from disk.
    pub fn list_backups(&self) -> BackupsResponse {
        let dir = if self.core.backup_dir.is_absolute() {
            self.core.backup_dir.clone()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&self.core.backup_dir))
                .unwrap_or_else(|_| self.core.backup_dir.clone())
        };
        let mut entries: Vec<BackupInfo> = Vec::new();
        if let Ok(read) = std::fs::read_dir(&dir) {
            for entry in read.flatten() {
                if let Ok(meta) = entry.metadata() {
                    let modified = meta
                        .modified()
                        .map(iso_from_system_time)
                        .unwrap_or_default();
                    entries.push(BackupInfo {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        size_bytes: meta.len(),
                        modified,
                    });
                }
            }
        }
        entries.sort_by(|a, b| b.modified.cmp(&a.modified));
        BackupsResponse {
            total_bytes: entries.iter().map(|e| e.size_bytes).sum(),
            dir: dir.to_string_lossy().into_owned(),
            entries,
        }
    }

    /// Last N entries of the core's JSONL audit log.
    pub fn read_audit(&self, limit: usize) -> Vec<AuditEntry> {
        let content = match std::fs::read_to_string(&self.audit_path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let lines: Vec<&str> = content
            .trim()
            .split('\n')
            .filter(|l| !l.is_empty())
            .collect();
        let start = lines.len().saturating_sub(limit);
        let mut out: Vec<AuditEntry> = Vec::new();
        for line in &lines[start..] {
            match serde_json::from_str::<AuditEntry>(line) {
                Ok(entry) => out.push(entry),
                // The TS bridge returns [] when any line in the window fails
                // to parse; mirror that.
                Err(_) => return vec![],
            }
        }
        out.reverse();
        out
    }

    /// GitHub repo info for the header bar — star count, cached for 10 min.
    pub fn github_info(&mut self) -> GithubInfo {
        let url = "https://github.com/rconpilot/PaperPilot".to_string();
        if let (Some(stars), Some(at)) = self.gh_cache {
            if at.elapsed().as_secs() < 10 * 60 {
                return GithubInfo {
                    stars: Some(stars),
                    url,
                };
            }
        }
        let fetched = (|| -> Option<i64> {
            let mut resp = ureq::get("https://api.github.com/repos/rconpilot/PaperPilot")
                .header("User-Agent", "PaperPilot-Studio")
                .call()
                .ok()?;
            let text = resp.body_mut().read_to_string().ok()?;
            let json: Value = serde_json::from_str(&text).ok()?;
            json.get("stargazers_count").and_then(|v| v.as_i64())
        })();
        if let Some(stars) = fetched {
            self.gh_cache = (Some(stars), Some(Instant::now()));
        }
        GithubInfo {
            stars: self.gh_cache.0,
            url,
        }
    }

    pub fn get_settings(&self) -> StudioSettings {
        self.settings.clone()
    }

    pub fn set_settings(&mut self, next: StudioSettings) -> StudioSettings {
        let _ = save_settings(&self.settings_path, &next);
        self.settings = next;
        self.rebuild();
        self.settings.clone()
    }
}
