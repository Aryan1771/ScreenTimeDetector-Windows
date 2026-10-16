#[path = "../src/platform.rs"]
mod platform;
use sysinfo::{ProcessesToUpdate, System};
fn main() {
    let mut s = System::new();
    s.refresh_processes(ProcessesToUpdate::All, true);
    println!(
        "foreground {:?}; idle {:?}",
        platform::foreground_window(),
        platform::idle_millis()
    );
    for (id, p) in s.processes() {
        if p.name().to_string_lossy().contains("digital-wellbeing") {
            println!("host {} path {:?}", id, p.exe());
            let mut current = p.parent();
            for _ in 0..4 {
                if let Some(pid) = current {
                    if let Some(proc) = s.process(pid) {
                        println!(" parent {} {}", pid, proc.name().to_string_lossy());
                        current = proc.parent();
                    } else {
                        break;
                    }
                }
            }
        }
    }
    let p = directories::ProjectDirs::from("org", "DigitalWellbeing", "DigitalWellbeing")
        .unwrap()
        .data_dir()
        .join("wellbeing.sqlite3");
    let c = rusqlite::Connection::open(&p).unwrap();
    println!("db {}", p.display());
    for sql in [
        "PRAGMA integrity_check",
        "SELECT key||'='||value FROM preferences WHERE key='tracking_enabled'",
        "SELECT domain||' '||SUM(active_seconds) FROM website_usage GROUP BY domain",
    ] {
        let mut stmt = c.prepare(sql).unwrap();
        for row in stmt.query_map([], |r| r.get::<_, String>(0)).unwrap() {
            println!("{}", row.unwrap());
        }
    }
    let install = std::path::Path::new(r"C:\Users\aryan\AppData\Local\Programs\DigitalWellbeing");
    println!(
        "installed manifest {}",
        std::fs::read_to_string(install.join("browser-bridge-manifest.json")).unwrap()
    );
    let js = std::fs::read_to_string(install.join("browser/chromium/background.js")).unwrap();
    println!("normalizes www: {}", js.contains("replace(/^www"));
    println!(
        "same exe {}",
        std::fs::read(install.join("digital-wellbeing.exe")).unwrap()
            == std::fs::read("target/release/digital-wellbeing.exe").unwrap()
    );
}
