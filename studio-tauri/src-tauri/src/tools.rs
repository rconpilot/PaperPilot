use crate::audit::{now_iso, AuditEntry, AuditLogger};
use crate::rcon::RconClient;
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Tiers (core/config.ts)
// ---------------------------------------------------------------------------

pub fn tier_rank(tier: &str) -> i32 {
    match tier {
        "readonly" => 0,
        "operator" => 1,
        "admin" => 2,
        _ => 0,
    }
}

pub fn tier_at_least(actual: &str, required: &str) -> bool {
    tier_rank(actual) >= tier_rank(required)
}

// ---------------------------------------------------------------------------
// Tool metadata (for list_tools — mirrors the zod schemas in core/tools.ts)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ToolArg {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub tier: String,
    pub args: Vec<ToolArg>,
}

fn arg(name: &str, description: &str, required: bool, kind: &str) -> ToolArg {
    ToolArg {
        name: name.to_string(),
        description: description.to_string(),
        required,
        kind: kind.to_string(),
    }
}

fn info(name: &str, description: &str, tier: &str, args: Vec<ToolArg>) -> ToolInfo {
    ToolInfo {
        name: name.to_string(),
        description: description.to_string(),
        tier: tier.to_string(),
        args,
    }
}

const PLAYER_DESC: &str = "Minecraft username (1-16 letters, digits or underscores)";

pub fn tool_infos() -> Vec<ToolInfo> {
    vec![
        info("server_status", "Get the Paper server's version, online/max players, player names and parsed TPS (1m/5m/15m, plus MSPT when reported) (runs version, list and tps via RCON).", "readonly", vec![]),
        info("list_players", "List the players currently online on the Paper server.", "readonly", vec![]),
        info("list_plugins", "List the plugins installed on the Paper server (runs plugins via RCON).", "readonly", vec![]),
        info("player_info", "Show info about an online player: position, health, food level, XP level and gamemode (runs data get entity via RCON).", "readonly", vec![
            arg("player", PLAYER_DESC, true, "string"),
        ]),
        info("read_logs", "Read the tail of the Minecraft server log file, optionally filtered by a search substring.", "readonly", vec![
            arg("lines", "How many lines to return (default 50, max 500)", false, "number"),
            arg("search", "Optional case-insensitive substring filter", false, "string"),
        ]),
        info("diagnose_crash", "Scan the tail of the server log for crash indicators (crash reports, OutOfMemoryError, ticking exceptions, watchdog stalls) and report the matching lines, if any.", "readonly", vec![
            arg("lines", "How many trailing log lines to scan (default 200, max 500)", false, "number"),
        ]),
        info("run_command", "Run a server command via RCON. Only commands matching the configured allowlist are accepted; everything else is rejected.", "admin", vec![
            arg("command", "The full command to run, without a leading slash", true, "string"),
        ]),
        info("kick_player", "Kick a player from the server, with an optional reason.", "operator", vec![
            arg("player", PLAYER_DESC, true, "string"),
            arg("reason", "Optional kick reason shown to the player", false, "string"),
        ]),
        info("whitelist_add", "Add a player to the server whitelist.", "operator", vec![
            arg("player", PLAYER_DESC, true, "string"),
        ]),
        info("whitelist_remove", "Remove a player from the server whitelist.", "operator", vec![
            arg("player", PLAYER_DESC, true, "string"),
        ]),
        info("ban_player", "Ban a player from the server, with an optional reason.", "admin", vec![
            arg("player", PLAYER_DESC, true, "string"),
            arg("reason", "Optional ban reason", false, "string"),
        ]),
        info("backup_world", "Prepare the world for a backup: runs save-all, then save-off, then save-on via RCON and reports each step's response. (Copy the world files while saving is off on the server side / via your backup tooling.)", "admin", vec![]),
        info("rotate_backups", "Prune the configured backup directory (backupDir) to the newest N backups (default 7), deleting older backup files/directories. Pass dryRun=true to list what would be deleted without deleting anything. Refuses to run when backupDir is missing or resolves to the filesystem root or the working directory.", "admin", vec![
            arg("keep", "How many of the newest backups to keep (default 7, max 100)", false, "number"),
            arg("dryRun", "When true, only report what would be deleted", false, "boolean"),
        ]),
    ]
}

pub fn tool_tier(name: &str) -> Option<&'static str> {
    match name {
        "server_status" | "list_players" | "list_plugins" | "player_info" | "read_logs"
        | "diagnose_crash" => Some("readonly"),
        "kick_player" | "whitelist_add" | "whitelist_remove" => Some("operator"),
        "run_command" | "ban_player" | "backup_world" | "rotate_backups" => Some("admin"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Parsers (core/tools.ts) — same regexes, same fallbacks
// ---------------------------------------------------------------------------

static LIST_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"There are (\d+) of a max of (\d+) players online:?\s*(.*)")
        .case_insensitive(true)
        .dot_matches_new_line(true)
        .build()
        .unwrap()
});
static VERSION_RE1: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"running .+? version ([^\s]+)")
        .case_insensitive(true)
        .build()
        .unwrap()
});
static VERSION_RE2: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"version ([^\s]+)")
        .case_insensitive(true)
        .build()
        .unwrap()
});
static TPS_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"TPS from last 1m, 5m, 15m:\s*([0-9.]+),\s*([0-9.]+),\s*([0-9.]+)")
        .case_insensitive(true)
        .build()
        .unwrap()
});
static MSPT_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"MSPT[:\s]+([0-9.]+)")
        .case_insensitive(true)
        .build()
        .unwrap()
});
static PLUGINS_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"Plugins\s*\(\d+\)\s*:\s*(.*)")
        .case_insensitive(true)
        .dot_matches_new_line(true)
        .build()
        .unwrap()
});
static DATA_GET_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"has the following entity data:\s*(.+)")
        .case_insensitive(true)
        .dot_matches_new_line(true)
        .build()
        .unwrap()
});
static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("\u{1b}\\[[0-9;]*m").unwrap());
static PLAYER_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_]{1,16}$").unwrap());
static FLOAT_SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([0-9])f$").unwrap());

fn strip_codes(raw: &str) -> String {
    // JS: raw.replace(/§./g, "") — remove § and the following character.
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            let _ = chars.next();
        } else {
            out.push(c);
        }
    }
    ANSI_RE.replace_all(&out, "").into_owned()
}

/// Format a float the way JS String(number) would (20.0 → "20").
fn js_num(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

pub struct ParsedList {
    pub online: Option<u32>,
    pub max: Option<u32>,
    pub players: Vec<String>,
}

pub fn parse_list_output(raw: &str) -> ParsedList {
    match LIST_RE.captures(raw) {
        None => ParsedList {
            online: None,
            max: None,
            players: vec![],
        },
        Some(caps) => {
            let names = caps[3]
                .split(',')
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .collect();
            ParsedList {
                online: caps[1].parse().ok(),
                max: caps[2].parse().ok(),
                players: names,
            }
        }
    }
}

pub fn parse_version_output(raw: &str) -> String {
    if let Some(caps) = VERSION_RE1.captures(raw) {
        return caps[1].to_string();
    }
    if let Some(caps) = VERSION_RE2.captures(raw) {
        return caps[1].to_string();
    }
    raw.trim().to_string()
}

pub struct ParsedTps {
    pub tps: [f64; 3],
    pub mspt: Option<f64>,
}

pub fn parse_tps_output(raw: &str) -> Option<ParsedTps> {
    let stripped = strip_codes(raw);
    let caps = TPS_RE.captures(&stripped)?;
    let mspt = MSPT_RE
        .captures(&stripped)
        .and_then(|c| c[1].parse::<f64>().ok());
    Some(ParsedTps {
        tps: [
            caps[1].parse().ok()?,
            caps[2].parse().ok()?,
            caps[3].parse().ok()?,
        ],
        mspt,
    })
}

pub fn parse_plugins_output(raw: &str) -> Option<Vec<String>> {
    let stripped = strip_codes(raw);
    let caps = PLUGINS_RE.captures(&stripped)?;
    Some(
        caps[1]
            .split(',')
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect(),
    )
}

pub fn parse_data_get_output(raw: &str) -> Option<String> {
    let stripped = strip_codes(raw);
    DATA_GET_RE
        .captures(&stripped)
        .map(|c| c[1].trim().to_string())
}

pub fn tail_log(content: &str, lines: usize, search: Option<&str>) -> Vec<String> {
    let mut all: Vec<String> = content
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    if all.last().map(|l| l.is_empty()) == Some(true) {
        all.pop();
    }
    if let Some(needle) = search {
        if !needle.is_empty() {
            let needle = needle.to_lowercase();
            all.retain(|line| line.to_lowercase().contains(&needle));
        }
    }
    let start = all.len().saturating_sub(lines);
    all[start..].to_vec()
}

static CRASH_RES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"crash[ -]?report",
        r"outofmemoryerror",
        r"ticking (entity|blockentity|world|player)",
        r"exception ticking",
        r"server has stopped responding",
        r"a single server tick took",
        r"java\.lang\.\w*(?:exception|error)",
        r"failed to tick",
    ]
    .iter()
    .map(|p| {
        RegexBuilder::new(p)
            .case_insensitive(true)
            .build()
            .unwrap()
    })
    .collect()
});

pub fn find_crash_indicators(content: &str) -> Vec<String> {
    content
        .split('\n')
        .map(|line| line.trim().to_string())
        .filter(|line| {
            !line.is_empty() && CRASH_RES.iter().any(|re| re.is_match(line))
        })
        .collect()
}

fn gamemode_name(value: &str) -> Option<&'static str> {
    match value {
        "0" => Some("Survival"),
        "1" => Some("Creative"),
        "2" => Some("Adventure"),
        "3" => Some("Spectator"),
        _ => None,
    }
}

/// Check whether `command` is allowed by the allowlist: the command must
/// equal an entry or start with an entry followed by a space
/// (case-insensitive). E.g. entry "time set" allows "time set day".
pub fn is_command_allowed(command: &str, allowlist: &[String]) -> bool {
    let normalized = normalize_command(command);
    if normalized.is_empty() {
        return false;
    }
    allowlist.iter().any(|entry| {
        let prefix = normalize_command(entry);
        !prefix.is_empty()
            && (normalized == prefix || normalized.starts_with(&format!("{prefix} ")))
    })
}

fn normalize_command(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub struct BackupEntry {
    pub name: String,
    pub mtime_ms: i64,
}

/// Pick the backups to delete: sort newest-first (name breaks mtime ties so
/// the result is deterministic) and return everything past the `keep` newest.
pub fn select_backups_to_delete(entries: &[BackupEntry], keep: usize) -> Vec<String> {
    let mut sorted: Vec<&BackupEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| b.mtime_ms.cmp(&a.mtime_ms).then_with(|| a.name.cmp(&b.name)));
    sorted
        .into_iter()
        .skip(keep)
        .map(|e| e.name.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Tool execution (guarded wrapper + the 13 handlers)
// ---------------------------------------------------------------------------

pub struct CoreConfig {
    pub allowlist: Vec<String>,
    pub log_path: PathBuf,
    pub backup_dir: PathBuf,
}

pub struct ToolCtx<'a> {
    pub config: &'a CoreConfig,
    pub rcon: &'a mut RconClient,
    pub audit: &'a AuditLogger,
}

/// The core instance's own tier. In Studio this is always "admin" — the
/// per-user team gate runs in the service before a handler is reached.
const INSTANCE_TIER: &str = "admin";

fn require_player_name(value: &Value) -> Result<String, String> {
    match value.as_str() {
        Some(s) if PLAYER_NAME_RE.is_match(s) => Ok(s.to_string()),
        _ => Err(format!(
            "Invalid player {}: expected a Minecraft username (1-16 letters, digits or underscores).",
            serde_json::to_string(value).unwrap_or_else(|_| "undefined".to_string())
        )),
    }
}

fn get_number(args: &Value, key: &str) -> Option<f64> {
    args.get(key).and_then(|v| v.as_f64())
}

fn get_string(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn reason_suffix(args: &Value) -> String {
    match get_string(args, "reason") {
        Some(r) if !r.trim().is_empty() => format!(" {}", r.trim()),
        _ => String::new(),
    }
}

/// Wrap a tool implementation with tier enforcement + audit logging, exactly
/// like the TS `guarded`: every call is audited — "denied" when a check
/// rejects it, otherwise "allowed" (with the error attached if execution
/// itself failed).
fn guarded(
    ctx: &mut ToolCtx,
    name: &str,
    required_tier: &str,
    args: &Value,
    extra_check: Option<fn(&ToolCtx, &Value) -> Result<(), String>>,
    run: fn(&mut ToolCtx, &Value) -> Result<String, String>,
) -> Result<String, String> {
    let base = |outcome: &str, detail: Option<String>| AuditEntry {
        ts: now_iso(),
        tool: name.to_string(),
        args: crate::audit::summarize_args(args),
        outcome: outcome.to_string(),
        tier: INSTANCE_TIER.to_string(),
        detail,
        user: None,
    };
    if !tier_at_least(INSTANCE_TIER, required_tier) {
        let detail = format!(
            "Tool \"{name}\" requires the \"{required_tier}\" tier, but this PaperPilot instance is configured as \"{INSTANCE_TIER}\"."
        );
        ctx.audit.log(base("denied", Some(detail.clone())));
        return Err(detail);
    }
    if let Some(check) = extra_check {
        if let Err(msg) = check(ctx, args) {
            ctx.audit.log(base("denied", Some(msg.clone())));
            return Err(msg);
        }
    }
    match run(ctx, args) {
        Ok(result) => {
            ctx.audit.log(base("allowed", None));
            Ok(result)
        }
        Err(msg) => {
            ctx.audit
                .log(base("allowed", Some(format!("Execution error: {msg}"))));
            Err(msg)
        }
    }
}

pub fn call_tool(ctx: &mut ToolCtx, name: &str, args: &Value) -> Result<String, String> {
    match name {
        "server_status" => guarded(ctx, name, "readonly", args, None, |ctx, _| {
            let version_raw = ctx.rcon.send("version")?;
            let list_raw = ctx.rcon.send("list")?;
            let tps_raw = ctx.rcon.send("tps")?;
            let list = parse_list_output(&list_raw);
            let tps_line = match parse_tps_output(&tps_raw) {
                None => format!("TPS (raw): {}", tps_raw.trim()),
                Some(t) => {
                    let mut line = format!(
                        "TPS (1m/5m/15m): {}, {}, {}",
                        js_num(t.tps[0]),
                        js_num(t.tps[1]),
                        js_num(t.tps[2])
                    );
                    if let Some(mspt) = t.mspt {
                        line.push_str(&format!(" — MSPT: {}", js_num(mspt)));
                    }
                    line
                }
            };
            let players_line = format!(
                "Players: {} / {} online{}",
                list.online.map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
                list.max.map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
                if list.players.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", list.players.join(", "))
                }
            );
            Ok(format!(
                "Version: {}\n{players_line}\n{tps_line}",
                parse_version_output(&version_raw)
            ))
        }),
        "list_players" => guarded(ctx, name, "readonly", args, None, |ctx, _| {
            let raw = ctx.rcon.send("list")?;
            let list = parse_list_output(&raw);
            if list.online.is_none() {
                return Ok(raw.trim().to_string());
            }
            if list.players.is_empty() {
                return Ok(format!(
                    "No players online (max {}).",
                    list.max.map(|n| n.to_string()).unwrap_or_else(|| "?".into())
                ));
            }
            Ok(format!(
                "{} player(s) online: {}",
                list.online.unwrap_or(0),
                list.players.join(", ")
            ))
        }),
        "list_plugins" => guarded(ctx, name, "readonly", args, None, |ctx, _| {
            let raw = ctx.rcon.send("plugins")?;
            match parse_plugins_output(&raw) {
                None => Ok(raw.trim().to_string()),
                Some(plugins) if plugins.is_empty() => Ok("No plugins installed.".to_string()),
                Some(plugins) => Ok(format!(
                    "{} plugin(s): {}",
                    plugins.len(),
                    plugins.join(", ")
                )),
            }
        }),
        "player_info" => guarded(ctx, name, "readonly", args, None, |ctx, args| {
            let player = require_player_name(args.get("player").unwrap_or(&Value::Null))?;
            let pos = parse_data_get_output(&ctx.rcon.send(&format!("data get entity {player} Pos"))?);
            let pos = match pos {
                None => return Ok(format!("{player} is not online (no entity data found).")),
                Some(p) => p,
            };
            let health = parse_data_get_output(&ctx.rcon.send(&format!("data get entity {player} Health"))?);
            let food = parse_data_get_output(&ctx.rcon.send(&format!("data get entity {player} foodLevel"))?);
            let xp = parse_data_get_output(&ctx.rcon.send(&format!("data get entity {player} XpLevel"))?);
            let game_type = parse_data_get_output(&ctx.rcon.send(&format!("data get entity {player} playerGameType"))?);
            let gamemode = match &game_type {
                None => "unknown".to_string(),
                Some(v) => gamemode_name(v)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| format!("unknown ({v})")),
            };
            let strip_float = |value: &Option<String>| -> String {
                match value {
                    None => "unknown".to_string(),
                    Some(v) => FLOAT_SUFFIX_RE.replace(v, "$1").into_owned(),
                }
            };
            Ok([
                format!("Player: {player}"),
                format!("Position: {pos}"),
                format!("Health: {}", strip_float(&health)),
                format!("Food level: {}", food.unwrap_or_else(|| "unknown".into())),
                format!("XP level: {}", xp.unwrap_or_else(|| "unknown".into())),
                format!("Gamemode: {gamemode}"),
            ]
            .join("\n"))
        }),
        "read_logs" => guarded(ctx, name, "readonly", args, None, |ctx, args| {
            let lines = get_number(args, "lines").map(|n| n as usize).unwrap_or(50);
            let search = get_string(args, "search");
            if !ctx.config.log_path.exists() {
                return Err(format!("Log file not found: {}", ctx.config.log_path.display()));
            }
            let content = std::fs::read_to_string(&ctx.config.log_path)
                .map_err(|e| e.to_string())?;
            let tail = tail_log(&content, lines, search.as_deref());
            if tail.is_empty() {
                return Ok(match &search {
                    Some(s) => format!("No log lines match \"{s}\"."),
                    None => "The log file is empty.".to_string(),
                });
            }
            Ok(tail.join("\n"))
        }),
        "diagnose_crash" => guarded(ctx, name, "readonly", args, None, |ctx, args| {
            let lines = get_number(args, "lines").map(|n| n as usize).unwrap_or(200);
            if !ctx.config.log_path.exists() {
                return Err(format!("Log file not found: {}", ctx.config.log_path.display()));
            }
            let content = std::fs::read_to_string(&ctx.config.log_path)
                .map_err(|e| e.to_string())?;
            let tail = tail_log(&content, lines, None);
            let hits = find_crash_indicators(&tail.join("\n"));
            if hits.is_empty() {
                return Ok(format!(
                    "No crash indicators found in the last {} log line(s).",
                    tail.len()
                ));
            }
            let shown: Vec<String> = hits.iter().skip(hits.len().saturating_sub(5)).cloned().collect();
            let mut out = vec![format!(
                "Found {} crash indicator(s) in the last {} log line(s):",
                hits.len(),
                tail.len()
            )];
            out.extend(shown);
            out.push("Check the server's crash-reports/ folder for the full crash report.".to_string());
            Ok(out.join("\n"))
        }),
        "run_command" => guarded(
            ctx,
            name,
            "admin",
            args,
            Some(|ctx, args| {
                let command = get_string(args, "command").unwrap_or_default();
                if !is_command_allowed(&command, &ctx.config.allowlist) {
                    let list = if ctx.config.allowlist.is_empty() {
                        "(empty)".to_string()
                    } else {
                        ctx.config.allowlist.join(", ")
                    };
                    return Err(format!(
                        "Command rejected: \"{command}\" does not match the configured allowlist [{list}]."
                    ));
                }
                Ok(())
            }),
            |ctx, args| {
                let command = match args.get("command") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Null) | None => String::new(),
                    Some(other) => serde_json::to_string(other).unwrap_or_default(),
                };
                let response = ctx.rcon.send(&command)?;
                let trimmed = response.trim();
                Ok(if trimmed.is_empty() {
                    "(command executed, no output)".to_string()
                } else {
                    trimmed.to_string()
                })
            },
        ),
        "kick_player" => guarded(ctx, name, "operator", args, None, |ctx, args| {
            let player = require_player_name(args.get("player").unwrap_or(&Value::Null))?;
            let response = ctx.rcon.send(&format!("kick {player}{}", reason_suffix(args)))?;
            let trimmed = response.trim();
            Ok(if trimmed.is_empty() {
                format!("Kicked {player}.")
            } else {
                trimmed.to_string()
            })
        }),
        "whitelist_add" => guarded(ctx, name, "operator", args, None, |ctx, args| {
            let player = require_player_name(args.get("player").unwrap_or(&Value::Null))?;
            let response = ctx.rcon.send(&format!("whitelist add {player}"))?;
            let trimmed = response.trim();
            Ok(if trimmed.is_empty() {
                format!("Added {player} to the whitelist.")
            } else {
                trimmed.to_string()
            })
        }),
        "whitelist_remove" => guarded(ctx, name, "operator", args, None, |ctx, args| {
            let player = require_player_name(args.get("player").unwrap_or(&Value::Null))?;
            let response = ctx.rcon.send(&format!("whitelist remove {player}"))?;
            let trimmed = response.trim();
            Ok(if trimmed.is_empty() {
                format!("Removed {player} from the whitelist.")
            } else {
                trimmed.to_string()
            })
        }),
        "ban_player" => guarded(ctx, name, "admin", args, None, |ctx, args| {
            let player = require_player_name(args.get("player").unwrap_or(&Value::Null))?;
            let response = ctx.rcon.send(&format!("ban {player}{}", reason_suffix(args)))?;
            let trimmed = response.trim();
            Ok(if trimmed.is_empty() {
                format!("Banned {player}.")
            } else {
                trimmed.to_string()
            })
        }),
        "backup_world" => guarded(ctx, name, "admin", args, None, |ctx, _| {
            let mut steps = Vec::new();
            for command in ["save-all", "save-off", "save-on"] {
                let response = ctx.rcon.send(command)?;
                let trimmed = response.trim();
                steps.push(format!(
                    "{command}: {}",
                    if trimmed.is_empty() { "(no output)" } else { trimmed }
                ));
            }
            Ok(steps.join("\n"))
        }),
        "rotate_backups" => guarded(ctx, name, "admin", args, None, |ctx, args| {
            let keep = get_number(args, "keep").map(|n| n as usize).unwrap_or(7);
            let dry_run = args.get("dryRun") == Some(&Value::Bool(true));
            let dir = &ctx.config.backup_dir;
            if !dir.exists() {
                return Err(format!("Backup directory not found: {}", dir.display()));
            }
            // Safety: never prune the filesystem root or the working directory
            // itself (a misconfigured backupDir must not wipe a whole tree).
            let resolved = std::fs::canonicalize(dir).map_err(|e| e.to_string())?;
            let cwd = std::env::current_dir()
                .ok()
                .and_then(|p| std::fs::canonicalize(p).ok());
            if resolved.parent().is_none() || cwd.as_deref() == Some(resolved.as_path()) {
                return Err(format!(
                    "Refusing to rotate backups in unsafe directory: {}",
                    dir.display()
                ));
            }
            let mut entries: Vec<BackupEntry> = Vec::new();
            let read = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
            for entry in read.flatten() {
                if let Ok(meta) = entry.metadata() {
                    let mtime_ms = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    entries.push(BackupEntry {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        mtime_ms,
                    });
                }
            }
            let to_delete = select_backups_to_delete(&entries, keep);
            let kept = entries.len() - to_delete.len();
            if to_delete.is_empty() {
                return Ok(format!(
                    "Nothing to delete: {} backup(s) in {}, keeping {kept} (keep={keep}).",
                    entries.len(),
                    dir.display()
                ));
            }
            if !dry_run {
                for name in &to_delete {
                    let path = dir.join(name);
                    if path.is_dir() {
                        let _ = std::fs::remove_dir_all(&path);
                    } else {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
            Ok(format!(
                "{} {} backup(s) from {}: {}.\nKept {kept} newest backup(s) (keep={keep}).",
                if dry_run { "Dry run — would delete" } else { "Deleted" },
                to_delete.len(),
                dir.display(),
                to_delete.join(", ")
            ))
        }),
        _ => Err(format!("Unknown tool \"{name}\".")),
    }
}
