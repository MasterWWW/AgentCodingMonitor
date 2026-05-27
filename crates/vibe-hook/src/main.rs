use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::env;
use std::io::{self, Read};

fn main() {
    let code = if let Err(e) = run() {
        eprintln!("vibe-hook: {e}");
        1
    } else {
        0
    };
    std::process::exit(code);
}

fn run() -> Result<()> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let source = parse_source(&mut args)?;
    let mut stdin = String::new();
    io::stdin().read_to_string(&mut stdin)?;
    let raw: Value = if stdin.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&stdin).context("invalid hook stdin json")?
    };
    let ev = normalize(&source, &raw);

    let port = read_port()?;
    let url = format!("http://127.0.0.1:{port}/api/events");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;
    let resp = client.post(&url).json(&ev).send()?;
    if !resp.status().is_success() {
        bail!(
            "POST {url} failed with status {}",
            resp.status()
        );
    }
    Ok(())
}

fn parse_source(args: &mut Vec<String>) -> Result<String> {
    let mut source = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--source" && i + 1 < args.len() {
            source = Some(args[i + 1].clone());
            args.remove(i + 1);
            args.remove(i);
            continue;
        }
        i += 1;
    }
    source.context("missing --source (cursor|claude|codex)")
}

fn read_port() -> Result<u16> {
    let dir = directories::ProjectDirs::from("com", "VibeMonitor", "vibe-monitor")
        .context("data dir")?
        .data_dir()
        .to_path_buf();
    let port_file = dir.join("port");
    if port_file.exists() {
        if let Ok(s) = std::fs::read_to_string(&port_file) {
            if let Ok(p) = s.trim().parse() {
                return Ok(p);
            }
        }
    }
    Ok(17392)
}

fn normalize(source: &str, raw: &Value) -> vibe_payload::NormalizedEvent {
    let src = match source {
        "cursor" => "cursor",
        "claude" | "claude-code" | "claude_code" => "claude_code",
        "codex" => "codex",
        _ => "cursor",
    };
    let event_name = raw
        .get("hook_event_name")
        .or_else(|| raw.get("event"))
        .or_else(|| raw.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let session_id = raw
        .get("session_id")
        .or_else(|| raw.get("conversation_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let cwd = raw
        .get("cwd")
        .or_else(|| raw.get("workspace"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let tool_name = raw
        .get("tool_name")
        .or_else(|| raw.get("tool"))
        .or_else(|| raw.get("tool_type"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let prompt = raw
        .get("prompt")
        .or_else(|| raw.get("user_message"))
        .or_else(|| raw.get("message"))
        .and_then(|v| v.as_str());

    let task_title = prompt.map(truncate_redact);

    vibe_payload::NormalizedEvent {
        source: src.to_string(),
        event_name: event_name.clone(),
        session_id,
        cwd,
        task_title,
        tool_name,
        raw_hint: Some(event_name),
    }
}

fn truncate_redact(input: &str) -> String {
    let lower = input.to_lowercase();
    if lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("sk-")
        || lower.contains("secret")
    {
        return "[redacted]".to_string();
    }
    input.chars().take(120).collect::<String>().trim().to_string()
}

mod vibe_payload {
    use serde::Serialize;

    #[derive(Serialize)]
    pub struct NormalizedEvent {
        pub source: String,
        pub event_name: String,
        pub session_id: Option<String>,
        pub cwd: Option<String>,
        pub task_title: Option<String>,
        pub tool_name: Option<String>,
        pub raw_hint: Option<String>,
    }
}
