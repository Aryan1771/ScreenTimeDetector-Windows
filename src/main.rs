mod platform;
mod storage;

use chrono::Local;
use directories::ProjectDirs;
use slint::{ModelRc, TimerMode, VecModel};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use sysinfo::{Pid, System};

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dirs = ProjectDirs::from("org", "DigitalWellbeing", "DigitalWellbeing")
        .ok_or("could not locate per-user data directory")?;
    std::fs::create_dir_all(dirs.data_dir())?;
    let db_path = dirs.data_dir().join("wellbeing.sqlite3");
    let db = storage::Database::open(&db_path)?;
    let monitoring_supported = platform::supported();
    let tracking = Arc::new(AtomicBool::new(true));
    let focus = Arc::new(std::sync::Mutex::new(None::<Instant>));
    start_tracker(db_path.clone(), tracking.clone());

    let window = AppWindow::new()?;
    window.set_tracking(true);
    window.set_status_text(
        if monitoring_supported {
            "Tracking is active"
        } else {
            "Global app tracking is unavailable in this session"
        }
        .into(),
    );
    window.set_usages(ModelRc::new(VecModel::from(Vec::<UsageItem>::new())));

    {
        let weak = window.as_weak();
        let state = tracking.clone();
        let supported = monitoring_supported;
        window.on_toggle_tracking(move || {
            let next = !state.load(Ordering::Relaxed);
            state.store(next, Ordering::Relaxed);
            if let Some(window) = weak.upgrade() {
                window.set_tracking(next);
                window.set_status_text(
                    if !supported {
                        "Global app tracking is unavailable in this session"
                    } else if next {
                        "Tracking is active"
                    } else {
                        "Tracking is paused"
                    }
                    .into(),
                );
            }
        });
    }
    {
        let weak = window.as_weak();
        let active = focus.clone();
        window.on_toggle_focus(move || {
            let mut session = active.lock().expect("focus mutex poisoned");
            if session.is_some() {
                *session = None;
            } else {
                *session = Some(Instant::now() + Duration::from_secs(25 * 60));
            }
            if let Some(window) = weak.upgrade() {
                window.set_focus_active(session.is_some());
                window.set_focus_text(match *session {
                    Some(until) => format!(
                        "Focus session · {} minutes remaining",
                        until
                            .saturating_duration_since(Instant::now())
                            .as_secs()
                            .div_ceil(60)
                    )
                    .into(),
                    None => "No focus session".into(),
                });
            }
        });
    }
    {
        let weak = window.as_weak();
        window.on_navigate(move |page| {
            if let Some(w) = weak.upgrade() {
                w.set_page(page);
            }
        });
    }
    {
        let path = db_path.clone();
        window.on_export_data(move || {
            if let Some(out) = directories::UserDirs::new().map(|d| {
                d.download_dir()
                    .unwrap_or(d.home_dir())
                    .join("digital-wellbeing-usage.csv")
            }) {
                if let Ok(db) = storage::Database::open(&path) {
                    let _ = db.export_csv(&out);
                }
            }
        });
    }
    {
        let path = db_path.clone();
        let weak = window.as_weak();
        window.on_set_goal(move |minutes| {
            if let Ok(db) = storage::Database::open(&path) {
                let goal = i64::from(minutes);
                let _ = db.set_goal_minutes(goal);
                if let Some(w) = weak.upgrade() {
                    w.set_goal_label(format!("{}h {:02}m", goal / 60, goal % 60).into());
                }
            }
        });
    }

    let weak = window.as_weak();
    let refresh_path = db_path;
    let focus_state = focus.clone();
    let timer = slint::Timer::default();
    if let Ok(goal) = db.goal_minutes() {
        window.set_goal_label(format!("{}h {:02}m", goal / 60, goal % 60).into());
        window.set_goal_minutes(goal.clamp(15, 720) as i32);
    }
    timer.start(TimerMode::Repeated, Duration::from_secs(5), move || {
        let Some(w) = weak.upgrade() else {
            return;
        };
        if let Ok(db) = storage::Database::open(&refresh_path) {
            if let Ok((total, rows)) = db.today_summary() {
                w.set_today_total(format_duration(total).into());
                let max = rows.iter().map(|r| r.1).max().unwrap_or(1).max(1);
                let items = rows
                    .into_iter()
                    .map(|(name, secs)| UsageItem {
                        name: name.into(),
                        duration: format_duration(secs).into(),
                        share: ((secs * 100 / max).clamp(3, 100)) as i32,
                    })
                    .collect::<Vec<_>>();
                w.set_usages(ModelRc::new(VecModel::from(items)));
            }
        }
        if let Ok(mut f) = focus_state.lock() {
            if let Some(until) = *f {
                if Instant::now() >= until {
                    *f = None;
                    w.set_focus_active(false);
                    w.set_focus_text("Focus session complete".into());
                } else {
                    w.set_focus_text(
                        format!(
                            "Focus session · {} minutes remaining",
                            until
                                .saturating_duration_since(Instant::now())
                                .as_secs()
                                .div_ceil(60)
                        )
                        .into(),
                    );
                }
            }
        }
    });
    window.run()?;
    Ok(())
}

fn start_tracker(db_path: PathBuf, tracking: Arc<AtomicBool>) {
    thread::Builder::new()
        .name("usage-tracker".into())
        .spawn(move || {
            let mut sys = System::new();
            let db = storage::Database::open(&db_path).ok();
            let mut last = Instant::now();
            loop {
                thread::sleep(Duration::from_secs(5));
                let now = Instant::now();
                let seconds = now.duration_since(last).as_secs().min(10) as i64;
                last = now;
                if !tracking.load(Ordering::Relaxed) {
                    continue;
                }
                if platform::idle_millis().is_none_or(|idle| idle > 60_000) {
                    continue;
                }
                let Some((pid, title)) = platform::foreground_window() else {
                    continue;
                };
                let target_pid = Pid::from_u32(pid);
                sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[target_pid]), true);
                let name = sys
                    .process(target_pid)
                    .map(|p| p.name().to_string_lossy().into_owned())
                    .unwrap_or_else(|| title.clone());
                let id = name.to_lowercase();
                if let Some(db) = db.as_ref() {
                    let _ = db.add_usage(&Local::now().date_naive(), &id, &name, seconds);
                }
                let _ = title;
            }
        })
        .expect("failed to start usage tracker");
}

fn format_duration(seconds: i64) -> String {
    if seconds >= 3600 {
        format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}s", seconds)
    }
}
