use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// One line of the JSONL audit log. Field order and names match the
/// TypeScript core's AuditEntry (+ the Studio `user` stamp) exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub ts: String,
    pub tool: String,
    pub args: String,
    pub outcome: String,
    pub tier: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub user: Option<String>,
}

/// Summarize tool arguments for the audit log, truncated to keep lines small.
pub fn summarize_args(args: &serde_json::Value) -> String {
    let text = serde_json::to_string(args).unwrap_or_default();
    if text.chars().count() > 200 {
        let cut: String = text.chars().take(200).collect();
        format!("{cut}…")
    } else {
        text
    }
}

/// Appends one JSON object per line to the audit log file. Writes are
/// synchronous on purpose: an audit entry must be on disk before the
/// corresponding tool call is considered complete. Every entry is stamped
/// with the active Studio user's name (the TS UserAuditLogger behaviour).
pub struct AuditLogger {
    path: PathBuf,
    get_user: Box<dyn Fn() -> String + Send + Sync>,
}

impl AuditLogger {
    pub fn new(path: PathBuf, get_user: impl Fn() -> String + Send + Sync + 'static) -> Self {
        AuditLogger {
            path,
            get_user: Box::new(get_user),
        }
    }

    pub fn log(&self, mut entry: AuditEntry) {
        entry.user = Some((self.get_user)());
        if let Some(dir) = self.path.parent() {
            if !dir.as_os_str().is_empty() {
                let _ = fs::create_dir_all(dir);
            }
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.path) {
            if let Ok(line) = serde_json::to_string(&entry) {
                let _ = writeln!(file, "{line}");
            }
        }
    }
}

/// ISO-8601 timestamp in the exact shape JS `Date.toISOString()` produces
/// (millisecond precision, trailing Z) — the audit log and the UI both use it.
pub fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub fn iso_from_system_time(t: std::time::SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Utc> = t.into();
    dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}
