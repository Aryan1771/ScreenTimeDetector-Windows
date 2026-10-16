use std::io::{self, Read, Write};

use chrono::{Local, Timelike};
use directories::ProjectDirs;
use serde_json::{Value, json};
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::platform;
use crate::storage::Database;

const MAX_MESSAGE_SIZE: usize = 1024 * 1024;
const HOST_NAME: &str = "com.digitalwellbeing.browser_bridge";
const CHROME_EXTENSION_ORIGIN: &str = "chrome-extension://mafmhbdjfjllohclegecbcnonbgkakgl/";

pub fn is_host_argument(argument: &std::ffi::OsStr) -> bool {
    argument == HOST_NAME
        || argument == "--native-messaging-host"
        || argument == CHROME_EXTENSION_ORIGIN
}

pub fn run(
    data_dir_override: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let dirs = ProjectDirs::from("org", "DigitalWellbeing", "DigitalWellbeing")
        .ok_or("could not locate per-user data directory")?;
    let data_dir = data_dir_override.unwrap_or_else(|| dirs.data_dir().to_path_buf());
    std::fs::create_dir_all(&data_dir)?;
    let db_path = data_dir.join("wellbeing.sqlite3");
    let db = Database::open(&db_path)?;
    let browser_pid = native_browser_pid();
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    loop {
        let mut header = [0_u8; 4];
        match input.read(&mut header[..1])? {
            0 => return Ok(()),
            1 => input.read_exact(&mut header[1..])?,
            _ => unreachable!(),
        }
        let length = u32::from_le_bytes(header) as usize;
        if length == 0 || length > MAX_MESSAGE_SIZE {
            write_response(&mut output, &json!({"ok": false}))?;
            if length > MAX_MESSAGE_SIZE {
                return Err(format!("native message exceeds {MAX_MESSAGE_SIZE} bytes").into());
            }
            continue;
        }
        let mut bytes = vec![0; length];
        input.read_exact(&mut bytes)?;
        let result = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .is_some_and(|message| record_message(&db, &message, browser_pid));
        write_response(&mut output, &json!({"ok": result}))?;
    }
}

fn record_message(db: &Database, message: &Value, browser_pid: Option<u32>) -> bool {
    record_message_with_foreground(db, message, browser_is_foreground(browser_pid))
}

fn record_message_with_foreground(db: &Database, message: &Value, foreground: bool) -> bool {
    let Some((domain, seconds)) = parse_website_usage(message) else {
        return false;
    };
    if !foreground || db.preference("tracking_enabled", "true").as_deref() != Ok("true") {
        return false;
    }
    let now = Local::now();
    db.add_website_usage(&now.date_naive(), now.hour(), &domain, seconds)
        .is_ok()
}

fn parse_website_usage(message: &Value) -> Option<(String, i64)> {
    if message.get("type").and_then(Value::as_str) != Some("website_usage") {
        return None;
    }
    if message
        .get("incognito")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let domain = validate_domain(message.get("domain")?.as_str()?)?;
    let seconds = message
        .get("seconds")?
        .as_i64()
        .filter(|seconds| (1..=15).contains(seconds))?;
    Some((domain, seconds))
}

fn browser_is_foreground(browser_pid: Option<u32>) -> bool {
    if platform::idle_millis().is_none_or(|idle| idle > 60_000) {
        return false;
    }
    let Some((pid, _)) = platform::foreground_window() else {
        return false;
    };
    // A Chrome bridge must not credit its last tab while Opera (or another
    // browser instance) is foreground. Bind the pipe to its launching process.
    browser_pid == Some(pid)
}

fn native_browser_pid() -> Option<u32> {
    let mut system = System::new();
    let own_pid = Pid::from_u32(std::process::id());
    system.refresh_processes(ProcessesToUpdate::Some(&[own_pid]), true);
    let mut pid = system.process(own_pid)?.parent()?;
    // Some Chromium builds launch native hosts through a hidden cmd.exe pipe
    // wrapper. Follow that wrapper without accepting unrelated parent apps.
    for _ in 0..4 {
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        let process = system.process(pid)?;
        let name = process.name().to_string_lossy().to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "chrome.exe"
                | "msedge.exe"
                | "brave.exe"
                | "opera.exe"
                | "opera_gx.exe"
                | "vivaldi.exe"
        ) {
            return Some(pid.as_u32());
        }
        if name != "cmd.exe" {
            return None;
        }
        pid = process.parent()?;
    }
    None
}

fn validate_domain(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 253 || !value.is_ascii() {
        return None;
    }
    let normalized = value.to_ascii_lowercase();
    let normalized = normalized
        .strip_prefix("www.")
        .unwrap_or(&normalized)
        .to_owned();
    if normalized.split('.').any(|label| {
        label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    }) {
        return None;
    }
    Some(normalized)
}

fn write_response(output: &mut impl Write, value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let length = u32::try_from(bytes.len())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    output.write_all(&length.to_le_bytes())?;
    output.write_all(&bytes)?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_validation_keeps_only_a_bounded_hostname() {
        assert_eq!(
            validate_domain("Docs.Example.COM"),
            Some("docs.example.com".into())
        );
        assert_eq!(
            validate_domain("WWW.YouTube.COM"),
            Some("youtube.com".into())
        );
        for invalid in [
            "",
            "https://example.com/path",
            "example.com/path",
            "bad..domain",
            "-bad.example",
            "bad-.example",
        ] {
            assert_eq!(validate_domain(invalid), None, "{invalid}");
        }
        assert_eq!(validate_domain(&"a".repeat(254)), None);
    }

    #[test]
    fn host_accepts_only_valid_bounded_domain_samples() {
        assert_eq!(
            parse_website_usage(
                &json!({"type":"website_usage","domain":"Example.com","seconds":5})
            ),
            Some(("example.com".into(), 5))
        );
        for invalid in [
            json!({"type":"website_usage","domain":"example.com/path","seconds":5}),
            json!({"type":"website_usage","domain":"example.com","seconds":0}),
            json!({"type":"website_usage","domain":"example.com","seconds":16}),
            json!({"type":"website_usage","domain":"example.com","seconds":5,"incognito":true}),
            json!({"type":"history","domain":"example.com","seconds":5}),
        ] {
            assert_eq!(parse_website_usage(&invalid), None);
        }
    }

    #[test]
    fn website_rows_require_browser_foreground() {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        let message = json!({"type":"website_usage","domain":"example.com","seconds":5});
        let day = Local::now().date_naive();
        assert!(!record_message_with_foreground(&db, &message, false));
        assert!(db.website_summary_for_day(&day).unwrap().is_empty());
        assert!(record_message_with_foreground(&db, &message, true));
        assert_eq!(
            db.website_summary_for_day(&day).unwrap(),
            vec![("example.com".into(), 5)]
        );
        db.set_preference("tracking_enabled", "false").unwrap();
        assert!(!record_message_with_foreground(&db, &message, true));
        assert_eq!(db.website_summary_for_day(&day).unwrap()[0].1, 5);
    }
}
